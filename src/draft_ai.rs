//! M11 draft AI baselines (#106, building on #104's observation catalog).
//!
//! Three observation-only baselines so draft decisions can be compared:
//!
//! * [`DraftAiKind::Random`] - a seeded uniform choice.
//! * [`DraftAiKind::CardValue`] - a card's declared points, declared cost and
//!   the number of implemented effect categories.
//! * [`DraftAiKind::Synergy`] - `CardValue` plus affinity/coverage over the
//!   public effect tags of the cards the viewer already picked.
//!
//! Every policy reads **only** a [`DraftObservation`]. Opponent hands, the
//! private seed, the deck order and unobserved cards are not in that type. An
//! unknown card (no catalog entry) scores `0` and is still a legal pick, so an
//! incomplete or entirely missing catalog degrades to a legality fallback
//! instead of a guessed value. Because these baselines never read a card's
//! *text*, they cannot value an unimplemented effect.

use crate::draft::{
    CardKind, DraftCardInfo, DraftMode, DraftObservation, DraftPhase, DraftSetup, DraftState,
};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The three comparable draft baselines. There is no silent default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftAiKind {
    Random,
    CardValue,
    Synergy,
}

impl DraftAiKind {
    pub fn id(self) -> &'static str {
        match self {
            DraftAiKind::Random => "random",
            DraftAiKind::CardValue => "card_value",
            DraftAiKind::Synergy => "synergy",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Random, Self::CardValue, Self::Synergy]
    }

    fn salt(self) -> u64 {
        match self {
            DraftAiKind::Random => 0x11,
            DraftAiKind::CardValue => 0x22,
            DraftAiKind::Synergy => 0x33,
        }
    }

    /// Choose the current round's pick(s) for 7-7 or 10-7 simultaneous.
    pub fn decide_pick(
        self,
        obs: &DraftObservation,
        rng: &mut ChaCha8Rng,
    ) -> Result<DraftDecision, String> {
        if obs.phase != DraftPhase::Drafting {
            return Err("draft AI is not in a picking phase".into());
        }
        let ranked_occupation = self.rank(obs, CardKind::Occupation);
        let ranked_minor = self.rank(obs, CardKind::MinorImprovement);
        let mut occupation = None;
        let mut minor = None;
        match obs.mode {
            DraftMode::Draft10_7Simultaneous => {
                occupation =
                    Some(self.pick_one(obs, CardKind::Occupation, &ranked_occupation, rng)?);
                minor = Some(self.pick_one(obs, CardKind::MinorImprovement, &ranked_minor, rng)?);
            }
            DraftMode::Draft77 => match obs.active_kind {
                Some(CardKind::Occupation) => {
                    occupation =
                        Some(self.pick_one(obs, CardKind::Occupation, &ranked_occupation, rng)?)
                }
                Some(CardKind::MinorImprovement) => {
                    minor =
                        Some(self.pick_one(obs, CardKind::MinorImprovement, &ranked_minor, rng)?)
                }
                None => return Err("draft round has no active kind".into()),
            },
            _ => return Err("this mode does not use round picks".into()),
        }
        let reason = format!(
            "{}: occupation={} minor={}",
            self.id(),
            occupation.as_deref().unwrap_or("-"),
            minor.as_deref().unwrap_or("-")
        );
        Ok(DraftDecision {
            occupation,
            minor,
            reason,
            ranked: ranked_occupation
                .into_iter()
                .chain(ranked_minor)
                .collect::<Vec<_>>(),
        })
    }

    /// Choose 7 of 10 for both kinds in the non-draft 10-7 mode.
    pub fn decide_choice(
        self,
        obs: &DraftObservation,
        rng: &mut ChaCha8Rng,
    ) -> Result<DraftChoiceDecision, String> {
        if obs.mode != DraftMode::Choose10_7 || obs.phase != DraftPhase::Choosing {
            return Err("draft AI choice is only for the 10-7 non-draft mode".into());
        }
        let ranked_occupations = self.rank(obs, CardKind::Occupation);
        let ranked_minors = self.rank(obs, CardKind::MinorImprovement);
        if ranked_occupations.len() < 7 || ranked_minors.len() < 7 {
            return Err("the 10-7 choice needs at least seven offered cards per kind".into());
        }
        let occupations = self.take_seven(obs, CardKind::Occupation, &ranked_occupations, rng)?;
        let minors = self.take_seven(obs, CardKind::MinorImprovement, &ranked_minors, rng)?;
        let reason = format!("{}: kept seven of each kind", self.id());
        Ok(DraftChoiceDecision {
            occupations,
            minors,
            reason,
            ranked_occupations,
            ranked_minors,
        })
    }

    /// Score every offered card of one kind. For `Random` the list keeps the
    /// offered order and a neutral score; the other policies sort best-first
    /// (ties keep the offered order, so the result is deterministic).
    pub fn rank(self, obs: &DraftObservation, kind: CardKind) -> Vec<ScoredCard> {
        let mut scored = offered(obs, kind)
            .iter()
            .map(|id| ScoredCard {
                id: id.clone(),
                kind,
                score: self.card_score(obs, id),
            })
            .collect::<Vec<_>>();
        if self != DraftAiKind::Random {
            scored.sort_by_key(|card| std::cmp::Reverse(card.score));
        }
        scored
    }

    fn card_score(self, obs: &DraftObservation, id: &str) -> i64 {
        match self {
            DraftAiKind::Random => 0,
            DraftAiKind::CardValue => base_value(obs.card_info(id)),
            DraftAiKind::Synergy => synergy_value(obs, id),
        }
    }

    fn pick_one(
        self,
        obs: &DraftObservation,
        kind: CardKind,
        ranked: &[ScoredCard],
        rng: &mut ChaCha8Rng,
    ) -> Result<String, String> {
        let offered = offered(obs, kind);
        if offered.is_empty() {
            return Err(format!("draft AI has no offered {} cards", kind.as_str()));
        }
        if self == DraftAiKind::Random {
            return Ok(offered
                .choose(rng)
                .cloned()
                .expect("non-empty offered list"));
        }
        ranked
            .first()
            .map(|card| card.id.clone())
            .ok_or_else(|| "draft AI could not score any offered card".into())
    }

    fn take_seven(
        self,
        obs: &DraftObservation,
        kind: CardKind,
        ranked: &[ScoredCard],
        rng: &mut ChaCha8Rng,
    ) -> Result<Vec<String>, String> {
        if self == DraftAiKind::Random {
            let mut deck = offered(obs, kind).to_vec();
            deck.shuffle(rng);
            deck.truncate(7);
            return Ok(deck);
        }
        Ok(ranked.iter().take(7).map(|card| card.id.clone()).collect())
    }
}

/// One diagnosable draft pick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftDecision {
    pub occupation: Option<String>,
    pub minor: Option<String>,
    pub reason: String,
    /// Ranked candidate scores, best first. Contains only offered cards.
    pub ranked: Vec<ScoredCard>,
}

/// One diagnosable 10-7 non-draft choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftChoiceDecision {
    pub occupations: Vec<String>,
    pub minors: Vec<String>,
    pub reason: String,
    pub ranked_occupations: Vec<ScoredCard>,
    pub ranked_minors: Vec<ScoredCard>,
}

/// A candidate and the score a policy gave it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoredCard {
    pub id: String,
    pub kind: CardKind,
    pub score: i64,
}

/// A single evaluated seat's result, for seat-rotation comparison.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSeatOutcome {
    pub ai: DraftAiKind,
    pub seat: usize,
    pub occupations: Vec<String>,
    pub minors: Vec<String>,
}

/// Run a whole draft with one AI per seat. Each decision uses a deterministic
/// RNG derived from the run seed, the seat and the draft event index, so a
/// fixed seed reproduces the exact same result.
pub fn run_draft(
    setup: &DraftSetup,
    seat_ais: &[DraftAiKind],
    seed: u64,
    catalog: &[DraftCardInfo],
) -> Result<DraftState, String> {
    if seat_ais.len() != setup.player_count {
        return Err("seat AI list must match the draft player count".into());
    }
    let mut state = setup.start()?;
    match state.mode {
        DraftMode::Deal7 => return Ok(state),
        DraftMode::Choose10_7 => {
            for (seat, &ai) in seat_ais.iter().enumerate() {
                let token = state.current_token();
                let obs = state.observe_with_catalog(seat, token, catalog)?;
                let decision = ai.decide_choice(&obs, &mut decision_rng(seed, seat, token, ai))?;
                state.commit_choice(seat, token, decision.occupations, decision.minors)?;
            }
        }
        _ => {
            let mut guard = 0;
            while !state.is_complete() {
                guard += 1;
                if guard > 128 {
                    return Err("draft did not terminate".into());
                }
                let token = state.current_token();
                for (seat, &ai) in seat_ais.iter().enumerate() {
                    let obs = state.observe_with_catalog(seat, token, catalog)?;
                    let decision =
                        ai.decide_pick(&obs, &mut decision_rng(seed, seat, token, ai))?;
                    state.commit_pick(seat, token, decision.occupation, decision.minor)?;
                }
            }
        }
    }
    Ok(state)
}

/// Compare all three baselines at every seat on the same setup and seed. The
/// evaluated AI occupies one seat and `Random` fills the rest as a fixed
/// reference, so the comparison can be repeated with the seat rotated.
///
/// This is a *functional* comparison only. Without a complete verified card
/// pool and full games, it does not measure real playing strength.
pub fn compare_draft_ais(
    setup: &DraftSetup,
    catalog: &[DraftCardInfo],
    seed: u64,
) -> Result<Vec<DraftSeatOutcome>, String> {
    let players = setup.player_count;
    let mut outcomes = Vec::new();
    for ai in DraftAiKind::all() {
        for seat in 0..players {
            let seat_ais = (0..players)
                .map(|other| {
                    if other == seat {
                        ai
                    } else {
                        DraftAiKind::Random
                    }
                })
                .collect::<Vec<_>>();
            let state = run_draft(setup, &seat_ais, seed, catalog)?;
            let (occupations, minors) = state
                .final_hands()
                .into_iter()
                .nth(seat)
                .ok_or("draft seat missing")?;
            outcomes.push(DraftSeatOutcome {
                ai,
                seat,
                occupations,
                minors,
            });
        }
    }
    Ok(outcomes)
}

/// Deterministic per-decision RNG for a seat. Exposed so a host (for example
/// the browser session) reproduces the exact same CPU picks for a fixed seed.
pub fn decision_rng(seed: u64, seat: usize, event_index: u64, ai: DraftAiKind) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(
        seed ^ ((seat as u64) << 32) ^ event_index.rotate_left(17) ^ ai.salt(),
    )
}

fn offered(obs: &DraftObservation, kind: CardKind) -> &[String] {
    match kind {
        CardKind::Occupation => &obs.offered_occupation,
        CardKind::MinorImprovement => &obs.offered_minor,
    }
}

/// Standalone value of a card from public metadata only. `None` means the card
/// has no catalog entry and must not be guessed.
fn base_value(info: Option<&DraftCardInfo>) -> i64 {
    let Some(info) = info else {
        return 0;
    };
    let cost = info
        .cost
        .iter()
        .map(|(good, quantity)| good_weight(good) * i64::from(*quantity))
        .sum::<i64>();
    i64::from(info.points) * 30 + info.effect_tags.len() as i64 * 8 - cost
}

/// `CardValue` plus a documented affinity/coverage term over the public effect
/// tags of the cards this viewer already picked.
fn synergy_value(obs: &DraftObservation, id: &str) -> i64 {
    let base = base_value(obs.card_info(id));
    let Some(info) = obs.card_info(id) else {
        return base;
    };
    let own = own_tag_counts(obs);
    let mut score = base;
    for tag in &info.effect_tags {
        let seen = own.get(tag).copied().unwrap_or(0);
        score += 6 * i64::from(seen.min(3));
    }
    let new_tags = info
        .effect_tags
        .iter()
        .filter(|tag| !own.contains_key(*tag))
        .count();
    score + 2 * new_tags as i64
}

fn own_tag_counts(obs: &DraftObservation) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for id in &obs.own_picked {
        if let Some(info) = obs.card_info(id) {
            for tag in &info.effect_tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn good_weight(good: &str) -> i64 {
    match good {
        "reed" => 2,
        "stone" => 3,
        "grain" => 2,
        "vegetable" => 3,
        _ => 1,
    }
}

//! Loopback-only, single-user web UI. No external services or accounts.
use crate::breeding::BreedingPlan;
use crate::cards::{CardDefinition, CardSet};
use crate::draft::{
    CardKind, Direction, DraftCardInfo, DraftMode, DraftPhase, DraftPool, DraftRuleset, DraftSetup,
    DraftState,
};
use crate::draft_ai::{decision_rng, DraftAiKind};
use crate::draft_game::{card_catalog, pool_from_card_set, start_game_from_draft};
use crate::engine::{Action, Detail, FreeAction};
use crate::harvest::{FeedingPlan, HarvestDetailPolicy};

use crate::evaluation::{DetailAwareEvaluationPolicy, WeightedEvaluationPolicy};
use crate::mcts::MctsPolicy;
use crate::model::{Config, Game, Observation, Space};
use crate::policy::{CardChoiceDetails, DetailPolicy, SimpleDetails, SpacePolicy};
use crate::score::standings_with_cards;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

const INDEX: &str = include_str!("../web/index.html");
const MAX_BODY: usize = 65_536;
/// Browser game logs grow throughout fourteen rounds. Loading uses a larger,
/// still bounded limit; ordinary POST endpoints retain the smaller limit.
const MAX_LOAD_BODY: usize = 8 * 1024 * 1024;

fn request_body_limit(path: &str) -> usize {
    if path == "/api/load" {
        MAX_LOAD_BODY
    } else {
        MAX_BODY
    }
}
/// Marker for a browser save file. A file without it is rejected instead of
/// being interpreted as some other JSON.
const SAVE_FORMAT: &str = "agricola-web-save";
/// On-disk version of the save envelope. Bumped when the envelope changes; the
/// inner `Game` save has its own version checks in `Game::load_json`.
const SAVE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone)]
pub struct Session {
    game: Game,
    mcts: bool,
    budget: usize,
    pending: Option<PendingHarvest>,
    draft: Option<DraftWeb>,
}

/// A server-owned draft session. The authoritative state stays on the server;
/// the browser only ever receives `DraftObservation` for the human seat.
#[derive(Clone)]
struct DraftWeb {
    setup: DraftSetup,
    state: DraftState,
    /// Card set the draft's vocabulary belongs to. Test fixtures and the
    /// implemented ("verified basic") pool are never mixed here.
    card_set: CardSet,
    /// Machine-readable pool provenance shown to players and saved with the
    /// session: `test_fixture` or `verified_basic`.
    pool_kind: String,
    /// Public card vocabulary attached to every viewer's observation.
    catalog: Vec<DraftCardInfo>,
    /// Set once the completed draft has seeded a real game.
    game_started: bool,
    /// Observation-only AI used for the non-human seats.
    draft_ai: DraftAiKind,
}

#[derive(Clone)]
enum PendingHarvest {
    Feeding(Action),
    Breeding(Action, Vec<FeedingPlan>),
}

struct ManualHarvest {
    feeding: Vec<FeedingPlan>,
    breeding: Vec<BreedingPlan>,
}
impl HarvestDetailPolicy for ManualHarvest {
    fn plan_feeding(&self, obs: &Observation) -> FeedingPlan {
        self.feeding[obs.player_id].clone()
    }
    fn plan_breeding(&self, obs: &Observation) -> BreedingPlan {
        self.breeding[obs.player_id].clone()
    }
}
impl Session {
    pub fn new(config: Config) -> Result<Self, String> {
        let mut session = Self {
            game: Game::new(config),
            mcts: false,
            budget: 8,
            pending: None,
            draft: None,
        };
        session.advance_cpu()?;
        Ok(session)
    }
    fn queue_or_apply(&mut self, action: Action) -> Result<(), String> {
        if self.game.will_harvest_after_work() {
            self.game.preview_harvest(&action, None)?;
            self.pending = Some(PendingHarvest::Feeding(action));
        } else {
            self.game.apply(action)?;
        }
        Ok(())
    }

    pub fn advance_cpu(&mut self) -> Result<(), String> {
        let details: &dyn DetailPolicy = if self.game.card_set.enabled() {
            &CardChoiceDetails
        } else {
            &SimpleDetails
        };
        let evaluation = DetailAwareEvaluationPolicy {
            weights: WeightedEvaluationPolicy {
                prior_weight: 0.5,
                state_weight: 2.0,
            },
            details,
        };
        for _ in 0..320 {
            if self.pending.is_some()
                || self.game.public.finished
                || self.game.public.active_player == 0
            {
                return Ok(());
            }
            let obs = self.game.observation(self.game.public.active_player)?;
            let legal = self.game.legal_spaces();
            let event_index = self.game.log.events.len() as u64;
            let mut rng = ChaCha8Rng::seed_from_u64(
                self.game.config.seed ^ event_index.wrapping_mul(0x9e37_79b9_7f4a_7c15),
            );
            let space = if self.mcts {
                MctsPolicy {
                    simulations: self.budget,
                    tree_depth: 2,
                }
                .select(&obs, &legal, &mut rng)
            } else {
                evaluation.select(&obs, &legal, &mut rng)
            }
            .ok_or("CPU could not select an action")?;
            let action = details
                .produce(&obs, space)
                .ok_or("CPU could not make action details")?;
            self.queue_or_apply(action)?;
        }
        Err("CPU turn count exceeded".into())
    }
    pub fn view(&self) -> Value {
        let human_turn = self.pending.is_none()
            && !self.game.public.finished
            && self.game.public.active_player == 0;
        let harvest = match &self.pending {
            None => Value::Null,
            Some(PendingHarvest::Feeding(action)) => json!({
                "phase": "feeding",
                "preview": self.game.preview_harvest(action, None).expect("validated action"),
            }),
            Some(PendingHarvest::Breeding(action, feeding)) => json!({
                "phase": "breeding",
                "preview": self.game.preview_harvest(action, Some(feeding)).expect("validated plans"),
            }),
        };
        let detail_hints = if human_turn {
            let mut map = serde_json::Map::new();
            for space in self.game.available_spaces() {
                if let Ok(Value::String(key)) = serde_json::to_value(space) {
                    if let Ok(value) = serde_json::to_value(self.game.detail_hints(space)) {
                        map.insert(key, value);
                    }
                }
            }
            Value::Object(map)
        } else {
            Value::Object(serde_json::Map::new())
        };
        let human_player = &self.game.public.players[0];
        let free_action_hints = if human_turn {
            serde_json::to_value(crate::hints::free_action_hints(human_player))
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let fence_hints = if human_turn {
            serde_json::to_value(self.game.fence_hints()).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let feeding_hints = match &self.pending {
            Some(PendingHarvest::Feeding(action)) => self
                .game
                .preview_harvest(action, None)
                .ok()
                .map(|preview| {
                    serde_json::to_value(crate::hints::feeding_hints(&preview.players[0]))
                        .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        let breeding_hints = match &self.pending {
            Some(PendingHarvest::Breeding(action, feeding)) => self
                .game
                .preview_harvest(action, Some(feeding))
                .ok()
                .map(|preview| {
                    serde_json::to_value(crate::hints::breeding_hints(&preview.players[0]))
                        .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        json!({
            "harvest": harvest,
            "hand": &self.game.cards.hands[0],
            "card_definitions": &self.game.card_set.definitions,
            "card_enabled": self.game.card_set.enabled(),
            "public": &self.game.public,
            "legal": if human_turn { self.game.legal_spaces() } else { vec![] },
            "available": if human_turn { self.game.available_spaces() } else { vec![] },
            "detail_hints": detail_hints,
            "free_action_hints": free_action_hints,
            "fence_hints": fence_hints,
            "feeding_hints": feeding_hints,
            "breeding_hints": breeding_hints,
            "standings": standings_with_cards(&self.game.public.players, &self.game.card_set),
            "last_actions": self.game.log.events.iter().rev().take(5).map(|e| json!({
                "player": e.player,
                "kind": &e.kind,
                "space": e.payload.get("Work").and_then(|w| w.get("action"))
                    .and_then(|a| a.get("space")),
            })).collect::<Vec<_>>(),
            "mcts": self.mcts,
            "seed": self.game.config.seed,
            "draft": self
                .draft
                .as_ref()
                .and_then(|draft| draft_json(draft, 0).ok())
                .unwrap_or(Value::Null),
        })
    }

    // ---- M11 draft foundation (Issue #104) --------------------------------

    fn draft(&self) -> Result<&DraftWeb, String> {
        self.draft.as_ref().ok_or("no draft session".into())
    }

    pub fn draft_view(&self) -> Result<Value, String> {
        draft_json(self.draft()?, 0)
    }

    /// Start a draft using the project-adopted clockwise direction for both
    /// kinds. The 7-7 phase order remains an explicit choice (#155).
    pub fn start_draft(&mut self, input: &Value) -> Result<(), String> {
        let mode = match input.get("mode").and_then(Value::as_str) {
            Some("deal_7") => DraftMode::Deal7,
            Some("draft_7_7") => DraftMode::Draft77,
            Some("draft_10_7_simultaneous") => DraftMode::Draft10_7Simultaneous,
            Some("choose_10_7") => DraftMode::Choose10_7,
            _ => return Err("unknown draft mode".into()),
        };
        let players = input.get("players").and_then(Value::as_u64).unwrap_or(2) as usize;
        if !(2..=4).contains(&players) {
            return Err("draft players must be 2, 3 or 4".into());
        }
        let direction_occupation = parse_direction(
            input.get("direction_occupation").and_then(Value::as_str),
            "direction_occupation",
        )?;
        let direction_minor = parse_direction(
            input.get("direction_minor").and_then(Value::as_str),
            "direction_minor",
        )?;
        let occupation_phase_first = match input.get("occupation_phase_first") {
            Some(value) => value
                .as_bool()
                .ok_or("occupation_phase_first must be a boolean")?,
            None if mode == DraftMode::Draft77 => {
                return Err("draft_7_7 requires an explicit occupation_phase_first".into())
            }
            None => true,
        };
        let seed = input.get("seed").and_then(Value::as_u64).unwrap_or(42);
        let pool_kind = input
            .get("pool")
            .and_then(Value::as_str)
            .unwrap_or("test_fixture");
        let (card_set, pool, deck_version, ruleset_id) = match pool_kind {
            "test_fixture" => {
                let pool = fixture_pool(mode, players);
                let set = fixture_card_set(&pool);
                (
                    set,
                    pool,
                    "test-fixture-v1".to_string(),
                    format!("fixture-clockwise-v2-{}", mode.id()),
                )
            }
            "verified_basic" => {
                // The implemented card pool the project already labels
                // "verified basic". Official card-by-card review is tracked by
                // #115/#139 and is not asserted here. The draft supply check
                // still gates the mode, so a mode needing more cards than exist
                // is rejected rather than padded with unverified cards.
                let set = crate::cards::verified_basic_card_set(players)?;
                let pool = pool_from_card_set(&set);
                (
                    set,
                    pool,
                    format!("verified-basic-{players}p-v1"),
                    format!("verified-basic-clockwise-v2-{}", mode.id()),
                )
            }
            other => return Err(format!("unknown draft pool kind: {other}")),
        };
        let draft_ai = match input.get("draft_ai").and_then(Value::as_str) {
            None | Some("random") => DraftAiKind::Random,
            Some("card_value") => DraftAiKind::CardValue,
            Some("synergy") => DraftAiKind::Synergy,
            Some(other) => return Err(format!("unknown draft AI: {other}")),
        };
        let catalog = card_catalog(&card_set);
        let setup = DraftSetup {
            mode,
            ruleset: DraftRuleset::explicit(
                ruleset_id,
                direction_occupation,
                direction_minor,
                occupation_phase_first,
            ),
            pool,
            player_count: players,
            seed,
            deck_version,
        };
        let state = setup.start()?;
        self.draft = Some(DraftWeb {
            setup,
            state,
            card_set,
            pool_kind: pool_kind.to_string(),
            catalog,
            game_started: false,
            draft_ai,
        });
        Ok(())
    }

    /// Seed a real game from the completed draft. The whole operation runs on a
    /// clone, so a rejected request (incomplete draft, wrong player count, or a
    /// fixture card missing from the card set) leaves the session untouched.
    pub fn start_draft_game(&mut self, input: &Value) -> Result<(), String> {
        let budget = input.get("budget").and_then(Value::as_u64).unwrap_or(8);
        if !(1..=256).contains(&budget) {
            return Err("MCTS simulations must be 1..=256".into());
        }
        let mut next = self.clone();
        let config = {
            let draft = next.draft.as_ref().ok_or("no draft session")?;
            if draft.game_started {
                return Err("a game was already started from this draft".into());
            }
            Config {
                players: draft.state.player_count,
                seed: input.get("seed").and_then(Value::as_u64).unwrap_or(42),
                additional_action_space_enabled: input
                    .get("extra")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        };
        let game = {
            let draft = next.draft.as_ref().ok_or("no draft session")?;
            start_game_from_draft(&draft.state, draft.card_set.clone(), config)?
        };
        next.game = game;
        next.pending = None;
        next.mcts = input.get("ai").and_then(Value::as_str) == Some("mcts");
        next.budget = budget as usize;
        if let Some(draft) = next.draft.as_mut() {
            draft.game_started = true;
        }
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }

    /// Human seat 0 commits one round pick; the CPU seats then follow.
    pub fn draft_pick(&mut self, input: &Value) -> Result<(), String> {
        let token = input
            .get("token")
            .and_then(Value::as_u64)
            .ok_or("draft token is required")?;
        let occupation = optional_string(input.get("occupation"))?;
        let minor = optional_string(input.get("minor"))?;
        // Whole request is transactional: the human commit and every CPU
        // follow-up either all apply or none do.
        let mut next = self.draft.clone().ok_or("no draft session")?;
        next.state.commit_pick(0, token, occupation, minor)?;
        auto_commit_cpus(&mut next)?;
        self.draft = Some(next);
        Ok(())
    }

    /// Human seat 0 finalises a 10-7 non-draft hand; the CPU seats follow.
    pub fn draft_choice(&mut self, input: &Value) -> Result<(), String> {
        let token = input
            .get("token")
            .and_then(Value::as_u64)
            .ok_or("draft token is required")?;
        let occupations = string_list(input.get("occupations"), "occupations")?;
        let minors = string_list(input.get("minors"), "minors")?;
        let mut next = self.draft.clone().ok_or("no draft session")?;
        next.state.commit_choice(0, token, occupations, minors)?;
        auto_commit_cpus(&mut next)?;
        self.draft = Some(next);
        Ok(())
    }

    pub fn reset_draft(&mut self) {
        self.draft = None;
    }

    // ---- M9 save / restore (Issue #92) ------------------------------------

    /// Points where a save cannot faithfully represent the session.
    ///
    /// A pending harvest holds a preview that is not part of `Game`, and an
    /// in-progress draft holds a separate state machine. Saving there would
    /// silently drop a decision, so the export is refused instead.
    fn save_blocker(&self) -> Option<&'static str> {
        if self.pending.is_some() {
            return Some(
                "収穫の給餌・繁殖を選択している間は保存できません。選択を完了してから保存してください。",
            );
        }
        // A finished draft still holds private observations, selection history
        // and its explicit ruleset outside Game. Until the save envelope can
        // authenticate and restore that state, refuse *all* draft-derived
        // saves instead of silently dropping the history on import.
        if self.draft.is_some() {
            return Some("ドラフト由来の対戦は履歴を復元できないため、現在は保存できません。通常対戦の保存をご利用ください。");
        }
        None
    }

    /// A self-contained save of the match, written by the browser.
    ///
    /// The payload embeds the engine's own save (`Game::save_json` shape) so a
    /// restored game is validated by the same code path as any other save: the
    /// event chain, the initial state, the rule/engine versions and the config
    /// must all agree. Hidden information is included on purpose — this is the
    /// player's own local file — so it must never be served by `/api/state`.
    pub fn export_save(&self) -> Result<Value, String> {
        if let Some(reason) = self.save_blocker() {
            return Err(reason.into());
        }
        let game = serde_json::to_value(&self.game).map_err(|error| error.to_string())?;
        Ok(json!({
            "format": SAVE_FORMAT,
            "schema_version": SAVE_SCHEMA_VERSION,
            "session": { "mcts": self.mcts, "budget": self.budget },
            "game": game,
        }))
    }

    /// File name offered to the browser.
    pub fn save_name(&self) -> String {
        format!(
            "agricola-{}p-seed{}-round{}.json",
            self.game.public.players.len(),
            self.game.config.seed,
            self.game.public.round
        )
    }

    /// Restore a save produced by [`Self::export_save`].
    ///
    /// Accepts either `{"save": <save>}` or the save object itself. The second
    /// form is what the browser sends: the file is posted verbatim, because a
    /// JavaScript `JSON.parse`/`JSON.stringify` round trip would silently round
    /// the 64-bit seeds to the nearest double.
    ///
    /// Transactional: the whole payload is validated into a copy first, so a
    /// corrupted or tampered file leaves the running match untouched.
    pub fn import_save(&mut self, input: &Value) -> Result<(), String> {
        let save = match input.get("save") {
            Some(save) => save,
            None if input.get("format").is_some() => input,
            None => return Err("save payload is required".into()),
        };
        let mut next = self.clone();
        next.restore_save(save)?;
        *self = next;
        Ok(())
    }

    fn restore_save(&mut self, save: &Value) -> Result<(), String> {
        if save.get("format").and_then(Value::as_str) != Some(SAVE_FORMAT) {
            return Err("アグリコラの保存ファイルではありません".into());
        }
        let version = save
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or("保存ファイルの schema_version がありません")?;
        if version != u64::from(SAVE_SCHEMA_VERSION) {
            return Err(format!(
                "保存ファイルの形式 {version} は未対応です（対応: {SAVE_SCHEMA_VERSION}）"
            ));
        }
        let game = save
            .get("game")
            .ok_or("保存ファイルにゲーム状態がありません")?;
        let text = serde_json::to_string(game).map_err(|error| error.to_string())?;
        // Reuses the engine's own validator: version/config/card-set metadata,
        // the initial state snapshot and every event hash are re-checked.
        let game =
            Game::load_json(&text).map_err(|error| format!("保存データが不正です: {error}"))?;
        let session = save.get("session").cloned().unwrap_or(Value::Null);
        let mcts = session
            .get("mcts")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let budget = session.get("budget").and_then(Value::as_u64).unwrap_or(8);
        if !(1..=256).contains(&budget) {
            return Err("保存ファイルのシミュレーション回数が不正です".into());
        }
        self.game = game;
        self.mcts = mcts;
        self.budget = budget as usize;
        // Neither a pending harvest nor a draft is part of the save envelope.
        self.pending = None;
        self.draft = None;
        Ok(())
    }
    pub fn reset(&mut self, input: &Value) -> Result<(), String> {
        let players = input.get("players").and_then(Value::as_u64).unwrap_or(2);
        if !(2..=4).contains(&players) {
            return Err("players must be 2, 3 or 4".into());
        }
        let budget = input.get("budget").and_then(Value::as_u64).unwrap_or(8);
        if !(1..=256).contains(&budget) {
            return Err("MCTS simulations must be 1..=256".into());
        }
        let config = Config {
            players: players as usize,
            seed: input.get("seed").and_then(Value::as_u64).unwrap_or(42),
            additional_action_space_enabled: input
                .get("extra")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        let card_set: CardSet = match input.get("preset").and_then(Value::as_str) {
            Some("verified") => crate::cards::verified_basic_card_set(config.players)?,
            Some(_) => return Err("unknown card preset".into()),
            None => match input.get("card_set") {
                Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                None => CardSet::default(),
            },
        };
        let mut next = Self {
            game: Game::with_cards(config, card_set)?,
            mcts: input.get("ai").and_then(Value::as_str) == Some("mcts"),
            budget: budget as usize,
            pending: None,
            // A fresh game resets the draft setup; the draft is a distinct
            // pre-game phase and must not leak into a new match.
            draft: None,
        };
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }
    pub fn play(&mut self, input: &Value) -> Result<(), String> {
        if self.pending.is_some()
            || self.game.public.finished
            || self.game.public.active_player != 0
        {
            return Err("not the human player's turn".into());
        }
        let space: Space =
            serde_json::from_value(input.get("space").cloned().ok_or("space is required")?)
                .map_err(|e| e.to_string())?;
        // Human input is always explicit. SimpleDetails remains CPU-only.
        let detail: Detail = serde_json::from_value(
            input
                .get("detail")
                .cloned()
                .ok_or("manual detail is required")?,
        )
        .map_err(|e| format!("invalid detail: {e}"))?;
        // Human and CPU share the engine's transactional validator. The
        // candidate-narrowed `legal_spaces` listing never defines legality.
        let action = Action { space, detail };
        self.game.validate_action(&action)?;
        let mut next = self.clone();
        next.queue_or_apply(action)?;
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }

    pub fn free(&mut self, input: &Value) -> Result<(), String> {
        if self.game.public.finished || self.pending.is_some() {
            return Err("free action not available during harvest/final results".into());
        }
        let action: FreeAction = serde_json::from_value(
            input
                .get("action")
                .cloned()
                .ok_or("free action is required")?,
        )
        .map_err(|e| e.to_string())?;
        let mut next = self.clone();
        next.game.apply_free(0, action)?;
        *self = next;
        Ok(())
    }

    /// A human approves food/workshop choices first, then sees the exact
    /// post-feeding state before choosing animal births. CPU uses its default.
    pub fn harvest(&mut self, input: &Value) -> Result<(), String> {
        let mut next = self.clone();
        let pending = next.pending.clone().ok_or("harvest is not pending")?;
        match pending {
            PendingHarvest::Feeding(action) => {
                let plan: FeedingPlan = serde_json::from_value(
                    input.get("feeding").cloned().ok_or("feeding is required")?,
                )
                .map_err(|e| e.to_string())?;
                plan.validate()?;
                let mut plans = vec![FeedingPlan::default(); next.game.config.players];
                plans[0] = plan;
                next.game.preview_harvest(&action, Some(&plans))?;
                next.pending = Some(PendingHarvest::Breeding(action, plans));
            }
            PendingHarvest::Breeding(action, feeding) => {
                let choice: BreedingPlan = serde_json::from_value(
                    input
                        .get("breeding")
                        .cloned()
                        .ok_or("breeding is required")?,
                )
                .map_err(|e| e.to_string())?;
                let preview = next.game.preview_harvest(&action, Some(&feeding))?;
                choice.apply(&mut preview.players[0].clone())?;
                let breeding = preview
                    .players
                    .iter()
                    .enumerate()
                    .map(|(i, player)| {
                        if i == 0 {
                            choice.clone()
                        } else {
                            BreedingPlan::simple(player)
                        }
                    })
                    .collect();
                let policy = ManualHarvest { feeding, breeding };
                next.game.apply_with_harvest_policy(action, &policy)?;
                next.pending = None;
                next.advance_cpu()?;
            }
        }
        *self = next;
        Ok(())
    }
}

/// Advance every CPU seat that has not yet committed, using only each seat's
/// own observation. Never reads another seat's private packet. The caller runs
/// this on a clone so a failure cannot partially mutate the session.
fn auto_commit_cpus(draft: &mut DraftWeb) -> Result<(), String> {
    let players = draft.state.player_count;
    let mode = draft.state.mode;
    for seat in 1..players {
        let pending = draft.state.commitments[seat].is_none() && !draft.state.is_complete();
        if !pending {
            continue;
        }
        let token = draft.state.current_token();
        let obs = draft
            .state
            .observe_with_catalog(seat, token, &draft.catalog)?;
        let ai = draft.draft_ai;
        let mut rng = decision_rng(draft.setup.seed, seat, token, ai);
        match mode {
            DraftMode::Choose10_7 => {
                let decision = ai.decide_choice(&obs, &mut rng)?;
                draft
                    .state
                    .commit_choice(seat, token, decision.occupations, decision.minors)?;
            }
            _ => {
                let decision = ai.decide_pick(&obs, &mut rng)?;
                draft
                    .state
                    .commit_pick(seat, token, decision.occupation, decision.minor)?;
            }
        }
    }
    Ok(())
}

/// Build the observable draft payload for the browser. It contains only the
/// human seat's own packets and history plus public counts; the seed, other
/// seats' packets, and the private discard pile are never serialized here.
fn draft_json(draft: &DraftWeb, viewer: usize) -> Result<Value, String> {
    let obs =
        draft
            .state
            .observe_with_catalog(viewer, draft.state.current_token(), &draft.catalog)?;
    let complete = draft.state.is_complete();
    // Only the viewer's own final hand may be revealed. Other seats' hands are
    // private even after the draft ends.
    let own_final_hands = if complete {
        draft
            .state
            .final_hands()
            .get(viewer)
            .map(|(occupations, minors)| json!({"occupations": occupations, "minors": minors}))
    } else {
        None
    };
    let ruleset = &draft.state.ruleset;
    let direction = |d: Direction| match d {
        Direction::Left => "left",
        Direction::Right => "right",
    };
    Ok(json!({
        "mode": obs.mode.id(),
        "ruleset_id": obs.ruleset_id,
        "pool_kind": &draft.pool_kind,
        "deck_version": &draft.setup.deck_version,
        // Public ruleset parameters, so a player can see which explicitly
        // chosen conventions the server is using. The direction choice is an
        // The clockwise passing rule is user-adopted, not a claim of official rules.
        "player_count": draft.setup.player_count,
        "start_player": obs.start_player,
        "direction_occupation": direction(ruleset.direction_occupation),
        "direction_minor": direction(ruleset.direction_minor),
        "occupation_phase_first": ruleset.occupation_phase_first,
        "ai": draft.draft_ai.id(),
        "phase": match obs.phase {
            DraftPhase::Drafting => "drafting",
            DraftPhase::Choosing => "choosing",
            DraftPhase::Complete => "complete",
        },
        "round": obs.round,
        "active_kind": obs.active_kind.map(CardKind::as_str),
        "hand_counts": obs.hand_counts,
        "own_picked": obs.own_picked,
        "offered_occupation": obs.offered_occupation,
        "offered_minor": obs.offered_minor,
        "remaining_to_pick": obs.remaining_to_pick,
        "request_token": obs.request_token,
        "history": obs.own_observation_history,
        "complete": complete,
        "game_started": draft.game_started,
        "own_final_hands": own_final_hands,
    }))
}

fn parse_direction(value: Option<&str>, field: &str) -> Result<Direction, String> {
    match value {
        None | Some("right") => Ok(Direction::Right),
        _ => Err(format!(
            "{field} must be right (clockwise) for this ruleset"
        )),
    }
}

/// Deterministic fixture pool. These IDs are NOT official cards; they exist so
/// the draft foundation can be exercised before the verified card pool (#115)
/// and the card subsystem (#139) are ready.
fn fixture_pool(mode: DraftMode, players: usize) -> DraftPool {
    let n = mode.dealt_per_kind() * players;
    DraftPool::new(
        (0..n).map(|i| format!("FIX-OCC-{i:02}")).collect(),
        (0..n).map(|i| format!("FIX-MIN-{i:02}")).collect(),
    )
}

/// Test-only card set for the fixture draft. These definitions let the
/// draft-to-game bridge (#104) run in the browser before the official pool
/// (#115/#139) is large enough. They are never returned by any `verified_*`
/// function and must not be treated as reviewed cards.
fn fixture_card_set(pool: &DraftPool) -> CardSet {
    let definition = |id: &String, kind: crate::cards::CardKind| CardDefinition {
        id: id.clone(),
        kind,
        cost: vec![],
        points: 0,
        effects: vec![],
    };
    let mut definitions = Vec::new();
    for id in &pool.occupations {
        definitions.push(definition(id, crate::cards::CardKind::Occupation));
    }
    for id in &pool.minors {
        definitions.push(definition(id, crate::cards::CardKind::MinorImprovement));
    }
    CardSet { definitions }
}

fn optional_string(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err("expected a string card id".into()),
    }
}

fn string_list(value: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    let array = value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field} must be an array of card ids"))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{field} must contain only strings"))
        })
        .collect()
}

fn local_request_allowed(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let local_host = host.is_some_and(|value| match value.rsplit_once(':') {
        Some((name, value_port)) => {
            (name == "127.0.0.1" || name == "localhost") && value_port.parse::<u16>() == Ok(port)
        }
        None => false,
    });
    let local_origin = origin.is_none_or(|value| {
        value == format!("http://127.0.0.1:{port}") || value == format!("http://localhost:{port}")
    });
    local_host && local_origin
}

fn json_content_type(value: Option<&str>) -> bool {
    value
        .and_then(|content_type| content_type.split(';').next())
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
}

fn response(stream: &mut TcpStream, status: &str, mime: &str, content: &str) -> Result<(), String> {
    let bytes = content.as_bytes();
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(bytes).map_err(|e| e.to_string())
}
fn handle(mut stream: TcpStream, session: &mut Session, port: u16) -> Result<(), String> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| e.to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let mut length = 0usize;
    let mut count = 0usize;
    let mut host: Option<String> = None;
    let mut origin: Option<String> = None;
    let mut content_type: Option<String> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if line.is_empty() || line == "\r\n" || line == "\n" {
            break;
        }
        count += 1;
        if count > 80 {
            return response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "application/json",
                "{\"error\":\"too many headers\"}",
            );
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value
                    .trim()
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            } else if name.eq_ignore_ascii_case("host") {
                host = Some(value.trim().to_ascii_lowercase());
            } else if name.eq_ignore_ascii_case("origin") {
                origin = Some(value.trim().to_ascii_lowercase());
            } else if name.eq_ignore_ascii_case("content-type") {
                content_type = Some(value.trim().to_owned());
            }
        }
    }
    if !local_request_allowed(host.as_deref(), origin.as_deref(), port) {
        return response(
            &mut stream,
            "403 Forbidden",
            "application/json",
            "{\"error\":\"non-local request rejected\"}",
        );
    }
    if method == "POST" && !json_content_type(content_type.as_deref()) {
        return response(
            &mut stream,
            "415 Unsupported Media Type",
            "application/json",
            "{\"error\":\"application/json required\"}",
        );
    }
    if length > request_body_limit(path) {
        return response(
            &mut stream,
            "413 Content Too Large",
            "application/json",
            "{\"error\":\"body too large for this endpoint\"}",
        );
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    if method == "GET" && path == "/" {
        return response(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX);
    }
    let result: Result<Value, String> = (|| match (method, path) {
        ("GET", "/api/state") => Ok(session.view()),
        ("GET", "/api/draft/state") => session.draft_view(),
        ("POST", "/api/draft/new")
        | ("POST", "/api/draft/pick")
        | ("POST", "/api/draft/choice")
        | ("POST", "/api/draft/start-game")
        | ("POST", "/api/draft/reset") => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            match path {
                "/api/draft/new" => session.start_draft(&value)?,
                "/api/draft/pick" => session.draft_pick(&value)?,
                "/api/draft/choice" => session.draft_choice(&value)?,
                "/api/draft/start-game" => session.start_draft_game(&value)?,
                _ => session.reset_draft(),
            }
            Ok(session.view())
        }
        ("POST", "/api/save") => {
            // The save is an explicit user action: it returns the full state,
            // including hidden information, so it is never served by GET.
            //
            // It is returned as *text*, not as a JSON object: the 64-bit seeds
            // exceed 2^53, so a JavaScript `JSON.parse`/`JSON.stringify` round
            // trip would round them and the file would no longer load.
            let save = session.export_save()?;
            let text = serde_json::to_string_pretty(&save).map_err(|e| e.to_string())?;
            Ok(json!({
                "save_text": text,
                "download_name": session.save_name(),
            }))
        }
        ("POST", "/api/load") => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            session.import_save(&value)?;
            Ok(session.view())
        }
        ("POST", "/api/new")
        | ("POST", "/api/play")
        | ("POST", "/api/free")
        | ("POST", "/api/harvest") => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if path == "/api/new" {
                session.reset(&value)?;
            } else if path == "/api/play" {
                session.play(&value)?;
            } else if path == "/api/free" {
                session.free(&value)?;
            } else {
                session.harvest(&value)?;
            }
            Ok(session.view())
        }
        _ => Err("endpoint not found".into()),
    })();
    match result {
        Ok(value) => response(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            &value.to_string(),
        ),
        Err(error) => response(
            &mut stream,
            "400 Bad Request",
            "application/json; charset=utf-8",
            &json!({"error": error}).to_string(),
        ),
    }
}
/// Start the local UI on the port from `AGRICOLA_WEB_PORT`, or 8787.
pub fn serve(config: Config) -> Result<(), String> {
    let port = std::env::var("AGRICOLA_WEB_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8787);
    serve_on(config, port)
}

/// Bind the loopback-only UI on `port` and serve until the process stops.
///
/// Port `0` asks the OS for a free port and prints the bound URL. Only
/// loopback `Host`/`Origin` values for the bound port are accepted, so the
/// boundary stays single-user and local regardless of the chosen port.
pub fn serve_on(config: Config, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("failed to bind 127.0.0.1:{port}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let mut session = Session::new(config)?;
    println!("Agricola local UI: http://127.0.0.1:{bound}/");
    for accepted in listener.incoming() {
        match accepted {
            Ok(stream) => {
                if let Err(error) = handle(stream, &mut session, bound) {
                    eprintln!("UI request error: {error}");
                }
            }
            Err(error) => eprintln!("UI connection error: {error}"),
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::EvaluationPolicy;

    #[test]
    fn local_api_rejects_cross_site_and_simple_form_posts() {
        assert!(local_request_allowed(Some("127.0.0.1:8787"), None, 8787));
        assert!(local_request_allowed(
            Some("localhost:8787"),
            Some("http://localhost:8787"),
            8787
        ));
        assert!(!local_request_allowed(
            Some("evil.example:8787"),
            Some("http://evil.example:8787"),
            8787
        ));
        assert!(!local_request_allowed(
            Some("127.0.0.1:8787"),
            Some("https://evil.example"),
            8787
        ));
        assert!(!local_request_allowed(None, None, 8787));
        assert!(!local_request_allowed(Some("127.0.0.1:9999"), None, 8787));
        assert!(json_content_type(Some("application/json; charset=utf-8")));
        assert!(!json_content_type(Some("text/plain")));
        assert!(!json_content_type(None));
    }

    #[test]
    fn rejects_invalid_configuration_and_move() {
        let mut session = Session::new(Config::default()).unwrap();
        assert!(session.reset(&json!({"players":5})).is_err());
        assert!(session.play(&json!({"space":"not_a_space"})).is_err());
        session
            .reset(&json!({"players":4,"seed":100,"extra":true}))
            .unwrap();
        assert_eq!(session.game.public.players.len(), 4);
        session.game.validate().unwrap();
    }
    fn play_simple(session: &mut Session, space: Space) {
        let detail = SimpleDetails
            .produce(&session.game.observation(0).unwrap(), space)
            .expect("test policy detail")
            .detail;
        session
            .play(&json!({"space": space, "detail": detail}))
            .unwrap();
        while let Some(pending) = session.pending.clone() {
            match pending {
                PendingHarvest::Feeding(_) => session
                    .harvest(&json!({"feeding": FeedingPlan::default()}))
                    .unwrap(),
                PendingHarvest::Breeding(action, feeding) => {
                    let preview = session
                        .game
                        .preview_harvest(&action, Some(&feeding))
                        .unwrap();
                    let choice = BreedingPlan::simple(&preview.players[0]);
                    session.harvest(&json!({"breeding": choice})).unwrap();
                }
            }
        }
    }

    #[test]
    fn manual_details_required_and_preview_does_not_commit() {
        let mut session = Session::new(Config::default()).unwrap();
        if session.game.public.active_player != 0 {
            session.advance_cpu().unwrap();
        }
        let legal = session.game.legal_spaces();
        let first = legal[0];
        let hash = session.game.state_hash();
        assert!(session.play(&json!({"space": first})).is_err());
        assert_eq!(session.game.state_hash(), hash);
    }

    #[test]
    fn human_path_accepts_a_legal_non_default_detail_and_rejects_illegal() {
        let seed = (0..2_000u64)
            .find(|&seed| {
                let probe = Game::new(Config {
                    players: 2,
                    seed,
                    additional_action_space_enabled: false,
                });
                probe.public.start_player == 0
                    && probe.public.revealed.first() == Some(&crate::model::RoundCard::Fencing)
            })
            .expect("seed with human start and Fencing revealed");
        let mut session = Session::new(Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        })
        .unwrap();
        assert_eq!(session.game.public.active_player, 0);
        session.game.public.players[0].supply.wood = 10;

        let hash = session.game.state_hash();
        // An illegal detail is rejected by the shared validator without
        // changing state; the web endpoint no longer filters by candidate set.
        assert!(session
            .play(&json!({"space":"fencing","detail":{"Fence":{"pastures":[[0,1]]}}}))
            .is_err());
        assert_eq!(session.game.state_hash(), hash);
        session
            .play(&json!({"space":"fencing","detail":{"Fence":{"pastures":[[1,2]]}}}))
            .expect("legal non-default detail accepted through the human endpoint");
        assert!(session.game.public.players[0]
            .pastures
            .iter()
            .any(|pasture| pasture.cells == vec![1, 2]));
    }

    #[test]
    fn manual_free_action_is_logged_and_replays() {
        let mut session = Session::new(Config {
            seed: 34,
            ..Config::default()
        })
        .unwrap();
        let before = session.game.public.players[0].supply.food;
        session
            .free(&json!({"action": {"DiscardGood": {"good": "food", "quantity": 1}}}))
            .unwrap();
        assert_eq!(session.game.public.players[0].supply.food, before - 1);
        let replay = Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }

    #[test]
    fn harvest_decisions_require_human_and_replay() {
        let mut session = Session::new(Config {
            seed: 123,
            ..Config::default()
        })
        .unwrap();
        for _ in 0..80 {
            if let Some(PendingHarvest::Feeding(action)) = session.pending.clone() {
                let before = session.game.state_hash();
                let preview = session.game.preview_harvest(&action, None).unwrap();
                assert_eq!(preview.round, 4);
                let mut plan = FeedingPlan::default();
                plan.priorities.reverse();
                session.harvest(&json!({"feeding": plan})).unwrap();
                assert_eq!(session.game.state_hash(), before);
                let (action, plans) = match session.pending.clone().unwrap() {
                    PendingHarvest::Breeding(action, plans) => (action, plans),
                    _ => panic!("breeding choice must follow feeding choice"),
                };
                let after_feeding = session.game.preview_harvest(&action, Some(&plans)).unwrap();
                let choice = BreedingPlan::simple(&after_feeding.players[0]);
                session.harvest(&json!({"breeding": choice})).unwrap();
                let replay =
                    Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
                assert_eq!(replay.state_hash(), session.game.state_hash());
                return;
            }
            let space = session.game.legal_spaces()[0];
            let detail = SimpleDetails
                .produce(&session.game.observation(0).unwrap(), space)
                .unwrap()
                .detail;
            session
                .play(&json!({"space": space, "detail": detail}))
                .unwrap();
        }
        panic!("harvest was never reached");
    }

    #[test]
    fn human_and_cpu_can_take_turns() {
        let mut session = Session::new(Config {
            seed: 5,
            ..Config::default()
        })
        .unwrap();
        for _ in 0..3 {
            if session.game.public.finished {
                break;
            }
            assert_eq!(session.game.public.active_player, 0);
            let space = session.game.legal_spaces()[0];
            play_simple(&mut session, space);
            session.game.validate().unwrap();
        }
    }
    #[test]
    fn complete_four_player_human_vs_cpu_match() {
        let mut session = Session::new(Config {
            seed: 103,
            players: 4,
            additional_action_space_enabled: true,
        })
        .unwrap();
        for turn in 0..120 {
            if session.game.public.finished {
                break;
            }
            assert_eq!(session.game.public.active_player, 0, "turn {turn}");
            let obs = session.game.observation(0).unwrap();
            let legal = session.game.legal_spaces();
            let mut rng = ChaCha8Rng::seed_from_u64(turn);
            let space = EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap();
            play_simple(&mut session, space);
        }
        assert!(session.game.public.finished);
        let replay = Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }

    #[test]
    fn complete_two_player_verified_cards_through_manual_requests() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .reset(&json!({"players": 2, "seed": 22, "preset": "verified"}))
            .unwrap();
        assert_eq!(session.view()["card_enabled"], true);
        for turn in 0..80 {
            if session.game.public.finished {
                break;
            }
            assert_eq!(session.game.public.active_player, 0, "turn {turn}");
            let obs = session.game.observation(0).unwrap();
            let legal = session.game.legal_spaces();
            let mut rng = ChaCha8Rng::seed_from_u64(turn);
            let space = if legal.contains(&Space::Lessons) {
                Space::Lessons
            } else {
                EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap()
            };
            play_simple(&mut session, space);
            session.game.validate().unwrap();
        }
        assert!(session.game.public.finished, "manual card game must finish");
        assert_eq!(session.game.public.round, 14);
        assert!(
            session
                .game
                .public
                .players
                .iter()
                .any(|player| !player.played_cards.is_empty()),
            "the card-enabled UI path must actually play a card"
        );
        let replay = Game::replay_with_cards(
            session.game.config.clone(),
            (*session.game.card_set).clone(),
            &session.game.log.events,
        )
        .unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }

    #[test]
    fn draft_endpoints_fix_clockwise_direction_and_require_explicit_phase_order() {
        let mut session = Session::new(Config::default()).unwrap();
        assert!(
            session
                .start_draft(&json!({
                    "mode":"draft_7_7","players":2,
                    "direction_occupation":"right","direction_minor":"right"
                }))
                .is_err(),
            "7-7 still needs an explicit phase order"
        );
        assert!(
            session
                .start_draft(&json!({
                    "mode":"draft_7_7","players":2,
                    "direction_occupation":"left","direction_minor":"right",
                    "occupation_phase_first":true
                }))
                .is_err(),
            "old reverse direction must not silently pass"
        );
        // Omitted direction now deliberately adopts the user-selected
        // clockwise rule instead of an unconfirmed tournament convention.
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":3,
                "occupation_phase_first":true
            }))
            .unwrap();
        let view = session.draft_view().unwrap();
        assert_eq!(view["direction_occupation"], "right");
        assert_eq!(view["direction_minor"], "right");
        let first = view["start_player"].as_u64().unwrap() as usize;
        assert!(first < 2);
    }

    #[test]
    fn draft_web_plays_full_simultaneous_and_redacts_private_data() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_10_7_simultaneous","players":2,"seed":9,
                "direction_occupation":"right","direction_minor":"right"
            }))
            .unwrap();
        for _ in 0..40 {
            let view = session.draft_view().unwrap();
            if view["complete"].as_bool().unwrap() {
                break;
            }
            let token = view["request_token"].as_u64().unwrap();
            let occupation = view["offered_occupation"][0].as_str().unwrap().to_owned();
            let minor = view["offered_minor"][0].as_str().unwrap().to_owned();
            session
                .draft_pick(&json!({"token":token,"occupation":occupation,"minor":minor}))
                .unwrap();
        }
        let view = session.draft_view().unwrap();
        assert!(view["complete"].as_bool().unwrap());
        assert_eq!(view["phase"], "complete");
        assert_eq!(
            view["own_final_hands"]["occupations"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        // The browser view never carries the private seed or a deck field.
        assert!(view.get("seed").is_none());
        assert!(view.get("packets").is_none());
        // Other seats' final hands are not exposed in the viewer payload.
        assert!(view.get("final_hands").is_none());
        session
            .draft()
            .unwrap()
            .state
            .validate_invariants()
            .unwrap();
    }

    #[test]
    fn draft_web_rejected_pick_does_not_advance_cpu_or_state() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":3,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        let before = session.draft().unwrap().state.state_hash();
        let token = session.draft().unwrap().state.current_token();
        // Off-packet card: the human commit and the CPU follow-ups must not
        // happen at all.
        assert!(session
            .draft_pick(&json!({"token":token,"occupation":"NOT-OFFERED"}))
            .is_err());
        assert_eq!(session.draft().unwrap().state.state_hash(), before);
        assert!(session
            .draft()
            .unwrap()
            .state
            .commitments
            .iter()
            .all(Option::is_none));
        // A stale token is likewise atomic.
        assert!(session
            .draft_pick(&json!({"token":token + 5,"occupation":"NOT-OFFERED"}))
            .is_err());
        assert_eq!(session.draft().unwrap().state.state_hash(), before);
    }

    #[test]
    fn draft_web_auto_plays_cpu_choice_mode() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"choose_10_7","players":3,"seed":17,
                "direction_occupation":"right","direction_minor":"right"
            }))
            .unwrap();
        let view = session.draft_view().unwrap();
        let token = view["request_token"].as_u64().unwrap();
        let occ = view["offered_occupation"]
            .as_array()
            .unwrap()
            .iter()
            .take(7)
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let minor = view["offered_minor"]
            .as_array()
            .unwrap()
            .iter()
            .take(7)
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        session
            .draft_choice(&json!({"token":token,"occupations":occ,"minors":minor}))
            .unwrap();
        assert!(session.draft_view().unwrap()["complete"].as_bool().unwrap());
    }

    fn drive_draft_to_completion(session: &mut Session) {
        for _ in 0..40 {
            let view = session.draft_view().unwrap();
            if view["complete"].as_bool().unwrap() {
                return;
            }
            let token = view["request_token"].as_u64().unwrap();
            if view["phase"] == "choosing" {
                let take7 = |key: &str| {
                    view[key]
                        .as_array()
                        .unwrap()
                        .iter()
                        .take(7)
                        .map(|value| value.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>()
                };
                session
                    .draft_choice(&json!({
                        "token": token,
                        "occupations": take7("offered_occupation"),
                        "minors": take7("offered_minor"),
                    }))
                    .unwrap();
                continue;
            }
            let active = view["active_kind"].as_str();
            let occupation = (active != Some("minor_improvement")
                && !view["offered_occupation"].as_array().unwrap().is_empty())
            .then(|| view["offered_occupation"][0].as_str().unwrap().to_owned());
            let minor = (active != Some("occupation")
                && !view["offered_minor"].as_array().unwrap().is_empty())
            .then(|| view["offered_minor"][0].as_str().unwrap().to_owned());
            session
                .draft_pick(&json!({"token":token,"occupation":occupation,"minor":minor}))
                .unwrap();
        }
        panic!("draft did not complete");
    }

    #[test]
    fn draft_web_starts_a_game_from_the_drafted_hand_and_keeps_history() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":606,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        drive_draft_to_completion(&mut session);
        let view = session.draft_view().unwrap();
        assert_eq!(view["complete"], true);
        let expected = view["own_final_hands"]["occupations"]
            .as_array()
            .unwrap()
            .iter()
            .chain(view["own_final_hands"]["minors"].as_array().unwrap().iter())
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let history_len = view["history"].as_array().unwrap().len();
        assert!(history_len > 0);

        session.start_draft_game(&json!({})).unwrap();
        let started = session.draft_view().unwrap();
        assert_eq!(started["game_started"], true);
        // The observation history survives into the playing phase.
        assert_eq!(started["history"].as_array().unwrap().len(), history_len);
        assert_eq!(session.view()["draft"]["game_started"], true);

        let mut hand = session.game.cards.hands[0].clone();
        let mut expected = expected;
        hand.sort();
        expected.sort();
        assert_eq!(
            hand, expected,
            "the in-game hand is exactly the drafted 7 + 7"
        );
        assert_eq!(session.view()["hand"].as_array().unwrap().len(), 14);
        // A second start request is rejected.
        assert!(session.start_draft_game(&json!({})).is_err());
    }

    #[test]
    fn draft_web_rejects_incomplete_start_without_touching_the_game() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":1,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        let hash = session.game.state_hash();
        assert!(session.start_draft_game(&json!({})).is_err());
        assert_eq!(session.game.state_hash(), hash);
        assert_eq!(session.draft_view().unwrap()["game_started"], false);
    }

    #[test]
    fn draft_web_separates_fixture_and_verified_pools() {
        let mut session = Session::new(Config::default()).unwrap();
        // 10-7 is blocked for the implemented pool instead of being padded.
        assert!(session
            .start_draft(&json!({
                "mode":"draft_10_7_simultaneous","players":2,"seed":1,
                "direction_occupation":"right","direction_minor":"right",
                "pool":"verified_basic"
            }))
            .is_err());
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":1,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true,"pool":"verified_basic"
            }))
            .unwrap();
        assert_eq!(session.draft_view().unwrap()["pool_kind"], "verified_basic");
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":1,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        assert_eq!(session.draft_view().unwrap()["pool_kind"], "test_fixture");
    }

    #[test]
    fn draft_web_exposes_selectable_baselines_and_rejects_unknown_ai() {
        let mut session = Session::new(Config::default()).unwrap();
        assert!(session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":1,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true,"draft_ai":"not_a_baseline"
            }))
            .is_err());
        for ai in ["random", "card_value", "synergy"] {
            session
                .start_draft(&json!({
                    "mode":"draft_7_7","players":2,"seed":11,
                    "direction_occupation":"right","direction_minor":"right",
                    "occupation_phase_first":true,"draft_ai":ai
                }))
                .unwrap();
            assert_eq!(session.draft_view().unwrap()["ai"], ai);
            drive_draft_to_completion(&mut session);
            assert!(session.draft_view().unwrap()["complete"].as_bool().unwrap());
        }
    }

    #[test]
    fn draft_web_verified_pool_seeds_a_playable_card_game() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":707,
                "direction_occupation":"right","direction_minor":"right",
                "occupation_phase_first":true,"pool":"verified_basic"
            }))
            .unwrap();
        drive_draft_to_completion(&mut session);
        session.start_draft_game(&json!({"seed":808})).unwrap();
        assert_eq!(session.view()["card_enabled"], true);
        assert_eq!(
            session.view()["public"]["players"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(session.view()["hand"].as_array().unwrap().len(), 14);
        // Opponent hands stay hidden in the serialized session view.
        assert!(session.view().get("hidden_round_deck").is_none());
    }

    // ---- M9 save / restore (Issue #92) ------------------------------------

    /// Play a few human turns through the same endpoint the UI uses, so the
    /// save is taken from a realistic mid-game state.
    fn play_turns(session: &mut Session, turns: usize) {
        for _ in 0..turns {
            if session.game.public.finished {
                return;
            }
            if session.pending.is_some() {
                let pending = session.pending.clone().expect("pending");
                match pending {
                    PendingHarvest::Feeding(_) => session
                        .harvest(&json!({"feeding": FeedingPlan::default()}))
                        .unwrap(),
                    PendingHarvest::Breeding(action, feeding) => {
                        let preview = session
                            .game
                            .preview_harvest(&action, Some(&feeding))
                            .unwrap();
                        let choice = BreedingPlan::simple(&preview.players[0]);
                        session.harvest(&json!({"breeding": choice})).unwrap();
                    }
                }
                continue;
            }
            if session.game.public.active_player != 0 {
                session.advance_cpu().unwrap();
                continue;
            }
            let space = session.game.legal_spaces()[0];
            play_simple(session, space);
        }
    }

    fn seeded_session(seed: u64) -> Session {
        Session::new(Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        })
        .unwrap()
    }

    #[test]
    fn a_saved_match_restores_into_a_fresh_session() {
        let mut session = seeded_session(42);
        play_turns(&mut session, 12);
        assert!(!session.game.public.finished, "the test saves mid-game");
        let save = session.export_save().unwrap();
        let hash = session.game.state_hash();
        let view = session.view();

        // A new process starts from `Session::new`; nothing of the old session
        // is reused, so the restore must carry every part of the state.
        let mut restarted = seeded_session(7);
        restarted.import_save(&json!({ "save": save })).unwrap();
        assert_eq!(restarted.game.state_hash(), hash);
        assert_eq!(restarted.view(), view);
        assert_eq!(restarted.game.public.round, session.game.public.round);
        assert_eq!(restarted.game.cards.hands[0], session.game.cards.hands[0]);
        assert_eq!(
            restarted.game.public.players[0].supply,
            session.game.public.players[0].supply
        );
        restarted.game.validate().unwrap();
    }

    #[test]
    fn a_save_posted_verbatim_keeps_the_64_bit_seeds() {
        // The browser posts the file text unchanged. Parsing it into JavaScript
        // numbers first would round `round_seed`/`action_seed` (they exceed
        // 2^53), and the engine would then reject the file as an initial-state
        // mismatch. This test pins the server-side contract.
        let mut session = seeded_session(42);
        play_turns(&mut session, 6);
        let save = session.export_save().unwrap();
        let text = serde_json::to_string(&save).unwrap();
        let round_seed = save["game"]["log"]["round_seed"].as_u64().unwrap();
        assert!(
            round_seed > (1u64 << 53),
            "the seed must exceed 2^53: {round_seed}"
        );

        let posted: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            posted["game"]["log"]["round_seed"].as_u64(),
            Some(round_seed)
        );

        let hash = session.game.state_hash();
        let mut restarted = seeded_session(3);
        // Top-level save object, not `{"save": ...}`.
        restarted.import_save(&posted).unwrap();
        assert_eq!(restarted.game.state_hash(), hash);
        assert_eq!(restarted.game.log.round_seed, session.game.log.round_seed);
        assert_eq!(restarted.game.log.action_seed, session.game.log.action_seed);
    }

    #[test]
    fn a_restored_match_finishes_and_replays() {
        let mut session = seeded_session(43);
        play_turns(&mut session, 20);
        let save = session.export_save().unwrap();
        let mut restarted = seeded_session(1);
        restarted.import_save(&json!({ "save": save })).unwrap();
        for _ in 0..400 {
            if restarted.game.public.finished {
                break;
            }
            play_turns(&mut restarted, 1);
        }
        assert!(
            restarted.game.public.finished,
            "the restored match must finish"
        );
        let replay = Game::replay_with_cards(
            restarted.game.config.clone(),
            (*restarted.game.card_set).clone(),
            &restarted.game.log.events,
        )
        .unwrap();
        assert_eq!(replay.state_hash(), restarted.game.state_hash());
    }

    /// Play one decision without resolving a queued harvest, so a test can
    /// observe the pending state itself.
    fn play_one_step(session: &mut Session) -> bool {
        if session.pending.is_some() {
            return true;
        }
        if session.game.public.finished {
            return false;
        }
        if session.game.public.active_player != 0 {
            session.advance_cpu().unwrap();
            return session.pending.is_some();
        }
        let space = session.game.legal_spaces()[0];
        let detail = SimpleDetails
            .produce(&session.game.observation(0).unwrap(), space)
            .expect("test policy detail")
            .detail;
        session
            .play(&json!({"space": space, "detail": detail}))
            .unwrap();
        session.pending.is_some()
    }

    #[test]
    fn a_save_is_refused_while_a_harvest_decision_is_pending() {
        let mut session = seeded_session(44);
        for _ in 0..400 {
            if play_one_step(&mut session) {
                break;
            }
        }
        assert!(session.pending.is_some(), "a harvest must be pending");
        let error = session.export_save().unwrap_err();
        assert!(error.contains("収穫"), "unexpected error: {error}");
    }

    #[test]
    fn a_save_is_refused_while_a_draft_is_in_progress() {
        let mut session = seeded_session(45);
        session
            .start_draft(&json!({
                "mode": "draft_7_7",
                "players": 2,
                "seed": 5,
                "direction_occupation": "right",
                "direction_minor": "right",
                "occupation_phase_first": true,
                "pool": "test_fixture",
                "draft_ai": "random"
            }))
            .unwrap();
        let error = session.export_save().unwrap_err();
        assert!(error.contains("ドラフト"), "unexpected error: {error}");
    }

    #[test]
    fn a_completed_draft_game_must_not_save_without_its_observation_history() {
        let mut session = seeded_session(55);
        session
            .start_draft(&json!({
                "mode":"draft_7_7", "players":2, "seed":606,
                "direction_occupation":"right", "direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        drive_draft_to_completion(&mut session);
        session.start_draft_game(&json!({})).unwrap();
        let before = session.draft_view().unwrap();
        assert!(before["history"]
            .as_array()
            .is_some_and(|history| !history.is_empty()));
        let hash = session.game.state_hash();
        let error = session.export_save().unwrap_err();
        assert!(error.contains("ドラフト"), "unexpected error: {error}");
        assert_eq!(session.game.state_hash(), hash);
        assert_eq!(session.draft_view().unwrap(), before);
    }

    #[test]
    fn save_upload_limit_is_specific_to_the_load_endpoint() {
        assert_eq!(request_body_limit("/api/load"), MAX_LOAD_BODY);
        assert_eq!(request_body_limit("/api/save"), MAX_BODY);
        assert_eq!(request_body_limit("/api/play"), MAX_BODY);
    }

    #[test]
    fn four_player_endgame_save_fits_the_bounded_load_limit() {
        let mut session = Session::new(Config {
            players: 4,
            seed: 66,
            additional_action_space_enabled: false,
        })
        .unwrap();
        for _ in 0..1500 {
            if session.game.public.finished {
                break;
            }
            play_turns(&mut session, 1);
        }
        assert!(
            session.game.public.finished,
            "four player match did not finish"
        );
        let save = session.export_save().unwrap();
        let text = serde_json::to_string_pretty(&save).unwrap();
        assert!(
            text.len() <= request_body_limit("/api/load"),
            "four-player endgame save is {} bytes",
            text.len()
        );
        let mut reloaded = seeded_session(9);
        reloaded.import_save(&save).unwrap();
        assert_eq!(reloaded.game.state_hash(), session.game.state_hash());
    }

    #[test]
    fn a_corrupted_save_is_rejected_without_touching_the_session() {
        let mut session = seeded_session(46);
        play_turns(&mut session, 8);
        let hash = session.game.state_hash();
        for broken in [
            json!({"save": "not an object"}),
            json!({"save": {}}),
            json!({"save": {"format": "other", "schema_version": 1}}),
            json!({"save": {"format": "agricola-web-save", "schema_version": 99}}),
            json!({"save": {"format": "agricola-web-save", "schema_version": 1}}),
            json!({}),
        ] {
            assert!(session.import_save(&broken).is_err(), "{broken}");
            assert_eq!(session.game.state_hash(), hash, "{broken}");
        }
    }

    #[test]
    fn a_tampered_save_is_rejected_by_the_engine_validation() {
        let mut session = seeded_session(47);
        play_turns(&mut session, 10);
        let save = session.export_save().unwrap();
        let mut tampered = save.clone();
        // Give the human free food without any logged action: the event chain
        // no longer reproduces the stored state.
        let food = tampered["game"]["public"]["players"][0]["supply"]["food"]
            .as_i64()
            .unwrap_or(0);
        tampered["game"]["public"]["players"][0]["supply"]["food"] = json!(food + 5);
        let hash = session.game.state_hash();
        let error = session.import_save(&json!({"save": tampered})).unwrap_err();
        assert!(
            error.contains("保存データが不正") || error.contains("replay"),
            "unexpected error: {error}"
        );
        assert_eq!(
            session.game.state_hash(),
            hash,
            "a rejected save changes nothing"
        );
    }

    #[test]
    fn a_save_name_is_a_safe_file_name() {
        let session = seeded_session(48);
        let name = session.save_name();
        assert!(name.starts_with("agricola-2p-seed48-round1"), "{name}");
        assert!(!name.contains('/') && !name.contains('\\'), "{name}");
    }
}

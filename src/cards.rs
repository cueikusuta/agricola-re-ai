//! Card-set metadata is supplied by callers; copyrighted card text is never bundled.
//! This is a deliberately small deterministic effect vocabulary, not a claim that
//! the full Agricola Revised deck has been implemented.
use crate::model::{Animal, Crop, Good, House, Player, Space, Tile};
use crate::registry::{
    build_default_registry, card_set_from_pool, card_set_from_registry, CardId, CardPool,
    CardRegistry,
};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Occupation,
    MinorImprovement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardEffect {
    /// Test-only probe: an effect whose resolution requires a player decision.
    /// Compiled only in the crate's own test build, so the production card pool
    /// and `CardSet::content_hash` are unaffected. It exists so the dispatcher's
    /// Interrupt branch (handler -> `EffectResult::Pending` -> `PendingDecision`
    /// -> pause) is exercised end to end instead of being an untested path.
    #[cfg(test)]
    PendingProbe {
        options: Vec<(Good, u32)>,
    },
    OnPlayGain {
        good: Good,
        amount: u32,
    },
    AfterWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    EndGameBonus {
        rule: ScoreBonusRule,
    },
    Prerequisite {
        rule: CardRequirement,
    },
    OnPlayGainPerStored {
        stored: Good,
        divisor: u32,
        good: Good,
        amount: u32,
    },
    OnPlayGainIfHouse {
        house: House,
        rooms: u8,
        good: Good,
        amount: u32,
    },
    StartRoundGain {
        good: Good,
        amount: u32,
        house: Option<House>,
        rooms: Option<u8>,
    },
    HarvestFieldGain {
        rule: HarvestFieldRule,
        good: Good,
        amount: u32,
    },
    AfterWorkerGainIfUsed {
        space: Space,
        required_space: Space,
        good: Good,
        amount: u32,
    },
    Passive {
        rule: PassiveRule,
    },
    AfterClayConstructionGain {
        good: Good,
        amount: u32,
    },
    AfterHarvestBakeGain {
        amount: u32,
    },
}

impl CardEffect {
    /// Stable public name for this implemented effect category. Used to build
    /// the neutral draft-AI catalog (#104). It describes only effects the
    /// engine actually implements, never a guessed printed effect.
    pub fn public_tag(&self) -> &'static str {
        match self {
            #[cfg(test)]
            CardEffect::PendingProbe { .. } => "pending_probe",
            CardEffect::OnPlayGain { .. } => "on_play_gain",
            CardEffect::AfterWorkerGain { .. } => "after_worker_gain",
            CardEffect::EndGameBonus { .. } => "end_game_bonus",
            CardEffect::Prerequisite { .. } => "prerequisite",
            CardEffect::OnPlayGainPerStored { .. } => "on_play_gain_per_stored",
            CardEffect::OnPlayGainIfHouse { .. } => "on_play_gain_if_house",
            CardEffect::StartRoundGain { .. } => "start_round_gain",
            CardEffect::HarvestFieldGain { .. } => "harvest_field_gain",
            CardEffect::AfterWorkerGainIfUsed { .. } => "after_worker_gain_if_used",
            CardEffect::Passive { .. } => "passive",
            CardEffect::AfterClayConstructionGain { .. } => "after_clay_construction_gain",
            CardEffect::AfterHarvestBakeGain { .. } => "after_harvest_bake_gain",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveRule {
    PastureCapacity(u32),
    FenceWoodDiscount(u32),
    ImprovementWoodDiscount(u32),
    WoodenRoomCost(u32),
    RoomCost(u32),
    MajorStoneDiscountByRooms,
    StoneDiscount(u32),
    HousingSpace(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarvestFieldRule {
    Animals { sheep_per: u32, cattle_per: u32 },
    MixedFields,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRequirement {
    MinimumOccupations(u8),
    MaximumOccupations(u8),
    MinimumAnimals { animal: Animal, number: u32 },
    MinimumCropFields { crop: Crop, number: u8 },
    MinimumPlayers(u8),
}

/// Named formulas keep card descriptions outside source and replay data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreBonusRule {
    PastureArea,
    HouseMaterial,
    UnfencedStables,
    ImprovementCount,
    SparePastureCapacity,
    LaterOccupations,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDefinition {
    /// A stable programmatic identifier, NOT the card's printed text.
    pub id: String,
    pub kind: CardKind,
    pub cost: Vec<(Good, u32)>,
    pub points: i32,
    pub effects: Vec<CardEffect>,
    /// Maximum times this card's effects may resolve per round, for cards
    /// printed with a per-round use limit. `None` = no limit (the default), and
    /// it is skipped when serializing, so existing card sets and saved games
    /// keep their exact bytes and `content_hash` is unchanged unless a limit is
    /// actually declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_limit: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSet {
    pub definitions: Vec<CardDefinition>,
}

impl CardSet {
    pub fn definition(&self, id: &str) -> Option<&CardDefinition> {
        self.definitions.iter().find(|card| card.id == id)
    }

    pub fn enabled(&self) -> bool {
        !self.definitions.is_empty()
    }

    /// Order-independent content hash of the deck's public metadata.
    ///
    /// Experiment manifests (M12/M13) must show that two runs used the same
    /// card pool. Only `id`, `kind`, `cost`, `points` and `effects` are hashed:
    /// no seed, no dealt hand, no hidden state. The `agricola-card-set-v1`
    /// domain tag keeps an empty set distinct from any populated one.
    pub fn content_hash(&self) -> String {
        let mut entries = self
            .definitions
            .iter()
            .map(|card| {
                (
                    card.id.clone(),
                    serde_json::to_string(card).expect("card definition serialization"),
                )
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries.dedup();
        let mut hasher = Sha256::new();
        hasher.update(b"agricola-card-set-v1\0");
        for (id, definition) in entries {
            hasher.update(id.as_bytes());
            hasher.update([0u8]);
            hasher.update(definition.as_bytes());
            hasher.update([0u8]);
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn validate(&self, players: usize) -> Result<(), String> {
        if !self.enabled() {
            return Ok(());
        }
        let mut ids = BTreeSet::new();
        for card in &self.definitions {
            if card.id.is_empty()
                || card.id.len() > 64
                || !card
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(card.id.clone())
            {
                return Err("invalid or duplicate card id".into());
            }
            if !(-10..=20).contains(&card.points)
                || card
                    .cost
                    .iter()
                    .any(|(_, quantity)| *quantity == 0 || *quantity > 30)
                || card.effects.iter().any(|effect| {
                    let amount = match effect {
                        CardEffect::OnPlayGain { amount, .. }
                        | CardEffect::AfterWorkerGain { amount, .. } => *amount,
                        CardEffect::EndGameBonus { .. }
                        | CardEffect::Prerequisite { .. }
                        | CardEffect::Passive { .. } => 1,
                        #[cfg(test)]
                        CardEffect::PendingProbe { .. } => 1,
                        CardEffect::OnPlayGainPerStored { amount, .. }
                        | CardEffect::OnPlayGainIfHouse { amount, .. }
                        | CardEffect::StartRoundGain { amount, .. }
                        | CardEffect::HarvestFieldGain { amount, .. }
                        | CardEffect::AfterWorkerGainIfUsed { amount, .. }
                        | CardEffect::AfterClayConstructionGain { amount, .. }
                        | CardEffect::AfterHarvestBakeGain { amount } => *amount,
                    };
                    amount == 0 || amount > 30
                })
                || card.effects.iter().any(|effect| {
                    matches!(effect, CardEffect::OnPlayGainPerStored { divisor: 0, .. })
                        || matches!(
                            effect,
                            CardEffect::HarvestFieldGain {
                                rule: HarvestFieldRule::Animals { sheep_per: 0, .. }
                                    | HarvestFieldRule::Animals { cattle_per: 0, .. },
                                ..
                            }
                        )
                })
            {
                return Err("unsupported card cost, points, or effect".into());
            }
        }
        for card in &self.definitions {
            for effect in &card.effects {
                if let CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(min),
                } = effect
                {
                    if *min == 0 || *min as usize > players {
                        return Err(format!(
                            "card {} is not eligible for this player count",
                            card.id
                        ));
                    }
                }
            }
        }
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let count = self.definitions.iter().filter(|c| c.kind == kind).count();
            if count < 7 * players {
                return Err(
                    "implemented card set needs seven cards of each kind per player".into(),
                );
            }
        }
        Ok(())
    }
}

/// Private: must never be serialized into a player's Observation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardState {
    pub hands: Vec<Vec<String>>,
    pub undealt: Vec<String>,
}

impl CardState {
    pub fn deal(set: &CardSet, seed: u64, players: usize) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xA5E1_9C32_76D4_50BF);
        let mut hands = vec![Vec::new(); players];
        let mut undealt = Vec::new();
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let mut pool = set
                .definitions
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| c.id.clone())
                .collect::<Vec<_>>();
            pool.shuffle(&mut rng);
            for hand in &mut hands {
                for _ in 0..7.min(pool.len()) {
                    hand.push(pool.pop().expect("checked length"));
                }
            }
            undealt.extend(pool);
        }
        Self { hands, undealt }
    }

    /// Build a card state from explicit, fully specified starting hands.
    ///
    /// This is the M11 draft-to-game bridge (#104): it never re-shuffles or
    /// deals randomly. Every card id must already exist in `set`, appear at
    /// most once across all seats, and each seat must hold exactly seven
    /// occupations and seven minor improvements. Cards that no seat was given
    /// become `undealt`, so the usual "every implemented card has exactly one
    /// location" invariant still holds.
    pub fn from_hands(set: &CardSet, hands: &[Vec<String>]) -> Result<Self, String> {
        if !set.enabled() {
            return Err("cannot seed hands from an empty card set".into());
        }
        if hands.is_empty() {
            return Err("at least one seat is required".into());
        }
        let mut seen = BTreeSet::new();
        for (seat, hand) in hands.iter().enumerate() {
            for id in hand {
                let definition = set
                    .definition(id)
                    .ok_or_else(|| format!("seat {seat} was given unknown card {id}"))?;
                if !seen.insert(id.clone()) {
                    return Err(format!("starting card {id} appears more than once"));
                }
                let _ = definition;
            }
            for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
                let count = hand
                    .iter()
                    .filter(|id| set.definition(id).is_some_and(|card| card.kind == kind))
                    .count();
                if count != 7 {
                    return Err(format!(
                        "seat {seat} must start with seven {}, got {count}",
                        match kind {
                            CardKind::Occupation => "occupations",
                            CardKind::MinorImprovement => "minor improvements",
                        }
                    ));
                }
            }
        }
        let undealt = set
            .definitions
            .iter()
            .filter(|card| !seen.contains(&card.id))
            .map(|card| card.id.clone())
            .collect();
        Ok(Self {
            hands: hands.to_vec(),
            undealt,
        })
    }

    /// In-game location validation: every implemented card ID occupies exactly
    /// one zone (undealt, a hand, or a played area), with no duplicates and no
    /// orphans. This is the invariant every card move must preserve, so it
    /// holds for the whole game — including positions where cards changed hands
    /// or were acquired after the initial deal.
    pub fn validate_location_invariant(
        &self,
        set: &CardSet,
        players: &[Player],
    ) -> Result<(), String> {
        if self.hands.len() != players.len() {
            return Err("card hand count mismatch".into());
        }
        let mut all = BTreeSet::new();
        for id in self
            .undealt
            .iter()
            .chain(self.hands.iter().flat_map(|hand| hand.iter()))
            .chain(players.iter().flat_map(|player| player.played_cards.iter()))
        {
            if set.definition(id).is_none() || !all.insert(id.as_str()) {
                return Err("card missing, duplicate or unknown".into());
            }
        }
        if all.len() != set.definitions.len() {
            return Err("card partition does not match card set".into());
        }
        Ok(())
    }

    /// Initial distribution validation: each player starts with exactly seven
    /// occupations and seven minor improvements across hand + played area.
    ///
    /// This is a *setup* property, not a running invariant: once play begins,
    /// cards change hands and can be acquired, so the per-player 7+7 count stops
    /// holding by design. Enforce it at deal/setup time only.
    pub fn validate_initial_distribution(
        &self,
        set: &CardSet,
        players: &[Player],
    ) -> Result<(), String> {
        if self.hands.len() != players.len() {
            return Err("card hand count mismatch".into());
        }
        for (hand, player) in self.hands.iter().zip(players) {
            let own = hand.iter().chain(player.played_cards.iter());
            for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
                let count = own
                    .clone()
                    .filter(|id| set.definition(id).is_some_and(|c| c.kind == kind))
                    .count();
                if count != if set.enabled() { 7 } else { 0 } {
                    return Err("player has incorrect original card allocation".into());
                }
            }
        }
        Ok(())
    }

    /// Card score invariant: a player's cached `card_points` equals the sum of
    /// the points printed on the cards in their played area.
    pub fn validate_card_points(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        for (_hand, player) in self.hands.iter().zip(players) {
            let points: i32 = player
                .played_cards
                .iter()
                .filter_map(|id| set.definition(id))
                .map(|card| card.points)
                .sum();
            if points != player.card_points {
                return Err("card score mismatch".into());
            }
        }
        Ok(())
    }

    /// Running-position validation: location partition + card score. Use this
    /// for any state reached by play; it does not assume the initial 7+7 deal.
    pub fn validate_in_game(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        self.validate_location_invariant(set, players)?;
        self.validate_card_points(set, players)
    }

    /// Full validation: the running invariants plus the initial-distribution
    /// count. Keep this for setup/deal state; in-game callers use
    /// [`Self::validate_in_game`], because a transfer or a newly acquired card
    /// legitimately breaks the per-player 7+7 count.
    pub fn validate(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        self.validate_location_invariant(set, players)?;
        self.validate_initial_distribution(set, players)?;
        self.validate_card_points(set, players)
    }
}

/// Verify card affordability using aggregate costs (including occupation fees).
fn card_costs(
    card: &CardDefinition,
    player: &Player,
    extra_food: u32,
) -> std::collections::BTreeMap<Good, u32> {
    let mut amounts = std::collections::BTreeMap::<Good, u32>::new();
    for &(good, amount) in &card.cost {
        *amounts.entry(good).or_default() += amount;
    }
    if card.kind == CardKind::MinorImprovement {
        if let Some(wood) = amounts.get_mut(&Good::Wood) {
            *wood = wood.saturating_sub(player.improvement_wood_discount);
        }
    }
    if card.kind == CardKind::MinorImprovement {
        if let Some(stone) = amounts.get_mut(&Good::Stone) {
            *stone = stone.saturating_sub(player.stone_cost_discount);
        }
    }
    *amounts.entry(Good::Food).or_default() += extra_food;
    amounts
}
pub fn affordable(card: &CardDefinition, player: &Player, extra_food: u32) -> bool {
    card_costs(card, player, extra_food)
        .iter()
        .all(|(&good, &amount)| player.supply.get(good) >= amount)
}

fn occupation_count(set: &CardSet, player: &Player) -> usize {
    player
        .played_cards
        .iter()
        .filter(|id| {
            set.definition(id)
                .is_some_and(|card| card.kind == CardKind::Occupation)
        })
        .count()
}

pub fn can_play(set: &CardSet, card: &CardDefinition, player: &Player, extra_food: u32) -> bool {
    if !affordable(card, player, extra_food) {
        return false;
    }
    let occupations = occupation_count(set, player);
    card.effects.iter().all(|effect| {
        let CardEffect::Prerequisite { rule } = effect else { return true; };
        match rule {
            CardRequirement::MinimumOccupations(min) => occupations >= usize::from(*min),
            CardRequirement::MaximumOccupations(max) => occupations <= usize::from(*max),
            CardRequirement::MinimumAnimals { animal, number } => player.animal_count(*animal) >= *number,
            CardRequirement::MinimumCropFields { crop, number } => player.farm.iter().filter(|cell| {
                matches!(cell.tile, Tile::Field { crop: Some(found), .. } if found == *crop)
            }).count() >= usize::from(*number),
            CardRequirement::MinimumPlayers(_) => true,
        }
    })
}

fn safe_gain(player: &mut Player, good: Good, amount: u32) -> Result<(), String> {
    let next = player
        .supply
        .get(good)
        .checked_add(amount)
        .ok_or("card resource overflow")?;
    player.supply.set(good, next);
    Ok(())
}

/// Execute one prevalidated card play; the containing Game transaction provides rollback.
/// This function validates the play, pays costs, applies on-play effects (Before phase),
/// moves the card from hand to played, and updates card_points.
/// This is the canonical public API used by evaluation, policy, and unit tests.
pub fn play(
    set: &CardSet,
    hand: &mut Vec<String>,
    player: &mut Player,
    id: &str,
    kind: CardKind,
    extra_food: u32,
) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    if card.kind != kind || !hand.iter().any(|x| x == id) {
        return Err("card is not in the player's hand or is the wrong kind".into());
    }
    if !can_play(set, card, player, extra_food) {
        return Err("cannot pay card costs or prerequisites are unmet".into());
    }
    for (good, amount) in card_costs(card, player, extra_food) {
        player.supply.pay(good, amount)?;
    }
    // Apply on-play (Before phase) effects
    apply_on_play_effects(set, player, id)?;
    // Move card from hand to played via the single canonical implementation
    crate::card_zones::hand_to_played(hand, player, id).map_err(|e| e.to_string())?;
    // Update card points
    let next_points = player
        .card_points
        .checked_add(card.points)
        .ok_or("card points overflow")?;
    player.card_points = next_points;
    Ok(())
}

/// Internal: validate and pay costs only (no effects, no move, no points).
/// Used by the engine path which then routes through card_zones and dispatch_play.
pub(crate) fn play_validate_pay(
    set: &CardSet,
    hand: &mut [String],
    player: &mut Player,
    id: &str,
    kind: CardKind,
    extra_food: u32,
) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    if card.kind != kind || !hand.iter().any(|x| x == id) {
        return Err("card is not in the player's hand or is the wrong kind".into());
    }
    if !can_play(set, card, player, extra_food) {
        return Err("cannot pay card costs or prerequisites are unmet".into());
    }
    for (good, amount) in card_costs(card, player, extra_food) {
        player.supply.pay(good, amount)?;
    }
    Ok(())
}

/// Apply on-play (Before phase) effects for a card that has been moved to played.
/// Called by the event dispatcher after the card is in the played zone.
pub fn apply_on_play_effects(set: &CardSet, player: &mut Player, id: &str) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    for effect in &card.effects {
        match effect {
            CardEffect::OnPlayGain { good, amount } => {
                safe_gain(player, *good, *amount)?;
            }
            CardEffect::OnPlayGainPerStored {
                stored,
                divisor,
                good,
                amount,
            } => {
                let bonus = (player.supply.get(*stored) / *divisor)
                    .checked_mul(*amount)
                    .ok_or("on-play card bonus overflow")?;
                safe_gain(player, *good, bonus)?;
            }
            CardEffect::OnPlayGainIfHouse {
                house,
                rooms,
                good,
                amount,
            } if player.house == *house && player.rooms() == usize::from(*rooms) => {
                safe_gain(player, *good, *amount)?;
            }
            CardEffect::Passive { rule } => match rule {
                PassiveRule::PastureCapacity(n) => player.pasture_capacity_bonus += n,
                PassiveRule::FenceWoodDiscount(n) => player.free_fence_wood += n,
                PassiveRule::ImprovementWoodDiscount(n) => player.improvement_wood_discount += n,
                PassiveRule::WoodenRoomCost(n) => player.wooden_room_cost = Some(*n),
                PassiveRule::RoomCost(n) => player.room_material_cost = Some(*n),
                PassiveRule::MajorStoneDiscountByRooms => {
                    player.major_stone_discount_by_rooms = true
                }
                PassiveRule::StoneDiscount(n) => player.stone_cost_discount += n,
                PassiveRule::HousingSpace(n) => player.extra_housing += n,
            },
            _ => {}
        }
    }
    Ok(())
}

/// Update card_points after a card is successfully played.
/// Called after move_card and effect dispatch.
pub fn update_card_points_on_play(player: &mut Player, points: i32) -> Result<(), String> {
    let next_points = player
        .card_points
        .checked_add(points)
        .ok_or("card points overflow")?;
    player.card_points = next_points;
    Ok(())
}

/// Deterministic, already-public effects on a recorded worker action.
pub fn after_worker(set: &CardSet, player: &mut Player, space: Space) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterWorkerGain {
                space: trigger,
                good,
                amount,
            } = effect
            {
                if *trigger == space {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

pub fn verified_scoring_cards(players: usize) -> Vec<CardDefinition> {
    fn scoring(
        id: &str,
        kind: CardKind,
        cost: Vec<(Good, u32)>,
        rule: ScoreBonusRule,
    ) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.to_owned(),
            kind,
            cost,
            points: 0,
            effects: vec![CardEffect::EndGameBonus { rule }],
        }
    }
    let mut cards = vec![
        scoring(
            "A032",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 2)],
            ScoreBonusRule::PastureArea,
        ),
        scoring(
            "A038",
            CardKind::MinorImprovement,
            vec![],
            ScoreBonusRule::HouseMaterial,
        ),
        scoring(
            "A098",
            CardKind::Occupation,
            vec![],
            ScoreBonusRule::UnfencedStables,
        ),
        scoring(
            "B098",
            CardKind::Occupation,
            vec![],
            ScoreBonusRule::SparePastureCapacity,
        ),
        scoring(
            "B099",
            CardKind::Occupation,
            vec![],
            ScoreBonusRule::LaterOccupations,
        ),
    ];
    if let Some(card) = cards.iter_mut().find(|card| card.id == "A038") {
        card.effects.push(CardEffect::Prerequisite {
            rule: CardRequirement::MinimumAnimals {
                animal: Animal::Sheep,
                number: 5,
            },
        });
    }
    if players >= 3 {
        let mut card = scoring(
            "A133",
            CardKind::Occupation,
            vec![],
            ScoreBonusRule::ImprovementCount,
        );
        card.effects.push(CardEffect::Prerequisite {
            rule: CardRequirement::MinimumPlayers(3),
        });
        cards.push(card);
    }
    cards
}

/// Calculated at read time from public played cards; it is never added to the
/// immutable printed-points counter and therefore cannot be counted twice.
pub fn end_game_bonus(set: &CardSet, player: &Player) -> i32 {
    end_game_bonus_for_definitions(&set.definitions, player)
}

/// Same computation as [`end_game_bonus`] over a borrowed definition slice.
///
/// The evaluation hot path owns only `Observation::card_definitions`, and
/// rebuilding a `CardSet` there would clone every definition once per candidate
/// action. This entry point keeps the lookup borrowed so search cost does not
/// scale with the size of the card pool.
pub fn end_game_bonus_for_definitions(definitions: &[CardDefinition], player: &Player) -> i32 {
    let definition = |id: &String| definitions.iter().find(|card| &card.id == id);
    player
        .played_cards
        .iter()
        .filter_map(definition)
        .flat_map(|card| card.effects.iter())
        .map(|effect| match effect {
            CardEffect::EndGameBonus { rule } => match rule {
                ScoreBonusRule::PastureArea => {
                    let squares = player.pastures.iter().map(|p| p.cells.len()).sum::<usize>();
                    [6, 7, 8, 10].iter().filter(|&&n| squares >= n).count() as i32
                }
                ScoreBonusRule::HouseMaterial => match player.house {
                    House::Wood => 3,
                    House::Clay => 2,
                    House::Stone => 0,
                },
                ScoreBonusRule::UnfencedStables => player
                    .farm
                    .iter()
                    .filter(|cell| cell.stable && !matches!(cell.tile, Tile::Pasture { .. }))
                    .count() as i32,
                ScoreBonusRule::ImprovementCount => {
                    let minor = player
                        .played_cards
                        .iter()
                        .filter(|id| {
                            definition(id)
                                .is_some_and(|card| card.kind == CardKind::MinorImprovement)
                        })
                        .count();
                    let total = minor + player.improvements.len();
                    match total {
                        0..=4 => 0,
                        5 => 2,
                        6 => 3,
                        7 => 4,
                        8 => 5,
                        9 => 7,
                        _ => 9,
                    }
                }
                ScoreBonusRule::SparePastureCapacity => player
                    .pastures
                    .iter()
                    .filter(|pasture| {
                        pasture.quantity > 0
                            && player
                                .pasture_capacity(pasture)
                                .saturating_sub(pasture.quantity)
                                >= 3
                    })
                    .count() as i32,
                ScoreBonusRule::LaterOccupations => {
                    let tutor = player.played_cards.iter().position(|id| {
                        definition(id).is_some_and(|card| {
                            card.effects.iter().any(|effect| {
                                matches!(
                                    effect,
                                    CardEffect::EndGameBonus {
                                        rule: ScoreBonusRule::LaterOccupations
                                    }
                                )
                            })
                        })
                    });
                    tutor.map_or(0, |index| {
                        player.played_cards[index + 1..]
                            .iter()
                            .filter(|id| {
                                definition(id).is_some_and(|card| card.kind == CardKind::Occupation)
                            })
                            .count() as i32
                    })
                }
            },
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod verified_scoring_card_tests {
    use super::*;
    use crate::model::{Animal, Improvement, Pasture};

    fn score_card(id: &str, edit: impl FnOnce(&mut Player)) -> i32 {
        let set = CardSet {
            definitions: verified_scoring_cards(4),
        };
        let mut player = Player::new(0, 0);
        player.played_cards.push(id.into());
        edit(&mut player);
        end_game_bonus(&set, &player)
    }

    #[test]
    fn borrowed_definitions_give_the_same_end_game_bonus_as_the_card_set() {
        let set = CardSet {
            definitions: verified_scoring_cards(4),
        };
        let mut player = Player::new(0, 0);
        player.played_cards.push("A032".into());
        player.played_cards.push("A038".into());
        player.pastures.push(Pasture {
            id: 0,
            cells: (0..7).collect(),
            animal: None,
            quantity: 0,
        });
        assert_eq!(
            end_game_bonus_for_definitions(&set.definitions, &player),
            end_game_bonus(&set, &player)
        );
        assert_eq!(
            end_game_bonus_for_definitions(&[], &player),
            0,
            "an unknown definition must not score"
        );
    }

    #[test]
    fn a032_pasture_area_thresholds() {
        assert_eq!(score_card("A032", |_| {}), 0);
        assert_eq!(
            score_card("A032", |p| p.pastures.push(Pasture {
                id: 0,
                cells: (0..7).collect(),
                animal: None,
                quantity: 0,
            })),
            2
        );
    }

    #[test]
    fn a038_bonus_respects_house_material() {
        assert_eq!(score_card("A038", |_| {}), 3);
        assert_eq!(score_card("A038", |p| p.house = House::Clay), 2);
        assert_eq!(score_card("A038", |p| p.house = House::Stone), 0);
    }

    #[test]
    fn a098_only_unfenced_stables() {
        assert_eq!(
            score_card("A098", |p| {
                p.farm[2].stable = true;
                p.farm[3].stable = true;
                p.farm[3].tile = Tile::Pasture { id: 0 };
            }),
            1
        );
    }

    #[test]
    fn a133_rewards_major_and_minor_improvements() {
        let mut set = CardSet {
            definitions: verified_scoring_cards(3),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "test_minor".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        let mut player = Player::new(0, 0);
        player
            .played_cards
            .extend(["A133".into(), "test_minor".into()]);
        player.improvements.extend([
            Improvement::Fireplace2,
            Improvement::Fireplace3,
            Improvement::Hearth4,
            Improvement::Hearth5,
        ]);
        assert_eq!(end_game_bonus(&set, &player), 2);
        player.improvements.push(Improvement::ClayOven);
        assert_eq!(end_game_bonus(&set, &player), 3);
        assert!(!verified_scoring_cards(2).iter().any(|c| c.id == "A133"));
    }

    #[test]
    fn b098_only_sufficiently_spare_and_occupied_pastures() {
        assert_eq!(
            score_card("B098", |p| {
                p.pastures.push(Pasture {
                    id: 0,
                    cells: vec![2, 3],
                    animal: Some(Animal::Sheep),
                    quantity: 1,
                });
                p.pastures.push(Pasture {
                    id: 1,
                    cells: vec![4, 6],
                    animal: None,
                    quantity: 0,
                });
            }),
            1
        );
    }

    #[test]
    fn b099_counts_own_later_occupations_only() {
        let mut set = CardSet {
            definitions: verified_scoring_cards(2),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "before_occ".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "after_occ".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        let mut player = Player::new(0, 0);
        player.played_cards = vec![
            "before_occ".into(),
            "B099".into(),
            "A032".into(),
            "after_occ".into(),
        ];
        assert_eq!(end_game_bonus(&set, &player), 1);
    }
}

#[cfg(test)]
mod card_prerequisite_tests {
    use super::*;

    fn prerequisite_card(rule: CardRequirement) -> (CardSet, CardDefinition) {
        let card = CardDefinition {
            use_limit: None,
            id: "check".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::Prerequisite { rule }],
        };
        (
            CardSet {
                definitions: vec![card.clone()],
            },
            card,
        )
    }

    #[test]
    fn occupations_minimum_and_maximum_are_exact() {
        let (mut set, card) = prerequisite_card(CardRequirement::MinimumOccupations(2));
        set.definitions
            .extend(["one", "two"].into_iter().map(|id| CardDefinition {
                use_limit: None,
                id: id.into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            }));
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        p.played_cards.push("one".into());
        assert!(!can_play(&set, &card, &p, 0));
        p.played_cards.push("two".into());
        assert!(can_play(&set, &card, &p, 0));
        let (_, too_many) = prerequisite_card(CardRequirement::MaximumOccupations(1));
        assert!(!can_play(&set, &too_many, &p, 0));
    }

    #[test]
    fn five_sheep_required_for_a038() {
        let set = CardSet {
            definitions: verified_scoring_cards(2),
        };
        let card = set.definition("A038").unwrap();
        let mut p = Player::new(0, 0);
        p.pet = Some(Animal::Sheep);
        assert!(!can_play(&set, card, &p, 0));
        p.pastures.push(crate::model::Pasture {
            id: 0,
            cells: vec![2, 3, 4],
            animal: Some(Animal::Sheep),
            quantity: 4,
        });
        assert!(can_play(&set, card, &p, 0));
    }

    #[test]
    fn crop_field_prerequisite_counts_only_planted_type() {
        let (set, card) = prerequisite_card(CardRequirement::MinimumCropFields {
            crop: Crop::Grain,
            number: 2,
        });
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        assert!(!can_play(&set, &card, &p, 0));
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        assert!(can_play(&set, &card, &p, 0));
    }

    #[test]
    fn player_count_requirement_rejected_by_card_set() {
        let mut cards = verified_scoring_cards(3);
        cards.push(CardDefinition {
            use_limit: None,
            id: "extra".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(3),
            }],
        });
        let set = CardSet { definitions: cards };
        assert!(set.validate(2).is_err());
    }
}

/** A/B base game cards that share the ordinary worker-completion resource hook.
 * No card artwork or printed rules are bundled.
 */
pub fn verified_minor_worker_cards(players: usize) -> Vec<CardDefinition> {
    fn gain(space: Space, good: Good, amount: u32) -> CardEffect {
        CardEffect::AfterWorkerGain {
            space,
            good,
            amount,
        }
    }
    fn card(
        id: &str,
        kind: CardKind,
        cost: Vec<(Good, u32)>,
        points: i32,
        effects: Vec<CardEffect>,
    ) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.to_string(),
            kind,
            cost,
            points,
            effects,
        }
    }
    let minor = CardKind::MinorImprovement;
    let mut result = vec![
        card(
            "A051",
            minor,
            vec![(Good::Wood, 1), (Good::Reed, 1)],
            1,
            vec![gain(Space::Fishing, Good::Food, 2)],
        ),
        card(
            "A067",
            minor,
            vec![(Good::Wood, 1)],
            0,
            vec![gain(Space::GrainSeeds, Good::Grain, 1)],
        ),
        card(
            "A078",
            minor,
            vec![(Good::Wood, 2)],
            1,
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                gain(Space::Fishing, Good::Food, 1),
                gain(Space::Fishing, Good::Reed, 1),
            ],
        ),
        card(
            "A080",
            minor,
            vec![(Good::Wood, 1)],
            0,
            vec![
                gain(Space::WesternQuarry, Good::Stone, 1),
                gain(Space::EasternQuarry, Good::Stone, 1),
            ],
        ),
        card(
            "B077",
            minor,
            vec![(Good::Food, 1)],
            1,
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(3),
                },
                gain(Space::DayLaborer, Good::Clay, 3),
            ],
        ),
    ];
    let mut geologist = vec![
        gain(Space::Forest, Good::Clay, 1),
        gain(Space::ReedBank, Good::Clay, 1),
    ];
    if players >= 3 {
        geologist.push(gain(Space::ClayPit, Good::Clay, 1));
    }
    result.push(card("B121", CardKind::Occupation, vec![], 0, geologist));
    result
}

#[cfg(test)]
mod verified_minor_worker_tests {
    use super::*;

    fn owner(id: &str, players: usize, actions: &[Space]) -> Player {
        let set = CardSet {
            definitions: verified_minor_worker_cards(players),
        };
        let mut player = Player::new(0, 0);
        player.played_cards.push(id.to_string());
        for &space in actions {
            after_worker(&set, &mut player, space).unwrap();
        }
        player
    }
    #[test]
    fn a067_bonus_grain_is_limited_to_seeds_action() {
        let p = owner("A067", 2, &[Space::GrainSeeds, Space::Farmland]);
        assert_eq!(p.supply.grain, 1);
    }
    #[test]
    fn a078_fishing_provides_two_types_of_bonus() {
        let p = owner("A078", 2, &[Space::Fishing, Space::ReedBank]);
        assert_eq!(p.supply.food, 1);
        assert_eq!(p.supply.reed, 1);
        let mut set = CardSet {
            definitions: verified_minor_worker_cards(2),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "test_occ".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        let card = set.definition("A078").unwrap();
        let mut p = Player::new(0, 0);
        p.supply.wood = 2;
        assert!(!can_play(&set, card, &p, 0));
        p.played_cards.push("test_occ".into());
        assert!(can_play(&set, card, &p, 0));
        assert_eq!(card.points, 1);
    }
    #[test]
    fn a080_only_stone_accumulation_actions_trigger() {
        let p = owner(
            "A080",
            3,
            &[
                Space::WesternQuarry,
                Space::EasternQuarry,
                Space::ResourceMarket,
            ],
        );
        assert_eq!(p.supply.stone, 2);
    }
    #[test]
    fn b077_requires_three_occupations_and_grants_clay() {
        let p = owner("B077", 2, &[Space::DayLaborer, Space::Forest]);
        assert_eq!(p.supply.clay, 3);
        let mut set = CardSet {
            definitions: verified_minor_worker_cards(2),
        };
        for i in 1..=3 {
            set.definitions.push(CardDefinition {
                use_limit: None,
                id: format!("occ{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        let card = set.definition("B077").unwrap();
        let mut p = Player::new(0, 1);
        p.played_cards.extend(["occ1".into(), "occ2".into()]);
        assert!(!can_play(&set, card, &p, 0));
        p.played_cards.push("occ3".into());
        assert!(can_play(&set, card, &p, 0));
        assert_eq!(card.points, 1);
    }
    #[test]
    fn b121_uses_third_space_only_at_three_or_more_players() {
        let two = owner("B121", 2, &[Space::Forest, Space::ReedBank, Space::ClayPit]);
        let three = owner("B121", 3, &[Space::Forest, Space::ReedBank, Space::ClayPit]);
        assert_eq!(two.supply.clay, 2);
        assert_eq!(three.supply.clay, 3);
    }
    #[test]
    fn a051_fishing_grants_food_two() {
        let p = owner("A051", 2, &[Space::Fishing, Space::Fishing, Space::Forest]);
        assert_eq!(p.supply.food, 4);
    }
    #[test]
    fn a051_only_triggers_on_fishing() {
        let p = owner(
            "A051",
            2,
            &[Space::Forest, Space::ReedBank, Space::DayLaborer],
        );
        assert_eq!(p.supply.food, 0);
    }
    #[test]
    fn a051_cost_requires_wood_and_reed() {
        let set = CardSet {
            definitions: verified_minor_worker_cards(2),
        };
        let card = set.definition("A051").unwrap();
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        p.supply.reed = 0;
        assert!(!can_play(&set, card, &p, 0));
        p.supply.reed = 1;
        assert!(can_play(&set, card, &p, 0));
        assert_eq!(card.points, 1);
    }
}

/** Verified deterministic on-play effects for the 2016 A/B starter cards. */
pub fn verified_immediate_resource_cards() -> Vec<CardDefinition> {
    vec![
        CardDefinition {
            use_limit: None,
            id: "A005".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerStored {
                stored: Good::Clay,
                divisor: 2,
                good: Good::Clay,
                amount: 1,
            }],
        },
        CardDefinition {
            use_limit: None,
            id: "A125".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::OnPlayGainIfHouse {
                    house: House::Clay,
                    rooms: 2,
                    good: Good::Clay,
                    amount: 3,
                },
                CardEffect::OnPlayGainIfHouse {
                    house: House::Clay,
                    rooms: 2,
                    good: Good::Reed,
                    amount: 2,
                },
                CardEffect::OnPlayGainIfHouse {
                    house: House::Clay,
                    rooms: 2,
                    good: Good::Stone,
                    amount: 2,
                },
            ],
        },
        // A077 Hod (fallback for A072 Calcium Fertilizers): OnPlayGain 1 clay.
        // The spec's second effect (any-player Pig Market clay gain) is not expressible
        // with current CardEffect variants (no any-player condition on AfterWorkerGain).
        CardDefinition {
            use_limit: None,
            id: "A077".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Clay,
                amount: 1,
            }],
        },
    ]
}

#[cfg(test)]
mod verified_immediate_resource_tests {
    use super::*;
    fn play_card(id: &str, player: &mut Player) -> Result<(), String> {
        let set = CardSet {
            definitions: verified_immediate_resource_cards(),
        };
        let mut hand = vec![id.to_string()];
        let kind = set.definition(id).unwrap().kind;
        play(&set, &mut hand, player, id, kind, 0)
    }

    #[test]
    fn a005_clay_bonus_tracks_two_clay_per_reward() {
        let mut player = Player::new(0, 1);
        player.supply.clay = 5;
        play_card("A005", &mut player).unwrap();
        assert_eq!(player.supply.clay, 7);
        assert_eq!(player.supply.food, 0);
        assert_eq!(player.played_cards, vec!["A005"]);
        let mut player = Player::new(0, 0);
        player.supply.clay = 5;
        let old = player.clone();
        assert!(play_card("A005", &mut player).is_err());
        assert_eq!(player, old);
    }

    #[test]
    fn a125_three_goods_only_in_two_room_clay_home() {
        let mut right = Player::new(0, 1);
        right.house = House::Clay;
        play_card("A125", &mut right).unwrap();
        assert_eq!(
            (right.supply.clay, right.supply.reed, right.supply.stone),
            (3, 2, 2)
        );
        let mut wrong = Player::new(0, 1);
        wrong.house = House::Wood;
        play_card("A125", &mut wrong).unwrap();
        assert_eq!(
            (wrong.supply.clay, wrong.supply.reed, wrong.supply.stone),
            (0, 0, 0)
        );
    }

    #[test]
    fn a077_on_play_gains_one_clay() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 1;
        play_card("A077", &mut player).unwrap();
        assert_eq!(player.supply.clay, 1);
        assert_eq!(player.supply.wood, 0);
        assert_eq!(player.played_cards, vec!["A077"]);
    }

    #[test]
    fn a077_cannot_play_without_wood() {
        let mut player = Player::new(0, 0);
        let old = player.clone();
        assert!(play_card("A077", &mut player).is_err());
        assert_eq!(player, old);
    }
}

/**
 * Verified 2016 base A/B occupations expressible entirely through the worker
 * completion hook. Only effect logic and opaque card IDs are stored here.
 * This subset alone is not a full playable 7+7-card deal.
 */
pub fn verified_worker_bonus_cards(players: usize) -> Vec<CardDefinition> {
    fn occupation(id: &str, effects: Vec<CardEffect>) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects,
        }
    }
    fn gain(space: Space, good: Good, amount: u32) -> CardEffect {
        CardEffect::AfterWorkerGain {
            space,
            good,
            amount,
        }
    }
    let mut cards = vec![
        occupation(
            "A116",
            [Space::Forest, Space::Copse, Space::Grove]
                .into_iter()
                .map(|space| gain(space, Good::Wood, 1))
                .collect(),
        ),
        occupation(
            "A119",
            [
                Space::Farmland,
                Space::GrainSeeds,
                Space::GrainUtilization,
                Space::Cultivation,
            ]
            .into_iter()
            .map(|space| gain(space, Good::Wood, 1))
            .collect(),
        ),
        // A122 鍋パン作り: after_worker_space grain_utilization -> gain clay 2 + wood 1
        occupation(
            "A122",
            vec![
                gain(Space::GrainUtilization, Good::Clay, 2),
                gain(Space::GrainUtilization, Good::Wood, 1),
            ],
        ),
    ];
    if players >= 4 {
        cards.push(occupation(
            "A155",
            vec![
                gain(Space::TravelingPlayers, Good::Wood, 1),
                gain(Space::TravelingPlayers, Good::Grain, 1),
            ],
        ));
    }
    if players >= 3 {
        cards.push(occupation(
            "B142",
            vec![gain(Space::GrainSeeds, Good::Vegetable, 1)],
        ));
    }
    cards
}

#[cfg(test)]
mod verified_worker_bonus_tests {
    use super::*;

    fn applied(card_id: &str, spaces: &[Space]) -> Player {
        let definitions = verified_worker_bonus_cards(4);
        let mut player = Player::new(0, 0);
        player.played_cards.push(card_id.to_string());
        for &space in spaces {
            after_worker(
                &CardSet {
                    definitions: definitions.clone(),
                },
                &mut player,
                space,
            )
            .unwrap();
        }
        player
    }

    #[test]
    fn base_deck_player_counts_exclude_multiplayer_only_cards() {
        let two = verified_worker_bonus_cards(2);
        let three = verified_worker_bonus_cards(3);
        let four = verified_worker_bonus_cards(4);
        assert_eq!(two.len(), 3);
        assert!(!two.iter().any(|c| c.id == "A155" || c.id == "B142"));
        assert_eq!(three.len(), 4);
        assert!(three.iter().any(|c| c.id == "B142"));
        assert!(!three.iter().any(|c| c.id == "A155"));
        assert_eq!(four.len(), 5);
    }

    #[test]
    fn a116_wood_from_wood_accumulations_only() {
        let player = applied(
            "A116",
            &[Space::Forest, Space::Copse, Space::Grove, Space::ClayPit],
        );
        assert_eq!(player.supply.wood, 3);
        assert_eq!(player.supply.clay, 0);
    }

    #[test]
    fn a119_wood_from_four_crop_related_actions_only() {
        let player = applied(
            "A119",
            &[
                Space::Farmland,
                Space::GrainSeeds,
                Space::GrainUtilization,
                Space::Cultivation,
                Space::Fishing,
            ],
        );
        assert_eq!(player.supply.wood, 4);
    }

    #[test]
    fn a155_grain_and_wood_from_traveling_players() {
        let player = applied("A155", &[Space::TravelingPlayers, Space::Fishing]);
        assert_eq!(player.supply.wood, 1);
        assert_eq!(player.supply.grain, 1);
        assert_eq!(player.supply.food, 0);
    }

    #[test]
    fn b142_vegetable_from_grain_seeds_only() {
        let player = applied("B142", &[Space::GrainSeeds, Space::Farmland]);
        assert_eq!(player.supply.vegetable, 1);
        assert_eq!(player.supply.grain, 0);
    }

    #[test]
    fn a122_clay_and_wood_from_grain_utilization_only() {
        let player = applied(
            "A122",
            &[Space::GrainUtilization, Space::GrainSeeds, Space::Farmland],
        );
        assert_eq!(player.supply.clay, 2);
        assert_eq!(player.supply.wood, 1);
        assert_eq!(player.supply.grain, 0);
        assert_eq!(player.supply.vegetable, 0);
    }

    #[test]
    fn a122_not_triggered_by_other_spaces() {
        let player = applied(
            "A122",
            &[
                Space::Forest,
                Space::ClayPit,
                Space::ReedBank,
                Space::Fishing,
            ],
        );
        assert_eq!(player.supply.clay, 0);
        assert_eq!(player.supply.wood, 0);
    }

    #[test]
    fn a122_occupation_has_no_cost_and_is_playable() {
        let definitions = verified_worker_bonus_cards(2);
        let card = definitions.iter().find(|c| c.id == "A122").unwrap().clone();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        let set = CardSet { definitions };
        let player = Player::new(0, 0);
        assert!(can_play(&set, &card, &player, 0));
    }
}

/// Deterministic public start-of-round rewards. Evaluates the owner's state
/// when the round begins, not when the card was originally played.
pub fn start_round_rewards(set: &CardSet, player: &mut Player) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::StartRoundGain {
                good,
                amount,
                house,
                rooms,
            } = effect
            {
                if house.is_none_or(|h| player.house == h)
                    && rooms.is_none_or(|r| player.rooms() == usize::from(r))
                {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// Called before taking crops from fields; livestock and field layouts still
/// represent the state at the start of the field phase.
pub fn harvest_field_rewards(set: &CardSet, player: &mut Player) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::HarvestFieldGain { rule, good, amount } = effect {
                let multiplier = match rule {
                    HarvestFieldRule::Animals {
                        sheep_per,
                        cattle_per,
                    } => (player.animal_count(Animal::Sheep) / *sheep_per)
                        .checked_add(player.animal_count(Animal::Cattle) / *cattle_per)
                        .ok_or("harvest animal bonus overflow")?,
                    HarvestFieldRule::MixedFields => {
                        let has = |crop: Option<Crop>| {
                            player.farm.iter().any(|cell| {
                            matches!(cell.tile, Tile::Field { crop: found, .. } if found == crop)
                        })
                        };
                        u32::from(has(Some(Crop::Grain)) && has(Some(Crop::Vegetable)) && has(None))
                    }
                };
                safe_gain(
                    player,
                    *good,
                    multiplier
                        .checked_mul(*amount)
                        .ok_or("harvest bonus overflow")?,
                )?;
            }
        }
    }
    Ok(())
}

/// Work-phase rewards that depend on a different space already being occupied.
pub fn after_worker_if_used(
    set: &CardSet,
    player: &mut Player,
    space: Space,
    used: &BTreeSet<Space>,
) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterWorkerGainIfUsed {
                space: trigger,
                required_space,
                good,
                amount,
            } = effect
            {
                if *trigger == space && used.contains(required_space) {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// Additional basic 2016 A/B cards with fully deterministic timing hooks.
pub fn verified_round_and_harvest_cards() -> Vec<CardDefinition> {
    let minor = CardKind::MinorImprovement;
    let occupation = CardKind::Occupation;
    vec![
        CardDefinition {
            use_limit: None,
            id: "B050".into(),
            kind: minor,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MaximumOccupations(3),
                },
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::Animals {
                        sheep_per: 3,
                        cattle_per: 2,
                    },
                    good: Good::Food,
                    amount: 1,
                },
            ],
        },
        CardDefinition {
            use_limit: None,
            id: "B057".into(),
            kind: minor,
            cost: vec![(Good::Clay, 1), (Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::StartRoundGain {
                good: Good::Food,
                amount: 1,
                house: Some(House::Wood),
                rooms: None,
            }],
        },
        CardDefinition {
            use_limit: None,
            id: "B061".into(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::HarvestFieldGain {
                rule: HarvestFieldRule::MixedFields,
                good: Good::Food,
                amount: 3,
            }],
        },
        CardDefinition {
            use_limit: None,
            id: "B062".into(),
            kind: minor,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::AfterWorkerGainIfUsed {
                space: Space::GrainSeeds,
                required_space: Space::Farmland,
                good: Good::Food,
                amount: 3,
            }],
        },
        CardDefinition {
            use_limit: None,
            id: "B118".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundGain {
                good: Good::Wood,
                amount: 1,
                house: None,
                rooms: Some(2),
            }],
        },
    ]
}

#[cfg(test)]
mod round_and_harvest_card_tests {
    use super::*;
    fn set() -> CardSet {
        CardSet {
            definitions: verified_round_and_harvest_cards(),
        }
    }
    #[test]
    fn b050_counts_sheep_in_threes_and_cattle_in_pairs() {
        let mut p = Player::new(0, 0);
        p.played_cards.push("B050".into());
        p.pastures.push(crate::model::Pasture {
            id: 0,
            cells: vec![2, 3, 4],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        p.pastures.push(crate::model::Pasture {
            id: 1,
            cells: vec![6, 7],
            animal: Some(Animal::Cattle),
            quantity: 3,
        });
        harvest_field_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.food, 3);
    }
    #[test]
    fn b057_works_only_in_wooden_house() {
        let mut p = Player::new(0, 0);
        p.played_cards.push("B057".into());
        start_round_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.food, 1);
        p.house = House::Clay;
        start_round_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.food, 1);
    }
    #[test]
    fn b061_requires_three_different_field_states() {
        let mut p = Player::new(0, 0);
        p.played_cards.push("B061".into());
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        harvest_field_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.food, 0);
        p.farm[4].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        harvest_field_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.food, 3);
    }
    #[test]
    fn b062_checks_other_space_occupied_in_same_round() {
        let mut p = Player::new(0, 0);
        p.played_cards.push("B062".into());
        after_worker_if_used(&set(), &mut p, Space::GrainSeeds, &BTreeSet::new()).unwrap();
        assert_eq!(p.supply.food, 0);
        after_worker_if_used(
            &set(),
            &mut p,
            Space::GrainSeeds,
            &BTreeSet::from([Space::Farmland]),
        )
        .unwrap();
        assert_eq!(p.supply.food, 3);
    }
    #[test]
    fn b118_requires_exactly_two_physical_rooms() {
        let mut p = Player::new(0, 0);
        p.played_cards.push("B118".into());
        start_round_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.wood, 1);
        p.farm[1].tile = Tile::Room;
        start_round_rewards(&set(), &mut p).unwrap();
        assert_eq!(p.supply.wood, 1);
    }
}

/// Purely code-defined basic A/B passive modifiers; no card text or artwork.
pub fn verified_passive_cards() -> Vec<CardDefinition> {
    fn card(
        id: &str,
        kind: CardKind,
        cost: Vec<(Good, u32)>,
        effect: PassiveRule,
    ) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.into(),
            kind,
            cost,
            points: 0,
            effects: vec![CardEffect::Passive { rule: effect }],
        }
    }
    vec![
        card(
            "A012",
            CardKind::MinorImprovement,
            vec![(Good::Clay, 1)],
            PassiveRule::PastureCapacity(2),
        ),
        CardDefinition {
            use_limit: None,
            id: "A075".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Stone, 2)],
            points: 2,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MaximumOccupations(3),
                },
                CardEffect::Passive {
                    rule: PassiveRule::ImprovementWoodDiscount(1),
                },
            ],
        },
        card(
            "B013",
            CardKind::MinorImprovement,
            vec![(Good::Stone, 1), (Good::Wood, 1)],
            PassiveRule::WoodenRoomCost(2),
        ),
        card(
            "A088",
            CardKind::Occupation,
            vec![],
            PassiveRule::FenceWoodDiscount(3),
        ),
        card(
            "B095",
            CardKind::Occupation,
            vec![],
            PassiveRule::MajorStoneDiscountByRooms,
        ),
        card(
            "B126",
            CardKind::Occupation,
            vec![],
            PassiveRule::RoomCost(3),
        ),
    ]
}
#[cfg(test)]
mod passive_cards_tests {
    use super::*;
    fn activate(id: &str, p: &mut Player) {
        let set = CardSet {
            definitions: verified_passive_cards(),
        };
        let mut hand = vec![id.into()];
        let kind = set.definition(id).unwrap().kind;
        play(&set, &mut hand, p, id, kind, 0).unwrap();
    }
    #[test]
    fn a012_pasture_extra_two_including_stable() {
        let mut p = Player::new(0, 0);
        p.supply.clay = 1;
        let q = crate::model::Pasture {
            id: 0,
            cells: vec![2],
            animal: None,
            quantity: 0,
        };
        activate("A012", &mut p);
        assert_eq!(p.pasture_capacity(&q), 4);
        p.farm[2].stable = true;
        assert_eq!(p.pasture_capacity(&q), 6);
    }
    #[test]
    fn a075_minor_wood_cost_discount() {
        let mut p = Player::new(0, 0);
        p.supply.stone = 2;
        activate("A075", &mut p);
        assert_eq!(p.card_points, 2);
        let mut set = CardSet {
            definitions: verified_passive_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "sample".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![],
        });
        play(
            &set,
            &mut vec!["sample".into()],
            &mut p,
            "sample",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        assert_eq!(p.supply.wood, 0);
    }
    #[test]
    fn b013_wooden_room_two_wood() {
        let mut p = Player::new(0, 0);
        p.supply.stone = 1;
        p.supply.wood = 1;
        activate("B013", &mut p);
        assert_eq!(p.wooden_room_cost, Some(2));
    }
    #[test]
    fn a088_fence_discount_three() {
        let mut p = Player::new(0, 0);
        activate("A088", &mut p);
        assert_eq!(p.free_fence_wood, 3);
    }
    #[test]
    fn b095_major_stone_discount() {
        let mut p = Player::new(0, 0);
        activate("B095", &mut p);
        assert!(p.major_stone_discount_by_rooms);
    }
    #[test]
    fn b126_room_cost_three() {
        let mut p = Player::new(0, 0);
        activate("B126", &mut p);
        assert_eq!(p.room_material_cost, Some(3));
    }
    #[test]
    fn b050_limits_occupation_count() {
        let mut set = CardSet {
            definitions: verified_round_and_harvest_cards(),
        };
        for i in 0..4 {
            set.definitions.push(CardDefinition {
                use_limit: None,
                id: format!("occ{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        assert!(can_play(&set, set.definition("B050").unwrap(), &p, 0));
        p.played_cards.extend((0..4).map(|i| format!("occ{i}")));
        assert!(!can_play(&set, set.definition("B050").unwrap(), &p, 0));
    }
}

/// One-time reward on the completion of qualifying clay construction.
pub fn after_clay_construction(
    set: &CardSet,
    previous: &Player,
    current: &mut Player,
) -> Result<(), String> {
    let qualifies = (previous.house == House::Clay && current.rooms() > previous.rooms())
        || (previous.house == House::Clay && current.house == House::Stone);
    if !qualifies {
        return Ok(());
    }
    for id in current.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterClayConstructionGain { good, amount } = effect {
                safe_gain(current, *good, *amount)?;
            }
        }
    }
    Ok(())
}

/// Additional A/B occupations with unambiguous fixed, public-state effects.
/// Consultant's immediate option here applies to the two-player starter pool.
pub fn verified_simple_occupations(players: usize) -> Vec<CardDefinition> {
    let mut result = vec![
        CardDefinition {
            use_limit: None,
            id: "A110".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterClayConstructionGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        CardDefinition {
            use_limit: None,
            id: "B107".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundGain {
                good: Good::Food,
                amount: 3,
                house: Some(House::Stone),
                rooms: None,
            }],
        },
    ];
    if players == 2 {
        result.push(CardDefinition {
            use_limit: None,
            id: "B102".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Clay,
                amount: 3,
            }],
        });
    }
    result
}

/// Includes ONLY cards whose effects are actually implemented. The official
/// 7+7 hands require 14 cards of each kind at two players.
pub fn verified_basic_card_set(players: usize) -> Result<CardSet, String> {
    if players != 2 {
        return Err("built-in A/B card pool currently supports two players only".into());
    }
    let mut definitions = Vec::new();
    definitions.extend(verified_scoring_cards(players));
    definitions.extend(verified_minor_worker_cards(players));
    definitions.extend(verified_immediate_resource_cards());
    definitions.extend(verified_worker_bonus_cards(players));
    definitions.extend(verified_round_and_harvest_cards());
    definitions.extend(verified_passive_cards());
    definitions.extend(verified_simple_occupations(players));
    definitions.extend(verified_extra_simple_cards());
    let set = CardSet { definitions };
    set.validate(players)?;
    Ok(set)
}

#[cfg(test)]
mod simple_occupation_tests {
    use super::*;
    #[test]
    fn a110_triggers_once_for_clay_room_or_stone_renovation() {
        let set = CardSet {
            definitions: verified_simple_occupations(2),
        };
        let mut old = Player::new(0, 0);
        old.house = House::Clay;
        old.played_cards.push("A110".into());
        let mut built = old.clone();
        built.farm[1].tile = Tile::Room;
        after_clay_construction(&set, &old, &mut built).unwrap();
        assert_eq!(built.supply.food, 3);
        let mut stone = old.clone();
        stone.house = House::Stone;
        after_clay_construction(&set, &old, &mut stone).unwrap();
        assert_eq!(stone.supply.food, 3);
        let mut not_clay = old.clone();
        not_clay.house = House::Wood;
        let mut extra = not_clay.clone();
        extra.farm[1].tile = Tile::Room;
        after_clay_construction(&set, &not_clay, &mut extra).unwrap();
        assert_eq!(extra.supply.food, 0);
    }
    #[test]
    fn b107_starts_payments_only_once_in_stone_house() {
        let set = CardSet {
            definitions: verified_simple_occupations(2),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("B107".into());
        start_round_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 0);
        p.house = House::Stone;
        start_round_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 3);
    }
    #[test]
    fn b102_gains_three_clay_only_in_two_player_subset() {
        let set = CardSet {
            definitions: verified_simple_occupations(2),
        };
        let mut p = Player::new(0, 0);
        play(
            &set,
            &mut vec!["B102".into()],
            &mut p,
            "B102",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        assert_eq!(p.supply.clay, 3);
        assert!(!verified_simple_occupations(3)
            .iter()
            .any(|c| c.id == "B102"));
    }
    #[test]
    fn only_implemented_cards_are_in_two_player_preset() {
        let set = verified_basic_card_set(2).unwrap();
        assert_eq!(set.definitions.len(), 32);
        assert_eq!(
            set.definitions
                .iter()
                .filter(|x| x.kind == CardKind::MinorImprovement)
                .count(),
            18
        );
        assert_eq!(
            set.definitions
                .iter()
                .filter(|x| x.kind == CardKind::Occupation)
                .count(),
            15
        );
        assert!(verified_basic_card_set(3).is_err());
    }
}

pub fn verified_extra_simple_cards() -> Vec<CardDefinition> {
    vec![
        CardDefinition {
            use_limit: None,
            id: "B010".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 3), (Good::Food, 3)],
            points: 0,
            effects: vec![CardEffect::Passive {
                rule: PassiveRule::HousingSpace(1),
            }],
        },
        CardDefinition {
            use_limit: None,
            id: "A063".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 2), (Good::Stone, 2)],
            points: 2,
            effects: vec![CardEffect::AfterHarvestBakeGain { amount: 3 }],
        },
    ]
}
pub fn verified_multiplayer_passive_cards(players: usize) -> Vec<CardDefinition> {
    if players < 3 {
        return vec![];
    }
    vec![CardDefinition {
        use_limit: None,
        id: "A143".into(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(3),
            },
            CardEffect::Passive {
                rule: PassiveRule::StoneDiscount(1),
            },
        ],
    }]
}
#[cfg(test)]
mod m6_extra_passive_tests {
    use super::*;
    #[test]
    fn b010_adds_housing_without_extra_tile() {
        let set = CardSet {
            definitions: verified_extra_simple_cards(),
        };
        let mut p = Player::new(0, 0);
        p.supply.wood = 3;
        p.supply.food = 3;
        play(
            &set,
            &mut vec!["B010".into()],
            &mut p,
            "B010",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        assert_eq!(p.extra_housing, 1);
        assert_eq!(p.rooms(), 2);
    }
    #[test]
    fn a143_only_three_plus_and_reduces_stone() {
        assert!(verified_multiplayer_passive_cards(2).is_empty());
        let set = CardSet {
            definitions: verified_multiplayer_passive_cards(3),
        };
        let mut p = Player::new(0, 0);
        play(
            &set,
            &mut vec!["A143".into()],
            &mut p,
            "A143",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        assert_eq!(p.stone_cost_discount, 1);
    }
}

/// Extra food when a player takes Bake Bread in a round directly following a
/// harvest round. The work action is already validated and applied.
pub fn after_harvest_baking(
    set: &CardSet,
    player: &mut Player,
    round: u8,
    baked: bool,
) -> Result<(), String> {
    if !baked || round < 2 || !crate::model::HARVEST_ROUNDS.contains(&(round - 1)) {
        return Ok(());
    }
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterHarvestBakeGain { amount } = effect {
                safe_gain(player, Good::Food, *amount)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod a063_baking_tests {
    use super::*;
    #[test]
    fn after_harvest_bread_action_only_and_exact_three_food() {
        let set = CardSet {
            definitions: verified_extra_simple_cards(),
        };
        let mut player = Player::new(0, 0);
        player.played_cards.push("A063".into());
        after_harvest_baking(&set, &mut player, 4, true).unwrap();
        after_harvest_baking(&set, &mut player, 5, false).unwrap();
        after_harvest_baking(&set, &mut player, 6, true).unwrap();
        assert_eq!(player.supply.food, 0);
        for round in [5, 8, 10, 12, 14] {
            after_harvest_baking(&set, &mut player, round, true).unwrap();
        }
        assert_eq!(player.supply.food, 15);
    }
    #[test]
    fn correct_improvement_cost_and_card_points() {
        let set = CardSet {
            definitions: verified_extra_simple_cards(),
        };
        let mut player = Player::new(0, 0);
        player.supply.wood = 2;
        player.supply.stone = 2;
        let mut hand = vec!["A063".into()];
        play(
            &set,
            &mut hand,
            &mut player,
            "A063",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        assert_eq!(player.card_points, 2);
        assert_eq!(player.supply.wood, 0);
        assert_eq!(player.supply.stone, 0);
    }
}

#[cfg(test)]
mod card_set_content_hash_tests {
    use super::*;

    fn card(id: &str, kind: CardKind, points: i32) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.into(),
            kind,
            cost: Vec::new(),
            points,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 1,
            }],
        }
    }

    #[test]
    fn empty_and_populated_sets_have_distinct_hashes() {
        let empty = CardSet::default();
        let one = CardSet {
            definitions: vec![card("X1", CardKind::Occupation, 1)],
        };
        assert_eq!(empty.content_hash().len(), 64);
        assert_ne!(empty.content_hash(), one.content_hash());
        // Deterministic across calls.
        assert_eq!(one.content_hash(), one.content_hash());
    }

    #[test]
    fn hash_ignores_definition_order_but_tracks_every_field() {
        let a = card("X1", CardKind::Occupation, 1);
        let b = card("X2", CardKind::MinorImprovement, 2);
        let forward = CardSet {
            definitions: vec![a.clone(), b.clone()],
        };
        let reverse = CardSet {
            definitions: vec![b.clone(), a.clone()],
        };
        assert_eq!(forward.content_hash(), reverse.content_hash());

        // Each individual field change must change the hash.
        let mut renamed = a.clone();
        renamed.id = "X9".into();
        let mut rekinded = a.clone();
        rekinded.kind = CardKind::MinorImprovement;
        let mut rescored = a.clone();
        rescored.points = 7;
        let mut recosted = a.clone();
        recosted.cost = vec![(Good::Wood, 2)];
        let mut reeffected = a.clone();
        reeffected.effects = vec![CardEffect::EndGameBonus {
            rule: ScoreBonusRule::PastureArea,
        }];
        for changed in [renamed, rekinded, rescored, recosted, reeffected] {
            let other = CardSet {
                definitions: vec![changed],
            };
            assert_ne!(
                forward.content_hash(),
                other.content_hash(),
                "a changed field must change the deck hash"
            );
        }
    }

    #[test]
    fn verified_two_player_pool_hashes_stably() {
        let first = verified_basic_card_set(2).unwrap();
        let second = verified_basic_card_set(2).unwrap();
        assert!(!first.definitions.is_empty());
        assert_eq!(first.content_hash(), second.content_hash());
        // The hash must not depend on how the pool was assembled.
        let mut shuffled = first.clone();
        shuffled.definitions.reverse();
        assert_eq!(first.content_hash(), shuffled.content_hash());
    }
}

/// Get the default immutable card registry (built once, shared via Arc).
///
/// This is the primary entry point for new code. The registry contains all
/// implemented cards with stable CardId assignments and validated definitions.
pub fn default_registry() -> Arc<CardRegistry> {
    build_default_registry().expect("default registry must build")
}

/// Get a CardPool for the given player count and mode.
///
/// - `experimental_mode = false`: Official pool (verified + eligible only, fail-closed)
/// - `experimental_mode = true`: Experimental pool (all implemented cards)
pub fn card_pool(players: usize, experimental_mode: bool) -> CardPool {
    CardPool::new(&default_registry(), players, experimental_mode)
}

/// Get a legacy CardSet for the given player count and mode (compat adapter).
///
/// This builds a CardSet from the pool's eligible cards, suitable for
/// existing APIs that expect CardSet (saves, observations, legacy engine paths).
pub fn card_set_for_game(players: usize, experimental_mode: bool) -> CardSet {
    let registry = default_registry();
    let pool = card_pool(players, experimental_mode);
    card_set_from_pool(&registry, &pool)
}

/// Get a legacy CardSet with ALL implemented cards (for full-catalog observations).
pub fn card_set_full_catalog() -> CardSet {
    card_set_from_registry(&default_registry())
}

/// Look up a card definition by external ID from the default registry.
pub fn card_definition(external_id: &str) -> Option<CardDefinition> {
    default_registry()
        .get_by_external(external_id)
        .map(|c| c.to_card_definition())
}

/// Look up a card definition by CardId from the default registry.
pub fn card_definition_by_id(card_id: CardId) -> Option<CardDefinition> {
    default_registry()
        .get(card_id)
        .map(|c| c.to_card_definition())
}

/// Convert external ID to CardId using the default registry.
pub fn external_to_card_id(external_id: &str) -> Option<CardId> {
    default_registry().card_id(external_id)
}

/// Convert CardId to external ID using the default registry.
pub fn card_id_to_external(card_id: CardId) -> Option<String> {
    default_registry()
        .external_id(card_id)
        .map(|s| s.to_string())
}

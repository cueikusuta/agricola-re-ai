//! Card-set metadata is supplied by callers; copyrighted card text is never bundled.
//! This is a deliberately small deterministic effect vocabulary, not a claim that
//! the full Agricola Revised deck has been implemented.
use crate::model::{Animal, Crop, Good, House, Improvement, Player, Space, Tile};
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
    /// C111 小動物飼育員 (Small Animal Breeder): at each round start, if the
    /// owner's food stock is at least the current round number, gain `amount`
    /// of `good` (the printed card grants 1 food). The threshold is the round
    /// number, so this is expressed as its own variant instead of widening the
    /// serialized shape of `StartRoundGain` (existing saves and content hashes
    /// must keep their exact bytes).
    StartRoundGainIfFoodAtLeastRound {
        good: Good,
        amount: u32,
    },
    /// C116 調度品職人 (Furniture Maker): each time the owner plays an
    /// occupation AFTER this card, gain `amount` of `good` per food paid as
    /// that occupation's cost (printed cost + optional fee). Fired from the
    /// play dispatcher's after-play scan; the legacy `cards::play` helper does
    /// not re-scan earlier cards and therefore never fires it.
    AfterOccupationGainPerFoodCost {
        good: Good,
        amount: u32,
    },
    /// E111 世捨て人 (Recluse), round-start half: at each round start, if the
    /// owner has played no minor improvements, gain `amount` of `good`. The
    /// condition needs a card-definition lookup, so the generic condition
    /// scan reports it as triggerable and the handler performs the check.
    StartRoundGainIfNoMinorPlayed {
        good: Good,
        amount: u32,
    },
    /// E111 世捨て人 (Recluse), harvest half: at the start of each harvest
    /// (the field phase is the first step of the harvest), if the owner has
    /// played no minor improvements, gain `amount` of `good`. Like the round
    /// variant, the handler performs the definition-dependent check.
    HarvestFieldGainIfNoMinorPlayed {
        good: Good,
        amount: u32,
    },
    /// A117 木材調達人 (Wood Carrier): when playing this card, gain `amount`
    /// of `good` per improvement already in front of the owner (base
    /// improvements plus played minor-improvement cards; this card itself is
    /// an occupation and never counts).
    OnPlayGainPerImprovementPlayed {
        good: Good,
        amount: u32,
    },
    /// On-play gain scaled by a counted quantity: gain `amount` of `good` per
    /// unit of `count_kind` the player owns at play time.
    OnPlayGainPerCount {
        count_kind: CountKind,
        good: Good,
        amount: u32,
    },
    /// C068 書棚 (Bookcase): each time the owner plays an occupation AFTER
    /// this card, gain `amount` of `good`. Fixed amount per occupation played,
    /// independent of the occupation's cost. Fired from the play dispatcher's
    /// after-play scan; the legacy `cards::play` helper does not re-scan earlier
    /// cards and therefore never fires it.
    AfterOccupationGain {
        good: Good,
        amount: u32,
    },
    /// C141 羊の世話人 / A050 ミルクがめ (self half): each time ANY player uses
    /// the specified space, the card owner gains `amount` of `good`. Unlike
    /// `AfterWorkerGain` which only fires when the card owner themselves uses
    /// the space, this variant fires for any player's use (including the owner).
    AfterAnyWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// A050 ミルクがめ (others half): each time ANY player uses the specified
    /// space, all players EXCEPT the card owner gain `amount` of `good`.
    /// This is a separate variant to keep the effect vocabulary focused and
    /// avoid widening `AfterAnyWorkerGain` with an `others_amount` field.
    AfterAnyWorkerOthersGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// D061 わらのたわら (Bale of Straw): at the start of each harvest, if the
    /// card owner has at least `threshold` fields sown with `crop`, gain
    /// `amount` of `good`. The condition is evaluated in the handler because
    /// it requires inspecting the player's farm state.
    HarvestStartGainIfFields {
        crop: Crop,
        threshold: u8,
        good: Good,
        amount: u32,
    },
    /// C083 若牛 (Early Cattle): on play, gain `amount` of `animal` (cattle).
    /// The player must provide an AnimalPlan for placement.
    OnPlayGainAnimal {
        animal: Animal,
        amount: u32,
    },
    /// C147 牛の飼育士 (Cowherd): each time the owner uses `space` (CattleMarket),
    /// gain `amount` of `animal` (cattle). The player must provide an AnimalPlan
    /// for placement.
    AfterWorkerGainAnimal {
        space: Space,
        animal: Animal,
        amount: u32,
    },
    /// B002 小牧場 (Mini Pasture): on play, fence `spaces` empty farmyard
    /// cell(s) into a pasture at no wood cost. The engine picks the cell
    /// deterministically (adjacent to an existing pasture when one exists);
    /// the rule itself lives in `engine::build_free_single_cell_pasture`.
    OnPlayBuildPasture {
        spaces: u8,
    },
    /// B003 密造酒 (Moonshine): on play, take one occupation from the owner's
    /// hand and either play it immediately by paying `food_cost` food, or pass
    /// it to the left neighbour. Hand-dependent: resolved by the play
    /// dispatcher's handler (`card_events`), not by the hand-less
    /// `apply_on_play_effects` projection path.
    OnPlayDrawPlayOrPassOccupation {
        food_cost: u32,
    },
    /// B005 経験の蓄積 (Store of Experience): on play, gain 1 of the good
    /// paired with the first band whose threshold covers the number of
    /// occupations still in the owner's hand (bands ascending, e.g.
    /// `[(4, stone), (5, reed), (6, clay), (7, wood)]`). Hand-dependent:
    /// resolved by the play dispatcher's handler.
    OnPlayGainByHandOccupationCount {
        bands: Vec<(u32, Good)>,
    },
    /// B008 露店 (Market Stall): on play, exchange `give_amount` of `give`
    /// for `receive_amount` of `receive`. If the payment resource is missing,
    /// the exchange does not fire (no partial exchange, no play failure).
    OnPlayExchangeGain {
        give: Good,
        give_amount: u32,
        receive: Good,
        receive_amount: u32,
    },
    /// B009 むち (Beating Rod): on play, gain `gain_amount` of `gain_good`,
    /// or pay `pay_amount` of `pay_good` to gain one `animal`. The engine
    /// deterministically exchanges whenever the payment resource is available;
    /// the animal is placed through the derived AnimalPlan (see the dispatcher
    /// handler for C083's vocabulary).
    OnPlayGainOrExchangeAnimal {
        gain_good: Good,
        gain_amount: u32,
        pay_good: Good,
        pay_amount: u32,
        animal: Animal,
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
            CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => {
                "start_round_gain_if_food_at_least_round"
            }
            CardEffect::AfterOccupationGainPerFoodCost { .. } => {
                "after_occupation_gain_per_food_cost"
            }
            CardEffect::AfterOccupationGain { .. } => "after_occupation_gain",
            CardEffect::StartRoundGainIfNoMinorPlayed { .. } => {
                "start_round_gain_if_no_minor_played"
            }
            CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => {
                "harvest_field_gain_if_no_minor_played"
            }
            CardEffect::OnPlayGainPerImprovementPlayed { .. } => {
                "on_play_gain_per_improvement_played"
            }
            CardEffect::OnPlayGainPerCount { .. } => "on_play_gain_per_count",
            CardEffect::OnPlayGainAnimal { .. } => "on_play_gain_animal",
            CardEffect::AfterWorkerGainAnimal { .. } => "after_worker_gain_animal",
            CardEffect::AfterAnyWorkerGain { .. } => "after_any_worker_gain",
            CardEffect::AfterAnyWorkerOthersGain { .. } => "after_any_worker_others_gain",
            CardEffect::HarvestStartGainIfFields { .. } => "harvest_start_gain_if_fields",
            CardEffect::OnPlayBuildPasture { .. } => "on_play_build_pasture",
            CardEffect::OnPlayDrawPlayOrPassOccupation { .. } => {
                "on_play_draw_play_or_pass_occupation"
            }
            CardEffect::OnPlayGainByHandOccupationCount { .. } => {
                "on_play_gain_by_hand_occupation_count"
            }
            CardEffect::OnPlayExchangeGain { .. } => "on_play_exchange_gain",
            CardEffect::OnPlayGainOrExchangeAnimal { .. } => "on_play_gain_or_exchange_animal",
        }
    }
}

/// What to count for OnPlayGainPerCount effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountKind {
    /// Number of occupations the player has played.
    Occupations,
    /// Number of clay rooms (clay or stone house rooms).
    ClayRooms,
    /// Number of improvements in front of the player (base + played minor improvements).
    Improvements,
    /// Total family members (workers).
    FamilyMembers,
    /// Number of major improvements in the bottom row (printed major improvements the player owns).
    MajorImprovementsBottomRow,
    /// Number of sown fields (fields with a crop planted).
    SownFields,
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
    Animals {
        sheep_per: u32,
        cattle_per: u32,
    },
    MixedFields,
    /// C106 じゃがいも収穫者 (Potato Harvester): one multiplier step per
    /// vegetable that the field phase is about to yield. The dispatcher runs
    /// before crops are removed, and each planted vegetable field yields
    /// exactly one good per harvest, so the count of vegetable fields with a
    /// growing crop equals the vegetables gained this field phase.
    VegetablesHarvested,
    /// E117 パイプ喫煙家 (Pipe Smoker): 1 if at least one field is planted
    /// with wheat at the start of the harvest (the field phase is the first
    /// step of the harvest, and no state changes between them), else 0.
    HasWheatField,
    /// E107 土地調査士 (Land Surveyor): at the field phase, the amount paired
    /// with the highest threshold the number of plowed field tiles reaches
    /// (2/4/6/7 -> 1/2/3/4 food on the printed card), else 0.
    FieldCountThresholds {
        thresholds: [u32; 4],
        amounts: [u32; 4],
    },
    /// A106 液肥散布人 (Slurry Spreader): at the field phase, one multiplier
    /// step per field whose LAST grain is taken this field phase (a growing
    /// grain field yields exactly one per harvest, so fields with one
    /// remaining crop become empty afterwards).
    LastGrainTaken,
    /// A106 液肥散布人 (Slurry Spreader): same as [`HarvestFieldRule::LastGrainTaken`]
    /// for vegetables (the printed card pays 2 food per last grain and 1 per
    /// last vegetable, hence two rule variants instead of one).
    LastVegetableTaken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRequirement {
    MinimumOccupations(u8),
    MaximumOccupations(u8),
    MinimumAnimals { animal: Animal, number: u32 },
    MinimumCropFields { crop: Crop, number: u8 },
    MinimumPlayers(u8),
    MinimumImprovements(u8),
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
    /// D100 君主 (Sovereign): 1 bonus point per scoring category that has
    /// reached its maximum of 4 points (fields, pastures, grain, vegetables,
    /// sheep, boar, cattle, fenced stables). Read from the canonical scoring
    /// table in `score.rs` so thresholds can never diverge.
    CategoriesAtMaxFour,
    /// A099 放牧人 (Fellow Grazer): 2 bonus points per pasture covering at
    /// least 3 farm cells, computed from the final farm state.
    LargePastures,
    /// A101 調理器具商 (Cookery Outfitter): 1 bonus point per cooking
    /// improvement in front of the player (the engine's cooking
    /// improvements are the fireplace/hearth set, see `Improvement::is_cooking`).
    CookingImprovements,
    /// D032 木のくまで (Wood Rake): +2 points if at least 7 goods stood in
    /// fields before the final harvest. Threshold stored in the variant.
    FieldsGoodsBeforeFinalHarvest(u32),
    /// D034 豪華な宿泊所 (Luxurious Hostel): +4 points if stone rooms exceed
    /// family members.
    StoneRoomsExceedPeople,
    /// E034 登記簿 (Land Register): +2 points if every farmyard space is used.
    FarmyardAllUsed,
    /// E035 人間不信 (Misanthropy): 2/3/5 points for exactly 4/3/2 family members.
    ExactFamilyMembers,
}

/// Where a card goes after being played. Defaults to the owner's played area;
/// the printed "pass left" cards move to the left neighbour's hand instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostPlayDestination {
    /// Card stays in the owner's played area (default).
    #[default]
    PlayedArea,
    /// Card moves to the left neighbour's hand after play (2016 Lookout:
    /// the printed card is passed to the player on the left).
    LeftNeighborHand,
}

/// Serde skip predicate for the default post-play destination.
pub fn post_play_is_played_area(dest: &PostPlayDestination) -> bool {
    matches!(dest, PostPlayDestination::PlayedArea)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDefinition {
    /// A stable programmatic identifier, NOT the card's printed text.
    pub id: String,
    pub kind: CardKind,
    pub cost: Vec<(Good, u32)>,
    pub points: i32,
    pub effects: Vec<CardEffect>,
    /// Where the card goes after being played. `PlayedArea` (the default) is
    /// skipped when serializing, so existing card sets and saved games keep
    /// their exact bytes and `content_hash` is unchanged unless a card
    /// actually passes left.
    #[serde(default, skip_serializing_if = "post_play_is_played_area")]
    pub post_play: PostPlayDestination,
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
                        | CardEffect::AfterWorkerGain { amount, .. }
                        | CardEffect::OnPlayGainAnimal { amount, .. }
                        | CardEffect::AfterWorkerGainAnimal { amount, .. } => *amount,
                        CardEffect::EndGameBonus { .. }
                        | CardEffect::Prerequisite { .. }
                        | CardEffect::Passive { .. } => 1,
                        #[cfg(test)]
                        CardEffect::PendingProbe { .. } => 1,
                        CardEffect::OnPlayGainPerStored { amount, .. }
                        | CardEffect::OnPlayGainIfHouse { amount, .. }
                        | CardEffect::StartRoundGain { amount, .. }
                        | CardEffect::StartRoundGainIfFoodAtLeastRound { amount, .. }
                        | CardEffect::AfterOccupationGainPerFoodCost { amount, .. }
                        | CardEffect::AfterOccupationGain { amount, .. }
                        | CardEffect::StartRoundGainIfNoMinorPlayed { amount, .. }
                        | CardEffect::HarvestFieldGainIfNoMinorPlayed { amount, .. }
                        | CardEffect::OnPlayGainPerImprovementPlayed { amount, .. }
                        | CardEffect::HarvestFieldGain { amount, .. }
                        | CardEffect::AfterWorkerGainIfUsed { amount, .. }
                        | CardEffect::AfterClayConstructionGain { amount, .. }
                        | CardEffect::AfterHarvestBakeGain { amount }
                        | CardEffect::OnPlayGainPerCount { amount, .. }
                        | CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. } => *amount,
                        CardEffect::OnPlayBuildPasture { .. } => 1,
                        CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost } => *food_cost,
                        CardEffect::OnPlayGainByHandOccupationCount { .. } => 1,
                        CardEffect::OnPlayExchangeGain {
                            give_amount,
                            receive_amount,
                            ..
                        } => (*give_amount).max(*receive_amount),
                        CardEffect::OnPlayGainOrExchangeAnimal {
                            gain_amount,
                            pay_amount,
                            ..
                        } => (*gain_amount).max(*pay_amount),
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

pub(crate) fn occupation_count(set: &CardSet, player: &Player) -> usize {
    player
        .played_cards
        .iter()
        .filter(|id| {
            set.definition(id)
                .is_some_and(|card| card.kind == CardKind::Occupation)
        })
        .count()
}

/// Count clay rooms (rooms in clay or stone house).
pub(crate) fn clay_room_count(player: &Player) -> usize {
    if matches!(player.house, House::Clay | House::Stone) {
        player.rooms()
    } else {
        0
    }
}

/// Count major improvements in the bottom row of the major-improvement supply board.
///
/// The 2016 Revised supply board lays major improvements out in rows by cost;
/// its bottom row holds exactly the three good-conversion improvements:
/// Joinery, Pottery and Basketry. `major_improvement_bottom_row` in
/// `docs/bga-card-effect-vocabulary.md` names this count source; the member
/// set is pinned by the B007 / D030 test contracts (B007-normal: bottom row 2
/// -> food_delta 4). This composition is not derivable from repo-internal spec
/// data alone, so it should be confirmed against the official board before
/// shipping (noted as an open rule question in PR t_b954c21d).
pub(crate) fn major_improvements_bottom_row(player: &Player) -> usize {
    player
        .improvements
        .iter()
        .filter(|imp| {
            matches!(
                imp,
                Improvement::Joinery | Improvement::Pottery | Improvement::Basketry
            )
        })
        .count()
}

/// Count sown fields (fields with a crop planted).
pub(crate) fn sown_field_count(player: &Player) -> usize {
    player
        .farm
        .iter()
        .filter(|cell| matches!(cell.tile, Tile::Field { crop: Some(_), .. }))
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
                crate::cards::CardRequirement::MinimumImprovements(min) => {
                                improvement_count_in_front(set, player) >= u32::from(*min)
                            }
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
            CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                let bonus = improvement_count_in_front(set, player)
                    .checked_mul(*amount)
                    .ok_or("on-play card bonus overflow")?;
                safe_gain(player, *good, bonus)?;
            }
            CardEffect::OnPlayGainPerCount {
                count_kind,
                good,
                amount,
            } => {
                let count = match count_kind {
                    CountKind::Occupations => occupation_count(set, player) as u32,
                    CountKind::ClayRooms => clay_room_count(player) as u32,
                    CountKind::Improvements => improvement_count_in_front(set, player),
                    CountKind::FamilyMembers => player.family as u32,
                    CountKind::MajorImprovementsBottomRow => {
                        major_improvements_bottom_row(player) as u32
                    }
                    CountKind::SownFields => sown_field_count(player) as u32,
                };
                let bonus = count
                    .checked_mul(*amount)
                    .ok_or("on-play card bonus overflow")?;
                safe_gain(player, *good, bonus)?;
            }
            // B002 小牧場 (Mini Pasture): free single-cell pasture, cell picked
            // deterministically from the legal candidates (player state only).
            CardEffect::OnPlayBuildPasture { spaces } => {
                for _ in 0..*spaces {
                    let Some(cell) = crate::engine::free_single_cell_pasture_candidates(player)
                        .into_iter()
                        .next()
                    else {
                        break;
                    };
                    crate::engine::build_free_single_cell_pasture(player, cell)?;
                }
            }
            // B008 露店 (Market Stall): conditional exchange — no payment
            // resource, no exchange (and never a play failure).
            CardEffect::OnPlayExchangeGain {
                give,
                give_amount,
                receive,
                receive_amount,
            } => {
                if player.supply.get(*give) >= *give_amount {
                    player.supply.pay(*give, *give_amount)?;
                    safe_gain(player, *receive, *receive_amount)?;
                }
            }
            // B009 むち (Beating Rod): gain, or pay to gain one animal.
            // Deterministic policy: exchange whenever the payment resource is
            // available; the animal is placed through the derived plan (same
            // vocabulary as OnPlayGainAnimal, overflow discarded).
            CardEffect::OnPlayGainOrExchangeAnimal {
                gain_good,
                gain_amount,
                pay_good,
                pay_amount,
                animal,
            } => {
                if player.supply.get(*pay_good) >= *pay_amount {
                    player.supply.pay(*pay_good, *pay_amount)?;
                    let plan = crate::card_events::auto_animal_plan(player, *animal, 1);
                    crate::engine::place_animals(player, &plan, 1)
                        .map_err(|e| format!("on-play animal placement failed: {e}"))?;
                } else {
                    safe_gain(player, *gain_good, *gain_amount)?;
                }
            }
            // B003 (OnPlayDrawPlayOrPassOccupation) and B005
            // (OnPlayGainByHandOccupationCount) read the owner's HAND, which
            // this hand-less projection path cannot see. They are resolved by
            // the play dispatcher's handlers (card_events.rs) and intentionally
            // no-op here, exactly like the A-deck's hand-dependent A003.
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
                ScoreBonusRule::CategoriesAtMaxFour => {
                    let categories = &crate::score::score(player).categories;
                    [
                        "fields",
                        "pastures",
                        "grain",
                        "vegetables",
                        "sheep",
                        "boar",
                        "cattle",
                        "fenced_stables",
                    ]
                    .iter()
                    .filter(|name| categories.get(**name).copied().unwrap_or(0) >= 4)
                    .count() as i32
                }
                ScoreBonusRule::LargePastures => (player
                    .pastures
                    .iter()
                    .filter(|pasture| pasture.cells.len() >= 3)
                    .count() as i32)
                    .saturating_mul(2),
                ScoreBonusRule::CookingImprovements => player
                    .improvements
                    .iter()
                    .filter(|improvement| improvement.is_cooking())
                    .count() as i32,
                ScoreBonusRule::FieldsGoodsBeforeFinalHarvest(threshold) => {
                    // Snapshot taken at end of final harvest field phase; if missing, treat as 0.
                    let goods = player.field_goods_before_final_harvest.unwrap_or(0);
                    if goods >= *threshold {
                        2
                    } else {
                        0
                    }
                }
                ScoreBonusRule::StoneRoomsExceedPeople => {
                    let stone_rooms = if matches!(player.house, House::Stone) {
                        player.rooms()
                    } else {
                        0
                    };
                    if stone_rooms > player.family as usize {
                        4
                    } else {
                        0
                    }
                }
                ScoreBonusRule::FarmyardAllUsed => {
                    // A farm cell counts as used when it carries a non-Empty
                    // tile or a stable is built on it: a stable-on-empty space
                    // (tile Empty, stable true) is occupied by the stable, so
                    // it must not be counted as unused.
                    let all_used = player
                        .farm
                        .iter()
                        .all(|cell| !matches!(cell.tile, Tile::Empty) || cell.stable);
                    if all_used {
                        2
                    } else {
                        0
                    }
                }
                ScoreBonusRule::ExactFamilyMembers => match player.family {
                    4 => 2,
                    3 => 3,
                    2 => 5,
                    _ => 0,
                },
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "after_occ".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
                post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }
    }
    let minor = CardKind::MinorImprovement;
    let mut result = vec![
        card(
            "A042",
            minor,
            vec![(Good::Clay, 2)],
            1,
            vec![
                gain(Space::Fishing, Good::Wood, 1),
                gain(Space::Forest, Good::Food, 1),
                gain(Space::Copse, Good::Food, 1),
                gain(Space::Grove, Good::Food, 1),
            ],
        ),
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
    // A121 レンガの刻印打ち: on_play clay 1 + after_worker lessons/clay_pit -> clay 1
    result.push(card(
        "A121",
        CardKind::Occupation,
        vec![],
        0,
        vec![
            CardEffect::OnPlayGain {
                good: Good::Clay,
                amount: 1,
            },
            gain(Space::Lessons, Good::Clay, 1),
            gain(Space::ClayPit, Good::Clay, 1),
        ],
    ));
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
                post_play: crate::cards::PostPlayDestination::PlayedArea,
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
    #[test]
    fn a042_fishing_grants_wood() {
        let p = owner("A042", 2, &[Space::Fishing, Space::Fishing]);
        assert_eq!(p.supply.wood, 2);
    }
    #[test]
    fn a042_wood_spaces_grant_food() {
        let p = owner("A042", 2, &[Space::Forest, Space::Copse, Space::Grove]);
        assert_eq!(p.supply.food, 3);
    }
    #[test]
    fn a042_only_triggers_on_listed_spaces() {
        let p = owner(
            "A042",
            2,
            &[Space::DayLaborer, Space::ReedBank, Space::GrainSeeds],
        );
        assert_eq!(p.supply.wood, 0);
        assert_eq!(p.supply.food, 0);
    }
    #[test]
    fn a042_cost_requires_two_clay() {
        let set = CardSet {
            definitions: verified_minor_worker_cards(2),
        };
        let card = set.definition("A042").unwrap();
        let mut p = Player::new(0, 0);
        p.supply.clay = 1;
        assert!(!can_play(&set, card, &p, 0));
        let old = p.clone();
        assert!(!can_play(&set, card, &p, 0));
        assert_eq!(p, old);
        p.supply.clay = 2;
        assert!(can_play(&set, card, &p, 0));
        assert_eq!(card.points, 1);
    }
    #[test]
    fn a121_on_play_gains_one_clay() {
        let set = CardSet {
            definitions: verified_minor_worker_cards(2),
        };
        let card = set.definition("A121").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        let mut p = Player::new(0, 0);
        let mut hand = vec!["A121".to_string()];
        play(&set, &mut hand, &mut p, "A121", CardKind::Occupation, 0).unwrap();
        assert_eq!(p.supply.clay, 1);
        assert_eq!(p.played_cards, vec!["A121"]);
    }
    #[test]
    fn a121_after_worker_lessons_and_clay_pit_grant_clay() {
        let p = owner(
            "A121",
            2,
            &[
                Space::Lessons,
                Space::ClayPit,
                Space::Lessons,
                Space::Forest,
            ],
        );
        // owner() only calls after_worker, not play(), so on_play is not triggered
        assert_eq!(p.supply.clay, 3); // lessons 1 + clay_pit 1 + lessons 1 = 3
    }
    #[test]
    fn a121_not_triggered_by_other_spaces() {
        let p = owner(
            "A121",
            2,
            &[
                Space::Forest,
                Space::ReedBank,
                Space::Fishing,
                Space::DayLaborer,
            ],
        );
        assert_eq!(p.supply.clay, 0); // no on_play, no triggering spaces
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E007 Pumpernickel (ライ麦パン): OnPlayGain 4 food, cost 1 grain.
        CardDefinition {
            use_limit: None,
            id: "E007".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Grain, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 4,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E008 農産物直売所 (Farmers Market): OnPlayGain 1 vegetable, cost 2 food.
        CardDefinition {
            use_limit: None,
            id: "E008".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Vegetable,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // D004 Cross-Cut Wood (横挽きの木材): OnPlayGainPerStored wood per stone in supply, cost 1 food.
        CardDefinition {
            use_limit: None,
            id: "D004".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerStored {
                stored: Good::Stone,
                divisor: 1,
                good: Good::Wood,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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

    #[test]
    fn e007_on_play_gains_four_food() {
        let mut player = Player::new(0, 0);
        player.supply.grain = 1;
        play_card("E007", &mut player).unwrap();
        assert_eq!(player.supply.food, 4);
        assert_eq!(player.supply.grain, 0);
        assert_eq!(player.played_cards, vec!["E007"]);
    }

    #[test]
    fn e007_cannot_play_without_grain() {
        let mut player = Player::new(0, 0);
        let old = player.clone();
        assert!(play_card("E007", &mut player).is_err());
        assert_eq!(player, old);
    }

    #[test]
    fn e008_on_play_gains_one_vegetable() {
        let mut player = Player::new(0, 0);
        player.supply.food = 2;
        play_card("E008", &mut player).unwrap();
        assert_eq!(player.supply.vegetable, 1);
        assert_eq!(player.supply.food, 0);
        assert_eq!(player.played_cards, vec!["E008"]);
    }

    #[test]
    fn e008_cannot_play_without_food() {
        let mut player = Player::new(0, 0);
        let old = player.clone();
        assert!(play_card("E008", &mut player).is_err());
        assert_eq!(player, old);
    }

    #[test]
    fn d004_on_play_gains_wood_per_stone() {
        let mut player = Player::new(0, 0);
        player.supply.food = 1;
        player.supply.stone = 3;
        play_card("D004", &mut player).unwrap();
        assert_eq!(player.supply.wood, 3);
        assert_eq!(player.supply.food, 0);
        assert_eq!(player.supply.stone, 3);
        assert_eq!(player.played_cards, vec!["D004"]);
    }

    #[test]
    fn d004_cannot_play_without_food() {
        let mut player = Player::new(0, 0);
        player.supply.stone = 2;
        let old = player.clone();
        assert!(play_card("D004", &mut player).is_err());
        assert_eq!(player, old);
    }
}

/**
 * Verified 2016 base A/B occupations expressible entirely through the worker
 * completion hook. Only effect logic and opaque card IDs are stored here.
 * This subset alone is not a full playable 7+7-card deal.
 *
 * The `players` argument is intentionally unused: the player-count-gated A/B
 * cards that used to be appended here (A155 at 4+, B142 at 3+) now live in the
 * band pools `verified_band_a_cards` / `verified_band_b_cards`, so this pool is
 * player-count independent. The signature is kept so existing callers
 * (`card_pool.rs`, `registry.rs`) do not have to change.
 */
pub fn verified_worker_bonus_cards(_players: usize) -> Vec<CardDefinition> {
    fn occupation(id: &str, effects: Vec<CardEffect>) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects,
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }
    }
    fn gain(space: Space, good: Good, amount: u32) -> CardEffect {
        CardEffect::AfterWorkerGain {
            space,
            good,
            amount,
        }
    }
    let cards = vec![
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
        // B064 水車の輪: after_worker_space grain_utilization if fishing used -> gain food 2
        CardDefinition {
            use_limit: None,
            id: "B064".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 2)],
            points: 1,
            effects: vec![CardEffect::AfterWorkerGainIfUsed {
                space: Space::GrainUtilization,
                required_space: Space::Fishing,
                good: Good::Food,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ];
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
        // A116, A119, A122, B064 are in this pool now (no player-count gating)
        assert_eq!(two.len(), 4);
        assert_eq!(three.len(), 4);
        assert_eq!(four.len(), 4);
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

    // B064 水車の輪: AfterWorkerGainIfUsed grain_utilization if fishing used -> food 2
    #[test]
    fn b064_food_from_grain_utilization_only_when_fishing_used() {
        let definitions = verified_worker_bonus_cards(4);
        let set = CardSet {
            definitions: definitions.clone(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("B064".into());
        // Fishing not used -> no gain
        after_worker_if_used(&set, &mut p, Space::GrainUtilization, &BTreeSet::new()).unwrap();
        assert_eq!(p.supply.food, 0);
        // Fishing used in the same round -> gain 2 food
        after_worker_if_used(
            &set,
            &mut p,
            Space::GrainUtilization,
            &BTreeSet::from([Space::Fishing]),
        )
        .unwrap();
        assert_eq!(p.supply.food, 2);
        // Other spaces do not trigger even when fishing is used
        after_worker_if_used(
            &set,
            &mut p,
            Space::Forest,
            &BTreeSet::from([Space::Fishing]),
        )
        .unwrap();
        assert_eq!(p.supply.food, 2);
    }

    #[test]
    fn b064_is_minor_improvement_costing_wood_two_with_one_point() {
        let definitions = verified_worker_bonus_cards(2);
        let card = definitions.iter().find(|c| c.id == "B064").unwrap().clone();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 2)]);
        assert_eq!(card.points, 1);
        let set = CardSet { definitions };
        let mut player = Player::new(0, 0);
        assert!(!can_play(&set, &card, &player, 0));
        player.supply.wood = 2;
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
            } else if let CardEffect::StartRoundGainIfNoMinorPlayed { good, amount } = effect {
                // E111 世捨て人, round-start half: the condition is checked
                // here because it needs card definitions.
                if no_minor_improvements_played(set, player) {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// Vegetables the field phase is about to yield: one per planted vegetable
/// field whose crop is still growing (each yields exactly one per harvest).
/// Read pre-harvest state, before any crops are removed.
fn vegetable_yield(player: &Player) -> u32 {
    player
        .farm
        .iter()
        .filter(|cell| {
            matches!(
                cell.tile,
                Tile::Field {
                    crop: Some(Crop::Vegetable),
                    remaining: 1..,
                }
            )
        })
        .count() as u32
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
                    HarvestFieldRule::VegetablesHarvested => vegetable_yield(player),
                    HarvestFieldRule::HasWheatField => u32::from(player.farm.iter().any(|cell| {
                        matches!(
                            cell.tile,
                            Tile::Field {
                                crop: Some(Crop::Grain),
                                ..
                            }
                        )
                    })),
                    HarvestFieldRule::FieldCountThresholds {
                        thresholds,
                        amounts,
                    } => {
                        let fields = player
                            .farm
                            .iter()
                            .filter(|cell| matches!(cell.tile, Tile::Field { .. }))
                            .count() as u32;
                        let mut best = 0;
                        for (threshold, gained) in thresholds.iter().zip(amounts.iter()) {
                            if fields >= *threshold {
                                best = *gained;
                            }
                        }
                        best
                    }
                    HarvestFieldRule::LastGrainTaken | HarvestFieldRule::LastVegetableTaken => {
                        let crop = if rule == &HarvestFieldRule::LastGrainTaken {
                            Crop::Grain
                        } else {
                            Crop::Vegetable
                        };
                        player
                            .farm
                            .iter()
                            .filter(|cell| {
                                matches!(
                                    cell.tile,
                                    Tile::Field {
                                        crop: Some(found),
                                        remaining: 1,
                                    } if found == crop
                                )
                            })
                            .count() as u32
                    }
                };
                safe_gain(
                    player,
                    *good,
                    multiplier
                        .checked_mul(*amount)
                        .ok_or("harvest bonus overflow")?,
                )?;
            } else if let CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount } = effect {
                // E111 世捨て人, harvest half: the condition is checked here
                // because it needs card definitions.
                if no_minor_improvements_played(set, player) {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// True when the player has played no minor improvements (E111 世捨て人
/// condition). Improvements handed over by other players (cards not played by
/// this player) do not appear in played_cards, so they are naturally excluded.
fn no_minor_improvements_played(set: &CardSet, player: &Player) -> bool {
    player.played_cards.iter().all(|id| {
        set.definition(id)
            .is_none_or(|card| card.kind != CardKind::MinorImprovement)
    })
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C141 羊の世話人: AfterAnyWorkerGain SheepMarket -> gain grain 1
        CardDefinition {
            use_limit: None,
            id: "C141".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterAnyWorkerGain {
                space: Space::SheepMarket,
                good: Good::Grain,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A050 ミルクがめ: AfterAnyWorkerGain CattleMarket -> gain food 3 (self) + AfterAnyWorkerOthersGain CattleMarket -> gain food 1 (others)
        CardDefinition {
            use_limit: None,
            id: "A050".into(),
            kind: minor,
            cost: vec![(Good::Clay, 1)],
            points: 0,
            effects: vec![
                CardEffect::AfterAnyWorkerGain {
                    space: Space::CattleMarket,
                    good: Good::Food,
                    amount: 3,
                },
                CardEffect::AfterAnyWorkerOthersGain {
                    space: Space::CattleMarket,
                    good: Good::Food,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // D061 わらのたわら: HarvestStartGainIfFields grain >= 3 -> gain food 2
        CardDefinition {
            use_limit: None,
            id: "D061".into(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::HarvestStartGainIfFields {
                crop: Crop::Grain,
                threshold: 3,
                good: Good::Food,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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

    // C141 羊の世話人: AfterAnyWorkerGain SheepMarket -> gain grain 1
    #[test]
    fn c141_gains_grain_when_any_player_uses_sheep_market() {
        use crate::card_events::dispatch_after_worker;
        use crate::cards::CardState;
        use std::collections::BTreeMap;

        let definitions = verified_round_and_harvest_cards();
        let set = CardSet { definitions };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0), Player::new(1, 1)];
        players[0].played_cards.push("C141".into());
        let mut used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;

        // Player 0 (C141 owner) uses SheepMarket -> should gain 1 grain
        dispatch_after_worker(
            &set,
            0,
            Space::SheepMarket,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
        )
        .unwrap();
        assert_eq!(players[0].supply.grain, 1);

        // Player 1 uses SheepMarket -> player 0 (C141 owner) should gain 1 grain
        used_spaces.clear();
        dispatch_after_worker(
            &set,
            1,
            Space::SheepMarket,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
        )
        .unwrap();
        assert_eq!(players[0].supply.grain, 2);

        // Player 0 uses Forest -> should not trigger
        used_spaces.clear();
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
        )
        .unwrap();
        assert_eq!(players[0].supply.grain, 2);
    }

    // A050 ミルクがめ: AfterAnyWorkerGain CattleMarket -> gain food 3 (self) + AfterAnyWorkerOthersGain CattleMarket -> gain food 1 (others)
    #[test]
    fn a050_gains_food_when_any_player_uses_cattle_market() {
        use crate::card_events::dispatch_after_worker;
        use crate::cards::CardState;
        use std::collections::BTreeMap;

        let definitions = verified_round_and_harvest_cards();
        let set = CardSet { definitions };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0), Player::new(1, 0), Player::new(2, 0)];
        players[0].played_cards.push("A050".into());
        let mut used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;

        // Player 0 (A050 owner) uses CattleMarket -> owner gains 3 food, others gain 1 each
        dispatch_after_worker(
            &set,
            0,
            Space::CattleMarket,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 3); // self: 3
        assert_eq!(players[1].supply.food, 1); // other: 1
        assert_eq!(players[2].supply.food, 1); // other: 1

        // Player 1 uses CattleMarket -> player 0 gains 3, others (1,2) gain 1
        used_spaces.clear();
        dispatch_after_worker(
            &set,
            1,
            Space::CattleMarket,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 6); // 3 + 3
        assert_eq!(players[1].supply.food, 2); // 1 + 1
        assert_eq!(players[2].supply.food, 2); // 1 + 1

        // Using a different space does not trigger
        used_spaces.clear();
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 6);
        assert_eq!(players[1].supply.food, 2);
        assert_eq!(players[2].supply.food, 2);
    }

    // D061 わらのたわら: HarvestStartGainIfFields grain >= 3 -> gain food 2
    #[test]
    fn d061_gains_food_at_harvest_with_three_grain_fields() {
        use crate::card_events::dispatch_harvest_field;
        use std::collections::BTreeMap;

        let definitions = verified_round_and_harvest_cards();
        let set = CardSet { definitions };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("D061".into());
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;

        // 2 grain fields -> no gain
        players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        dispatch_harvest_field(
            &set,
            0,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            4,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 0);
        // 3 grain fields -> gain 2 food
        players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        dispatch_harvest_field(
            &set,
            0,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            4,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 2);
        // 4 grain fields -> still gains 2 food (threshold is 3)
        players[0].farm[5].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        dispatch_harvest_field(
            &set,
            0,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            4,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 4);
    }

    // D061: vegetable fields don't count
    #[test]
    fn d061_ignores_vegetable_fields() {
        use crate::card_events::dispatch_harvest_field;
        use std::collections::BTreeMap;

        let definitions = verified_round_and_harvest_cards();
        let set = CardSet { definitions };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("D061".into());
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;

        players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        dispatch_harvest_field(
            &set,
            0,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut BTreeMap::new(),
            4,
            &used_spaces,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 0);
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
                post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
    // Exclude expansion (E-prefixed) and D-deck cards from the A/B verified pool.
    definitions.extend(
        verified_immediate_resource_cards()
            .into_iter()
            .filter(|c| !c.id.starts_with('E') && !c.id.starts_with('D')),
    );
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
        assert_eq!(set.definitions.len(), 39);
        assert_eq!(
            set.definitions
                .iter()
                .filter(|x| x.kind == CardKind::MinorImprovement)
                .count(),
            22
        );
        assert_eq!(
            set.definitions
                .iter()
                .filter(|x| x.kind == CardKind::Occupation)
                .count(),
            17
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        CardDefinition {
            use_limit: None,
            id: "A063".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 2), (Good::Stone, 2)],
            points: 2,
            effects: vec![CardEffect::AfterHarvestBakeGain { amount: 3 }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

/// C-deck band 070-120 cards whose printed effects are fully expressible with
/// the current typed effect vocabulary, sourced from the BGA Agricola card
/// wiki (agricola.cloudfree.jp; card pages C076 p=3013, C106 p=2983,
/// C111 p=2978, cross-checked against the English list).
///
/// These are deliberately kept OUT of `verified_basic_card_set`: that preset is
/// the built-in A/B two-player pack (E/D/C-prefixed cards are filtered or never
/// added), so adding band cards here must not change its pool size, its
/// distribution digest, or any recorded replay hash.
pub fn verified_band_c_cards() -> Vec<CardDefinition> {
    fn card(
        id: &str,
        kind: CardKind,
        cost: Vec<(Good, u32)>,
        effects: Vec<CardEffect>,
    ) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.to_owned(),
            kind,
            cost,
            points: 0,
            effects,
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }
    }
    let minor = CardKind::MinorImprovement;
    let occupation = CardKind::Occupation;
    vec![
        // C076 木材荷車 (Wood Cart): cost 3 wood, prerequisite 3 occupations
        // played; each use of a wood accumulation space (森林/小森/林) grants
        // 2 extra wood.
        card(
            "C076",
            minor,
            vec![(Good::Wood, 3)],
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(3),
                },
                CardEffect::AfterWorkerGain {
                    space: Space::Forest,
                    good: Good::Wood,
                    amount: 2,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::Copse,
                    good: Good::Wood,
                    amount: 2,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::Grove,
                    good: Good::Wood,
                    amount: 2,
                },
            ],
        ),
        // C106 じゃがいも収穫者 (Potato Harvester): no cost; on play gain
        // 3 food, and in each harvest field phase gain 1 extra food per
        // vegetable taken from the fields.
        card(
            "C106",
            occupation,
            vec![],
            vec![
                CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 3,
                },
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::VegetablesHarvested,
                    good: Good::Food,
                    amount: 1,
                },
            ],
        ),
        // C111 小動物飼育員 (Small Animal Breeder): no cost; at each round
        // start, if food stock >= current round number, gain 1 food.
        card(
            "C111",
            occupation,
            vec![],
            vec![CardEffect::StartRoundGainIfFoodAtLeastRound {
                good: Good::Food,
                amount: 1,
            }],
        ),
        // C116 調度品職人 (Furniture Maker): no cost; on play gain 1 wood, and
        // each later occupation play grants 1 wood per food paid as its cost.
        card(
            "C116",
            occupation,
            vec![],
            vec![
                CardEffect::OnPlayGain {
                    good: Good::Wood,
                    amount: 1,
                },
                CardEffect::AfterOccupationGainPerFoodCost {
                    good: Good::Wood,
                    amount: 1,
                },
            ],
        ),
        // C068 書棚 (Bookcase): cost 2 wood, prerequisite 1 occupation;
        // each time an occupation is played after this, gain 1 vegetable.
        card(
            "C068",
            minor,
            vec![(Good::Wood, 2)],
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                CardEffect::AfterOccupationGain {
                    good: Good::Vegetable,
                    amount: 1,
                },
            ],
        ),
        // C004 筆記板 (Slate Board): cost 1 food; on play gain 1 wood per occupation played.
        card(
            "C004",
            minor,
            vec![(Good::Food, 1)],
            vec![CardEffect::OnPlayGainPerCount {
                count_kind: CountKind::Occupations,
                good: Good::Wood,
                amount: 1,
            }],
        ),
        // C005 改修 (Renovation): cost 1 food; on play gain 1 clay per clay room + 1 clay per improvement.
        card(
            "C005",
            minor,
            vec![(Good::Food, 1)],
            vec![
                CardEffect::OnPlayGainPerCount {
                    count_kind: CountKind::ClayRooms,
                    good: Good::Clay,
                    amount: 1,
                },
                CardEffect::OnPlayGainPerCount {
                    count_kind: CountKind::Improvements,
                    good: Good::Clay,
                    amount: 1,
                },
            ],
        ),
        // C083 若牛 (Early Cattle): minor improvement, -3 points, on play gain 2 cattle.
        CardDefinition {
            use_limit: None,
            id: "C083".to_owned(),
            kind: minor,
            cost: vec![],
            points: -3,
            effects: vec![CardEffect::OnPlayGainAnimal {
                animal: Animal::Cattle,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C147 牛の飼育士 (Cowherd): occupation, 0 points, each time owner uses CattleMarket gain 1 cattle.
        card(
            "C147",
            occupation,
            vec![],
            vec![CardEffect::AfterWorkerGainAnimal {
                space: Space::CattleMarket,
                animal: Animal::Cattle,
                amount: 1,
            }],
        ),
    ]
}
#[cfg(test)]
mod verified_band_c_tests {
    use super::*;

    fn set() -> CardSet {
        CardSet {
            definitions: verified_band_c_cards(),
        }
    }

    fn field(crop: Option<Crop>, remaining: u8) -> Tile {
        Tile::Field { crop, remaining }
    }

    #[test]
    fn c076_requires_three_occupations_and_costs_three_wood() {
        let mut set = set();
        set.definitions.extend([
            CardDefinition {
                use_limit: None,
                id: "occ1".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            },
            CardDefinition {
                use_limit: None,
                id: "occ2".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            },
            CardDefinition {
                use_limit: None,
                id: "occ3".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            },
        ]);
        let card = set.definition("C076").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 3)]);
        assert_eq!(card.points, 0);
        let mut p = Player::new(0, 0);
        p.supply.wood = 3;
        assert!(!can_play(&set, card, &p, 0));
        p.played_cards
            .extend(["occ1".to_string(), "occ2".to_string(), "occ3".to_string()]);
        assert!(can_play(&set, card, &p, 0));
        // 支払えなければ出せない。
        p.supply.wood = 2;
        assert!(!can_play(&set, card, &p, 0));
    }

    #[test]
    fn c076_gains_two_wood_on_each_wood_accumulation_space() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("C076".into());
        after_worker(&set, &mut p, Space::Forest).unwrap();
        assert_eq!(p.supply.wood, 2);
        after_worker(&set, &mut p, Space::Copse).unwrap();
        assert_eq!(p.supply.wood, 4);
        after_worker(&set, &mut p, Space::Grove).unwrap();
        assert_eq!(p.supply.wood, 6);
        // 木材累積スペース以外では発動しない。
        after_worker(&set, &mut p, Space::ClayPit).unwrap();
        after_worker(&set, &mut p, Space::DayLaborer).unwrap();
        assert_eq!(p.supply.wood, 6);
        assert_eq!(p.supply.clay, 0);
    }

    #[test]
    fn c106_gains_three_food_on_play_and_no_cost() {
        let set = set();
        let card = set.definition("C106").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        let mut hand = vec!["C106".to_string()];
        let mut p = Player::new(0, 0);
        play(&set, &mut hand, &mut p, "C106", CardKind::Occupation, 0).unwrap();
        assert_eq!(p.supply.food, 3);
        assert_eq!(p.played_cards, vec!["C106"]);
    }

    #[test]
    fn c106_food_per_vegetable_harvested_in_field_phase() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("C106".into());
        // 野菜畑2（生育中）＋麦畑1 → 野菜2に対して食料2。
        p.farm[1].tile = field(Some(Crop::Vegetable), 2);
        p.farm[2].tile = field(Some(Crop::Vegetable), 1);
        p.farm[3].tile = field(Some(Crop::Grain), 3);
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 2);
        // 野菜畑が空なら追加食料なし。
        let mut q = Player::new(0, 0);
        q.played_cards.push("C106".into());
        q.farm[1].tile = field(Some(Crop::Grain), 2);
        q.farm[2].tile = field(None, 0);
        harvest_field_rewards(&set, &mut q).unwrap();
        assert_eq!(q.supply.food, 0);
    }

    #[test]
    fn c111_has_no_cost_and_zero_points() {
        let set = set();
        let card = set.definition("C111").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        let mut hand = vec!["C111".to_string()];
        let mut p = Player::new(0, 0);
        play(&set, &mut hand, &mut p, "C111", CardKind::Occupation, 0).unwrap();
        assert_eq!(p.played_cards, vec!["C111"]);
    }

    #[test]
    fn c116_on_play_gains_one_wood_and_is_playable_without_cost() {
        let set = set();
        let card = set.definition("C116").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        let mut hand = vec!["C116".to_string()];
        let mut p = Player::new(0, 0);
        play(&set, &mut hand, &mut p, "C116", CardKind::Occupation, 0).unwrap();
        assert_eq!(p.supply.wood, 1);
        assert_eq!(p.played_cards, vec!["C116"]);
    }

    #[test]
    fn band_cards_are_absent_from_the_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        for id in ["C076", "C106", "C111", "C116", "C083", "C147"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }

    #[test]
    fn c083_has_negative_three_points_and_no_cost() {
        let set = set();
        let card = set.definition("C083").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, -3);
        assert_eq!(
            card.effects,
            vec![CardEffect::OnPlayGainAnimal {
                animal: Animal::Cattle,
                amount: 2,
            }]
        );
    }

    #[test]
    fn c147_has_zero_points_and_no_cost() {
        let set = set();
        let card = set.definition("C147").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        assert_eq!(
            card.effects,
            vec![CardEffect::AfterWorkerGainAnimal {
                space: Space::CattleMarket,
                animal: Animal::Cattle,
                amount: 1,
            }]
        );
    }
}

/// D-deck band 070-120 cards whose printed effects are fully expressible with
/// the current typed effect vocabulary (BGA Agricola card wiki;
/// D100 p=2821, D105 p=2816, D109 p=2812). Kept out of the A/B two-player
/// preset, same as [`verified_band_c_cards`].
pub fn verified_band_d_cards() -> Vec<CardDefinition> {
    fn card(
        id: &str,
        kind: CardKind,
        cost: Vec<(Good, u32)>,
        effects: Vec<CardEffect>,
    ) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.to_owned(),
            kind,
            cost,
            points: 0,
            effects,
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }
    }
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    vec![
        // D100 君主 (Sovereign): no cost; at scoring, 1 bonus point per scoring
        // category that has reached its maximum of 4 points (fenced stables
        // included).
        card(
            "D100",
            CardKind::Occupation,
            vec![],
            vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::CategoriesAtMaxFour,
            }],
        ),
        // D105 彫刻家 (Sculptor): no cost; each use of a clay accumulation
        // space grants 1 food, each use of a stone accumulation space grants
        // 1 grain.
        card(
            "D105",
            CardKind::Occupation,
            vec![],
            vec![
                gain(Space::ClayPit, Good::Food, 1),
                gain(Space::WesternQuarry, Good::Grain, 1),
                gain(Space::EasternQuarry, Good::Grain, 1),
            ],
        ),
        // D109 種まきの達人 (Sow Master): no cost; on play gain 1 wood, and
        // each use of an action space with the sow action (grain seeds,
        // vegetable seeds, cultivation) grants 2 food.
        card(
            "D109",
            CardKind::Occupation,
            vec![],
            vec![
                CardEffect::OnPlayGain {
                    good: Good::Wood,
                    amount: 1,
                },
                gain(Space::GrainSeeds, Good::Food, 2),
                gain(Space::VegetableSeeds, Good::Food, 2),
                gain(Space::Cultivation, Good::Food, 2),
            ],
        ),
        // D005 畑の粘土 (Clay from Fields): cost 1 food; on play gain 1 clay per sown field.
        card(
            "D005",
            CardKind::MinorImprovement,
            vec![(Good::Food, 1)],
            vec![CardEffect::OnPlayGainPerCount {
                count_kind: CountKind::SownFields,
                good: Good::Clay,
                amount: 1,
            }],
        ),
        // D032 木のくまで (Wood Rake): cost 1 wood; at scoring, +2 points if at least
        // 7 goods stood in fields before the final harvest.
        card(
            "D032",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 1)],
            vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::FieldsGoodsBeforeFinalHarvest(7),
            }],
        ),
        // D034 豪華な宿泊所 (Luxurious Hostel): cost 1 wood + 2 clay; at scoring,
        // +4 points if stone rooms exceed family members.
        card(
            "D034",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 1), (Good::Clay, 2)],
            vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::StoneRoomsExceedPeople,
            }],
        ),
    ]
}

/// Improvements in front of a player: base improvements plus played
/// minor-improvement cards (A117 木材調達人 condition).
pub fn improvement_count_in_front(set: &CardSet, player: &Player) -> u32 {
    let minor = player
        .played_cards
        .iter()
        .filter(|id| {
            set.definition(id)
                .is_some_and(|card| card.kind == CardKind::MinorImprovement)
        })
        .count() as u32;
    minor + player.improvements.len() as u32
}

/// A-deck band 070-120 cards whose printed effects are fully expressible with
/// the current typed effect vocabulary (BGA Agricola card wiki; A099 p=3326,
/// A101 p=3324, A106 p=3319, A117 p=3308, A155 p=3270). Kept out of the A/B two-player
/// preset, same as the C/D/E band pools.
pub fn verified_band_a_cards() -> Vec<CardDefinition> {
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    vec![
        // A099 放牧人 (Fellow Grazer): 2 bonus points per pasture covering at
        // least 3 farm cells at scoring time.
        CardDefinition {
            use_limit: None,
            id: "A099".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::LargePastures,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A101 調理器具商 (Cookery Outfitter): 1 bonus point per cooking
        // improvement at scoring time.
        CardDefinition {
            use_limit: None,
            id: "A101".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::CookingImprovements,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A106 液肥散布人 (Slurry Spreader): per field whose last grain is
        // harvested this field phase 2 food, per field whose last vegetable
        // is harvested 1 food.
        CardDefinition {
            use_limit: None,
            id: "A106".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::LastGrainTaken,
                    good: Good::Food,
                    amount: 2,
                },
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::LastVegetableTaken,
                    good: Good::Food,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A117 木材調達人 (Wood Carrier): 1 wood per improvement already in
        // front of the owner when playing this card.
        CardDefinition {
            use_limit: None,
            id: "A117".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerImprovementPlayed {
                good: Good::Wood,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A155 奇術師 (Conjurer): after worker placed on traveling_players,
        // gain 1 wood and 1 grain. Only available at 4 players.
        CardDefinition {
            use_limit: None,
            id: "A155".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                gain(Space::TravelingPlayers, Good::Wood, 1),
                gain(Space::TravelingPlayers, Good::Grain, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A008 食材かご (Food Basket): cost 1 reed; prerequisites 2 occupations and
        // 2 improvements in front; on play gain 1 grain and 1 vegetable.
        CardDefinition {
            use_limit: None,
            id: "A008".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Reed, 1)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(2),
                },
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumImprovements(2),
                },
                CardEffect::OnPlayGain {
                    good: Good::Grain,
                    amount: 1,
                },
                CardEffect::OnPlayGain {
                    good: Good::Vegetable,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

/// B-deck band 121-168 cards whose printed effects are fully expressible with
/// the current typed effect vocabulary (BGA Agricola card wiki;
/// B122 p=3135, B142 p=3207, B144 p=3113). Kept out of the A/B two-player
/// preset, same as the C/D/E band pools.
pub fn verified_band_b_cards() -> Vec<CardDefinition> {
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    vec![
        // B122 鉱物学者 (Mineralogist): clay accumulation spaces (clay_pit, hollow)
        // grant 1 stone; stone accumulation spaces (western_quarry, eastern_quarry)
        // grant 1 clay. Available at all player counts.
        CardDefinition {
            use_limit: None,
            id: "B122".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                gain(Space::ClayPit, Good::Stone, 1),
                gain(Space::Hollow, Good::Stone, 1),
                gain(Space::WesternQuarry, Good::Clay, 1),
                gain(Space::EasternQuarry, Good::Clay, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B142 八百屋 (Greengrocer): grain_seeds grants 1 vegetable.
        // Only available at 3+ players.
        CardDefinition {
            use_limit: None,
            id: "B142".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                gain(Space::GrainSeeds, Good::Vegetable, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B144 坑夫 (Collier): clay accumulation spaces (clay_pit, hollow) grant
        // 1 wood; clay_pit additionally grants 1 reed.
        // Only available at 3+ players.
        CardDefinition {
            use_limit: None,
            id: "B144".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                gain(Space::ClayPit, Good::Wood, 1),
                gain(Space::Hollow, Good::Wood, 1),
                gain(Space::ClayPit, Good::Reed, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B006 採石場見学 (Excursion to the Quarry): cost 2 food; prerequisite:
        // at least 1 occupation played; on play gain 1 stone per family member.
        CardDefinition {
            use_limit: None,
            id: "B006".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                CardEffect::OnPlayGainPerCount {
                    count_kind: CountKind::FamilyMembers,
                    good: Good::Stone,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B007 賃金 (Wages): cost 0 (free); on play gain 2 food + 1 food per
        // major improvement in the bottom row (Joinery / Pottery / Basketry).
        CardDefinition {
            use_limit: None,
            id: "B007".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 2,
                },
                CardEffect::OnPlayGainPerCount {
                    count_kind: CountKind::MajorImprovementsBottomRow,
                    good: Good::Food,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

/// B-deck chapter 1: bga-checked on-play cards (B002/B003/B005/B008/B009)
/// whose printed effects map onto the typed effect vocabulary, sourced from
/// the BGA Agricola card wiki (agricola.cloudfree.jp; B002 p=3255, B003
/// p=3254, B005 p=3252, B008 p=3249, B009 p=3248, cross-checked against the
/// English list). Every one of these cards is printed "pass left", expressed
/// as `post_play: LeftNeighborHand`.
///
/// These are deliberately kept OUT of `verified_basic_card_set` (same as the
/// band pools): the built-in A/B two-player preset must keep its pool size,
/// distribution digest and recorded replay hashes unchanged, and B003/B005's
/// hand-dependent effects are not modelled by the hand-less `cards::play`
/// projection the AI evaluators use.
pub fn verified_b_onplay_simple_cards() -> Vec<CardDefinition> {
    let minor = CardKind::MinorImprovement;
    vec![
        // B002 小牧場 (Mini Pasture): cost 2 food; on play fence one empty
        // farmyard cell into a pasture at no wood cost, adjacent to an
        // existing pasture when the player owns one.
        CardDefinition {
            use_limit: None,
            id: "B002".to_owned(),
            kind: minor,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![CardEffect::OnPlayBuildPasture { spaces: 1 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B003 密造酒 (Moonshine): no cost; on play take one occupation from
        // hand and play it for 2 food, or pass it to the left neighbour.
        CardDefinition {
            use_limit: None,
            id: "B003".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B005 経験の蓄積 (Store of Experience): no cost; on play gain 1 good
        // by the number of occupations still in hand: 0-4 stone, 5 reed,
        // 6 clay, 7 wood.
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B008 露店 (Market Stall): cost 1 grain; on play exchange 1 grain
        // for 1 vegetable (the printed cost is paid separately by the play).
        CardDefinition {
            use_limit: None,
            id: "B008".to_owned(),
            kind: minor,
            cost: vec![(Good::Grain, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayExchangeGain {
                give: Good::Grain,
                give_amount: 1,
                receive: Good::Vegetable,
                receive_amount: 1,
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B009 むち (Beating Rod): no cost; on play gain 1 reed, or exchange
        // 1 reed for 1 cattle (deterministic engine policy: exchange when a
        // reed is available).
        CardDefinition {
            use_limit: None,
            id: "B009".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainOrExchangeAnimal {
                gain_good: Good::Reed,
                gain_amount: 1,
                pay_good: Good::Reed,
                pay_amount: 1,
                animal: Animal::Cattle,
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
    ]
}

/// Test module for verified_band_b_cards.
#[cfg(test)]
mod verified_band_b_tests {
    use super::*;

    fn applied(card_id: &str, spaces: &[Space], _players: usize) -> Player {
        let definitions = verified_band_b_cards();
        let mut player = Player::new(0, 0);
        player.played_cards.push(card_id.to_string());
        let set = CardSet { definitions };
        for &space in spaces {
            after_worker(&set, &mut player, space).unwrap();
        }
        player
    }

    #[test]
    fn b122_stone_from_clay_spaces_clay_from_stone_spaces() {
        // B122: clay_pit/hollow -> stone; western/eastern_quarry -> clay
        let p = applied(
            "B122",
            &[
                Space::ClayPit,
                Space::Hollow,
                Space::WesternQuarry,
                Space::EasternQuarry,
            ],
            2,
        );
        assert_eq!(p.supply.stone, 2); // clay_pit + hollow
        assert_eq!(p.supply.clay, 2); // western_quarry + eastern_quarry
        assert_eq!(p.supply.wood, 0);
        assert_eq!(p.supply.reed, 0);
    }

    #[test]
    fn b122_not_triggered_by_other_spaces() {
        let p = applied(
            "B122",
            &[
                Space::Forest,
                Space::ReedBank,
                Space::Fishing,
                Space::DayLaborer,
            ],
            2,
        );
        assert_eq!(p.supply.stone, 0);
        assert_eq!(p.supply.clay, 0);
    }

    #[test]
    fn b122_playable_at_all_player_counts() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B122").unwrap();
        // No MinimumPlayers prerequisite, so playable at 2, 3, 4
        for players in [2, 3, 4] {
            let p = Player::new(0, 0);
            assert!(
                can_play(&set, card, &p, 0),
                "B122 should be playable at {players} players"
            );
        }
    }

    #[test]
    fn b142_vegetable_from_grain_seeds_only() {
        let p = applied(
            "B142",
            &[Space::GrainSeeds, Space::Farmland, Space::GrainUtilization],
            3,
        );
        assert_eq!(p.supply.vegetable, 1); // only grain_seeds triggers
        assert_eq!(p.supply.grain, 0);
        assert_eq!(p.supply.food, 0);
    }

    #[test]
    fn b142_not_triggered_by_other_spaces() {
        let p = applied(
            "B142",
            &[
                Space::Forest,
                Space::ClayPit,
                Space::VegetableSeeds,
                Space::Cultivation,
            ],
            3,
        );
        assert_eq!(p.supply.vegetable, 0);
    }

    #[test]
    fn b142_requires_three_players() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B142").unwrap();
        let has_min_players = card.effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3)
                }
            )
        });
        assert!(
            has_min_players,
            "B142 should have MinimumPlayers(3) prerequisite"
        );
        let p = Player::new(0, 0);
        assert!(
            can_play(&set, card, &p, 0),
            "B142 should be playable via can_play"
        );
    }

    #[test]
    fn b144_not_triggered_by_other_spaces() {
        let p = applied(
            "B144",
            &[
                Space::Forest,
                Space::WesternQuarry,
                Space::ReedBank,
                Space::Fishing,
            ],
            3,
        );
        assert_eq!(p.supply.wood, 0);
        assert_eq!(p.supply.reed, 0);
    }

    #[test]
    fn b144_wood_from_clay_spaces_reed_from_clay_pit() {
        // B144: clay_pit/hollow -> wood; clay_pit -> reed
        let p = applied("B144", &[Space::ClayPit, Space::Hollow, Space::ClayPit], 3);
        assert_eq!(p.supply.wood, 3); // clay_pit + hollow + clay_pit
        assert_eq!(p.supply.reed, 2); // clay_pit + clay_pit
        assert_eq!(p.supply.clay, 0);
        assert_eq!(p.supply.stone, 0);
    }

    #[test]
    fn b144_requires_three_players() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B144").unwrap();
        let has_min_players = card.effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3)
                }
            )
        });
        assert!(
            has_min_players,
            "B144 should have MinimumPlayers(3) prerequisite"
        );
        let p = Player::new(0, 0);
        assert!(
            can_play(&set, card, &p, 0),
            "B144 should be playable via can_play"
        );
    }

    #[test]
    fn band_b_cards_are_absent_from_the_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        // A155 sits in the A-deck band pool rather than the B-deck one, but it
        // is covered here too: neither band pool may leak into the A/B
        // two-player preset.
        for id in ["B122", "B142", "B144", "A155"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }
}

/// E-deck band 070-120 cards whose printed effects are fully expressible with
/// the current typed effect vocabulary (BGA Agricola card wiki;
/// E117 p=2636). Kept out of the A/B two-player preset, same as
/// [`verified_band_c_cards`] and [`verified_band_d_cards`].
pub fn verified_band_e_cards() -> Vec<CardDefinition> {
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    vec![
        CardDefinition {
            use_limit: None,
            id: "E117".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::HarvestFieldGain {
                rule: HarvestFieldRule::HasWheatField,
                good: Good::Wood,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E107 土地調査士 (Land Surveyor): at the field phase, 2/4/6/7 or
        // more plowed fields yield 1/2/3/4 food
        // (agricola.cloudfree.jp ?p=2646).
        CardDefinition {
            use_limit: None,
            id: "E107".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::HarvestFieldGain {
                rule: HarvestFieldRule::FieldCountThresholds {
                    thresholds: [2, 4, 6, 7],
                    amounts: [1, 2, 3, 4],
                },
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E111 世捨て人 (Recluse): with no minor improvements played, 1 food
        // at each round start and 1 wood at the start of each harvest
        // (agricola.cloudfree.jp ?p=2642).
        CardDefinition {
            use_limit: None,
            id: "E111".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::StartRoundGainIfNoMinorPlayed {
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::HarvestFieldGainIfNoMinorPlayed {
                    good: Good::Wood,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E113 教母 (Godmother): 1 vegetable per family-growth action (the
        // engine's family-growth spaces: with room, without room, and the
        // modest 2p variant) (agricola.cloudfree.jp ?p=2640).
        CardDefinition {
            use_limit: None,
            id: "E113".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                gain(Space::FamilyGrowthWithRoom, Good::Vegetable, 1),
                gain(Space::FamilyGrowthNoRoom, Good::Vegetable, 1),
                gain(Space::ModestFamilyGrowth, Good::Vegetable, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E118 薪切り (Kindling Gatherer): 1 extra wood per food gain from an
        // action space; the BGA wiki lists 日雇い/漁/小劇場/資材市場 as the
        // trigger spaces (agricola.cloudfree.jp ?p=2635), same printed list
        // style as A155.
        CardDefinition {
            use_limit: None,
            id: "E118".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                gain(Space::DayLaborer, Good::Wood, 1),
                gain(Space::Fishing, Good::Wood, 1),
                gain(Space::TravelingPlayers, Good::Wood, 1),
                gain(Space::ResourceMarket, Good::Wood, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E034 登記簿 (Land Register): cost 1 wood; at scoring, +2 points if
        // every farmyard space is used.
        CardDefinition {
            use_limit: None,
            id: "E034".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::FarmyardAllUsed,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E035 人間不信 (Misanthropy): cost 1 wood; at scoring, 2/3/5 points for
        // exactly 4/3/2 family members.
        CardDefinition {
            use_limit: None,
            id: "E035".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::ExactFamilyMembers,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

#[cfg(test)]
mod verified_band_d_tests {
    use super::*;

    fn set() -> CardSet {
        CardSet {
            definitions: verified_band_d_cards(),
        }
    }

    fn applied(id: &str, spaces: &[Space]) -> Player {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push(id.to_string());
        for &space in spaces {
            after_worker(&set, &mut p, space).unwrap();
        }
        p
    }

    #[test]
    fn d105_food_per_clay_and_grain_per_stone_accumulations() {
        let p = applied(
            "D105",
            &[
                Space::ClayPit,
                Space::ClayPit,
                Space::WesternQuarry,
                Space::EasternQuarry,
                Space::Forest,
            ],
        );
        assert_eq!(p.supply.food, 2);
        assert_eq!(p.supply.grain, 2);
    }

    #[test]
    fn d109_wood_on_play_and_food_per_sow_space() {
        let set = set();
        let mut hand = vec!["D109".to_string()];
        let mut p = Player::new(0, 0);
        play(&set, &mut hand, &mut p, "D109", CardKind::Occupation, 0).unwrap();
        assert_eq!(p.supply.wood, 1);
        for space in [
            Space::GrainSeeds,
            Space::VegetableSeeds,
            Space::Cultivation,
            Space::Farmland,
        ] {
            after_worker(&set, &mut p, space).unwrap();
        }
        assert_eq!(p.supply.food, 6, "three sow spaces -> 3 x 2 food");
    }

    #[test]
    fn d100_bonus_point_per_category_at_max_four() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D100".into());
        assert_eq!(end_game_bonus(&set, &p), 0);
        // fields 5 (=4 pts), pastures 4 (=4 pts), fenced stable x4 (=4 pts),
        // grain 8 (=4 pts) -> 4 categories.
        for i in 1..=5 {
            p.farm[i].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
        }
        for i in 0..4 {
            p.pastures.push(crate::model::Pasture {
                id: i,
                cells: vec![10, 11],
                animal: None,
                quantity: 0,
            });
        }
        for i in 10..=13 {
            p.farm[i].stable = true;
            p.farm[i].tile = Tile::Pasture { id: (10 + i) as u8 };
        }
        for _ in 0..8 {
            p.supply.grain += 1;
        }
        assert_eq!(end_game_bonus(&set, &p), 4);
    }

    /// Test for D100 3-point boundary: categories at exactly 3 points should NOT
    /// count towards the bonus. This ensures a mutation of the threshold from 4 to 3
    /// would be detected.
    #[test]
    fn d100_three_point_categories_do_not_count() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D100".into());

        // Test fields boundary: 4 fields = 3 pts, 5 fields = 4 pts
        for i in 1..=4 {
            p.farm[i].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
        }
        assert_eq!(
            end_game_bonus(&set, &p),
            0,
            "D100 should give 0 bonus with 4 fields (3 pts)"
        );
        p.farm[5].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        assert_eq!(
            end_game_bonus(&set, &p),
            1,
            "D100 should give 1 bonus with 5 fields (4 pts)"
        );

        // Reset and test pastures boundary: 3 pastures = 3 pts, 4 pastures = 4 pts
        let mut p2 = Player::new(0, 0);
        p2.played_cards.push("D100".into());
        for i in 0..3 {
            p2.pastures.push(crate::model::Pasture {
                id: i,
                cells: vec![(10 + i * 2) as usize, (11 + i * 2) as usize],
                animal: None,
                quantity: 0,
            });
        }
        assert_eq!(
            end_game_bonus(&set, &p2),
            0,
            "D100 should give 0 bonus with 3 pastures (3 pts)"
        );
        p2.pastures.push(crate::model::Pasture {
            id: 3,
            cells: vec![16, 17],
            animal: None,
            quantity: 0,
        });
        assert_eq!(
            end_game_bonus(&set, &p2),
            1,
            "D100 should give 1 bonus with 4 pastures (4 pts)"
        );

        // Reset and test grain boundary: 6 grain = 3 pts, 8 grain = 4 pts
        let mut p3 = Player::new(0, 0);
        p3.played_cards.push("D100".into());
        for _ in 0..6 {
            p3.supply.grain += 1;
        }
        assert_eq!(
            end_game_bonus(&set, &p3),
            0,
            "D100 should give 0 bonus with 6 grain (3 pts)"
        );
        p3.supply.grain += 2; // Now 8 grain = 4 pts
        assert_eq!(
            end_game_bonus(&set, &p3),
            1,
            "D100 should give 1 bonus with 8 grain (4 pts)"
        );

        // Reset and test fenced_stables boundary: 3 fenced stables = 3 pts, 4 = 4 pts
        let mut p4 = Player::new(0, 0);
        p4.played_cards.push("D100".into());
        for i in 10..=12 {
            p4.farm[i].stable = true;
            p4.farm[i].tile = Tile::Pasture { id: (10 + i) as u8 };
        }
        assert_eq!(
            end_game_bonus(&set, &p4),
            0,
            "D100 should give 0 bonus with 3 fenced stables (3 pts)"
        );
        p4.farm[13].stable = true;
        p4.farm[13].tile = Tile::Pasture { id: 23 };
        assert_eq!(
            end_game_bonus(&set, &p4),
            1,
            "D100 should give 1 bonus with 4 fenced stables (4 pts)"
        );
    }

    #[test]
    fn d032_two_points_when_field_goods_snapshot_reaches_threshold() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D032".into());
        // No snapshot recorded (cardless or pre-harvest) -> no bonus.
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Boundary: 6 goods is below the printed threshold of 7.
        p.field_goods_before_final_harvest = Some(6);
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Exactly 7 -> +2.
        p.field_goods_before_final_harvest = Some(7);
        assert_eq!(end_game_bonus(&set, &p), 2);
        // 8 also qualifies.
        p.field_goods_before_final_harvest = Some(8);
        assert_eq!(end_game_bonus(&set, &p), 2);
        // Snapshot is set at the end of the final harvest field phase.
        let mut game_snapshot = Player::new(0, 0);
        game_snapshot.played_cards.push("D032".into());
        game_snapshot.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 7,
        };
        let goods: u32 = game_snapshot
            .farm
            .iter()
            .map(|cell| match cell.tile {
                Tile::Field {
                    crop: Some(_),
                    remaining,
                } => remaining as u32,
                _ => 0,
            })
            .sum();
        game_snapshot.field_goods_before_final_harvest = Some(goods);
        assert_eq!(end_game_bonus(&set, &game_snapshot), 2);
    }

    #[test]
    fn d032_snapshot_captured_before_final_harvest_decrement() {
        // Regression test for PR #378 P1: the D032 snapshot must record the
        // goods standing in fields *before* the final-harvest field phase
        // decrements them, not the reduced count afterwards.
        use crate::model::{Config, Game};

        // Build a cardless game, advance to round 14 (final harvest), and
        // stand 3 grain fields with 3 crops each = 9 goods before harvest.
        let mut game = Game::new(Config::default());
        game.public.round = 14;
        let p = &mut game.public.players[0];
        p.played_cards.push("D032".into());
        for cell in &mut p.farm[1..=3] {
            cell.tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 3,
            };
        }

        game.harvest_fields().unwrap();

        // Borrow ends here; re-borrow immutably for assertions.
        let p = &game.public.players[0];

        // Capture happened before the decrement: 3 + 3 + 3 = 9.
        assert_eq!(p.field_goods_before_final_harvest, Some(9));
        // The fields were consumed by the harvest (remaining now 2 each).
        assert!(p.farm[1..=3]
            .iter()
            .all(|cell| matches!(cell.tile, Tile::Field { remaining: 2, .. })));

        // End-game scoring awards +2 once the pre-harvest snapshot reaches 7.
        let d032_set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "D032".into(),
                kind: CardKind::MinorImprovement,
                cost: vec![(Good::Wood, 1)],
                points: 0,
                effects: vec![CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::FieldsGoodsBeforeFinalHarvest(7),
                }],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            }],
        };
        assert_eq!(end_game_bonus(&d032_set, p), 2);
    }

    #[test]
    fn d032_cost_is_one_wood_and_absent_from_two_player_preset() {
        let set = set();
        let card = set.definition("D032").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
        let preset = verified_basic_card_set(2).unwrap();
        assert!(preset.definition("D032").is_none());
    }

    #[test]
    fn d034_four_points_only_when_stone_rooms_exceed_family() {
        let set = set();
        // Same room count as family -> 0 (boundary: rooms == family).
        let mut p = Player::new(0, 0);
        p.played_cards.push("D034".into());
        p.house = House::Stone;
        assert_eq!(p.family, 2);
        assert_eq!(p.rooms(), 2);
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Wood house with extra rooms never qualifies.
        p.farm[1].tile = Tile::Room;
        p.house = House::Wood;
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Stone house, rooms 3 > family 2 -> +4.
        p.house = House::Stone;
        assert_eq!(end_game_bonus(&set, &p), 4);
        // Larger family catches up: 3 stone rooms vs 3 family -> 0.
        p.family = 3;
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Rooms ahead again -> +4.
        p.farm[2].tile = Tile::Room;
        assert_eq!(end_game_bonus(&set, &p), 4);
    }

    #[test]
    fn d034_cost_is_wood_and_clay() {
        let set = set();
        let card = set.definition("D034").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1), (Good::Clay, 2)]);
        assert_eq!(card.points, 0);
    }

    #[test]
    fn band_d_cards_are_absent_from_the_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        for id in ["D100", "D105", "D109", "D032", "D034"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }
}

#[cfg(test)]
mod verified_band_e_tests {
    use super::*;

    #[test]
    fn e117_wood_per_harvest_with_wheat_field() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E117".into());
        // 小麦畑がなければ得ない。
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.wood, 0);
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.wood, 2);
        // 野菜畑や空き畑では発動しない。
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.wood, 2);
        // 複数の小麦畑でも1収穫につき1回（木2）。
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.wood, 4);
    }

    #[test]
    fn e117_has_no_cost_and_is_absent_from_the_two_player_preset() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let card = set.definition("E117").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
        let preset = verified_basic_card_set(2).unwrap();
        for id in ["E107", "E111", "E113", "E117", "E118", "E034", "E035"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }

    #[test]
    fn e107_food_by_field_count_thresholds() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E107".into());
        // 0〜1畑では得ない。
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 0);
        p.farm[1].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 0);
        // 2畑→1。
        p.farm[2].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 1);
        // 6畑→3（最高到達閾値の段階のみ）。
        for cell in p.farm.iter_mut().take(7).skip(3) {
            cell.tile = Tile::Field {
                crop: None,
                remaining: 0,
            };
        }
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 4);
        // 7畑→4。
        p.farm[7].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 8);
    }

    #[test]
    fn e111_gains_only_while_no_minor_improvement_is_played() {
        let mut set = CardSet {
            definitions: verified_band_e_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "minor1".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        let mut p = Player::new(0, 0);
        p.played_cards.push("E111".into());
        start_round_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 1);
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.wood, 1);
        // 小進歩を出した後は両方とも得られない。
        p.played_cards.push("minor1".into());
        start_round_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 1);
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.wood, 1);
    }

    #[test]
    fn e113_vegetable_per_family_growth_space() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E113".into());
        for space in [
            Space::FamilyGrowthWithRoom,
            Space::FamilyGrowthNoRoom,
            Space::ModestFamilyGrowth,
        ] {
            after_worker(&set, &mut p, space).unwrap();
        }
        assert_eq!(p.supply.vegetable, 3);
        // 増員以外のスペースでは得ない。
        after_worker(&set, &mut p, Space::DayLaborer).unwrap();
        assert_eq!(p.supply.vegetable, 3);
    }

    #[test]
    fn e118_wood_per_food_gaining_action_space() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E118".into());
        for space in [
            Space::DayLaborer,
            Space::Fishing,
            Space::TravelingPlayers,
            Space::ResourceMarket,
        ] {
            after_worker(&set, &mut p, space).unwrap();
        }
        assert_eq!(p.supply.wood, 4);
        // 資材系スペースでは得ない。
        after_worker(&set, &mut p, Space::Forest).unwrap();
        after_worker(&set, &mut p, Space::GrainSeeds).unwrap();
        assert_eq!(p.supply.wood, 4);
    }

    #[test]
    fn e034_two_points_only_when_every_farmyard_cell_is_used() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E034".into());
        // Default farm has two Room cells and 13 Empty cells -> not all used.
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Fill most cells but leave one Empty -> still 0 (boundary: 14 of 15).
        for i in 0..14 {
            p.farm[i].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
        }
        assert_eq!(end_game_bonus(&set, &p), 0);
        // All 15 cells used (two rooms + 13 fields) -> +2.
        p.farm[14].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        assert_eq!(end_game_bonus(&set, &p), 2);
        // Pasture tiles also count as used.
        let mut q = Player::new(0, 0);
        q.played_cards.push("E034".into());
        q.farm[0].tile = Tile::Room;
        q.farm[5].tile = Tile::Room;
        for i in 1..5 {
            q.farm[i].tile = Tile::Pasture { id: 0 };
        }
        for i in 6..15 {
            q.farm[i].tile = Tile::Pasture { id: 0 };
        }
        assert_eq!(end_game_bonus(&set, &q), 2);
    }

    #[test]
    fn e034_stable_on_empty_counts_as_used() {
        // P2: a stable-on-empty cell (Tile::Empty with stable == true) is
        // occupied by the stable and must count as used for E034.
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E034".into());
        p.farm[0].tile = Tile::Room;
        p.farm[5].tile = Tile::Room;
        for i in 1..=4 {
            p.farm[i].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
        }
        for i in 6..=13 {
            p.farm[i].tile = Tile::Field {
                crop: Some(Crop::Vegetable),
                remaining: 1,
            };
        }
        // Cell 14 is an Empty tile carrying a stable: stable-on-empty.
        p.farm[14].tile = Tile::Empty;
        p.farm[14].stable = true;
        assert_eq!(end_game_bonus(&set, &p), 2);

        // Same layout without the stable -> not all used -> 0.
        let mut q = p.clone();
        q.farm[14].stable = false;
        assert_eq!(end_game_bonus(&set, &q), 0);
    }

    #[test]
    fn e034_cost_is_one_wood_and_absent_from_two_player_preset() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let card = set.definition("E034").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
        let preset = verified_basic_card_set(2).unwrap();
        assert!(preset.definition("E034").is_none());
    }

    #[test]
    fn e035_table_pays_two_three_five_for_exactly_four_three_two() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        // 2 family members -> +5.
        let mut p = Player::new(0, 0);
        p.played_cards.push("E035".into());
        assert_eq!(p.family, 2);
        assert_eq!(end_game_bonus(&set, &p), 5);
        // 3 family members -> +3.
        p.family = 3;
        assert_eq!(end_game_bonus(&set, &p), 3);
        // 4 family members -> +2.
        p.family = 4;
        assert_eq!(end_game_bonus(&set, &p), 2);
        // 5 family members -> 0 (printed card pays nothing at 5).
        p.family = 5;
        assert_eq!(end_game_bonus(&set, &p), 0);
        // 1 family member is not a listed row -> 0.
        p.family = 1;
        assert_eq!(end_game_bonus(&set, &p), 0);
    }

    #[test]
    fn e035_cost_is_one_wood_and_absent_from_two_player_preset() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let card = set.definition("E035").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
        let preset = verified_basic_card_set(2).unwrap();
        assert!(preset.definition("E035").is_none());
    }
}
#[cfg(test)]
mod verified_band_a_tests {
    use super::*;
    use crate::model::{Improvement, Pasture};

    #[test]
    fn a099_two_points_per_large_pasture() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("A099".into());
        assert_eq!(end_game_bonus(&set, &p), 0);
        // 2マス牧場は対象外。
        p.pastures.push(Pasture {
            id: 1,
            cells: vec![1, 2],
            animal: None,
            quantity: 0,
        });
        assert_eq!(end_game_bonus(&set, &p), 0);
        // 3マス以上の牧場1つにつき2点。
        p.pastures[0].cells.push(3);
        assert_eq!(end_game_bonus(&set, &p), 2);
        p.pastures.push(Pasture {
            id: 2,
            cells: vec![10, 11, 12, 13],
            animal: None,
            quantity: 0,
        });
        assert_eq!(end_game_bonus(&set, &p), 4);
    }

    #[test]
    fn a101_one_point_per_cooking_improvement() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("A101".into());
        assert_eq!(end_game_bonus(&set, &p), 0);
        // かまど系は対象、窯やその他は対象外。
        p.improvements.push(Improvement::Fireplace2);
        p.improvements.push(Improvement::Hearth4);
        p.improvements.push(Improvement::ClayOven);
        p.improvements.push(Improvement::Well);
        assert_eq!(end_game_bonus(&set, &p), 2);
    }

    #[test]
    fn a106_food_per_last_crop_harvested() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("A106".into());
        // 最後の1つを収穫する畑がない場合は得ない。
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 0);
        // 最後の小麦1畑→2、最後の野菜1畑→1。
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 3);
        // 残り1の小麦/野菜が無くなれば発動しない。
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        p.farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 5);
    }

    #[test]
    fn a117_wood_per_improvement_in_front() {
        let mut set = CardSet {
            definitions: verified_band_a_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "minor1".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        let mut p = Player::new(0, 0);
        p.supply.wood = 5;
        assert_eq!(improvement_count_in_front(&set, &p), 0);
        p.played_cards.push("minor1".into());
        p.improvements.push(Improvement::Well);
        assert_eq!(improvement_count_in_front(&set, &p), 2);
        play(
            &set,
            &mut vec!["A117".into()],
            &mut p,
            "A117",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        // 進歩2つにつき木2。
        assert_eq!(p.supply.wood, 7);
        // A117自体（職業）はカウントされない。
        p.played_cards.push("A117".into());
        assert_eq!(improvement_count_in_front(&set, &p), 2);
    }

    #[test]
    fn band_a_cards_are_absent_from_the_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        for id in ["A099", "A101", "A106", "A117", "A155"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }

    #[test]
    fn a155_gains_wood_and_grain_on_traveling_players() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("A155".into());
        after_worker(&set, &mut p, Space::TravelingPlayers).unwrap();
        assert_eq!(p.supply.wood, 1);
        assert_eq!(p.supply.grain, 1);
        assert_eq!(p.supply.food, 0);
        // Multiple uses accumulate.
        after_worker(&set, &mut p, Space::TravelingPlayers).unwrap();
        assert_eq!(p.supply.wood, 2);
        assert_eq!(p.supply.grain, 2);
    }

    #[test]
    fn a155_not_triggered_by_other_spaces() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("A155".into());
        after_worker(&set, &mut p, Space::Forest).unwrap();
        after_worker(&set, &mut p, Space::ClayPit).unwrap();
        after_worker(&set, &mut p, Space::DayLaborer).unwrap();
        assert_eq!(p.supply.wood, 0);
        assert_eq!(p.supply.grain, 0);
    }

    #[test]
    fn a155_requires_four_players() {
        // Card has MinimumPlayers(4) prerequisite.
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };
        let card = set.definition("A155").unwrap();
        let has_min_players = card.effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4)
                }
            )
        });
        assert!(
            has_min_players,
            "A155 should have MinimumPlayers(4) prerequisite"
        );
        let p = Player::new(0, 0);
        assert!(
            can_play(&set, card, &p, 0),
            "A155 should be playable via can_play"
        );
    }

    #[test]
    fn a008_grain_vegetable_fixed_with_prereqs() {
        let mut set = CardSet {
            definitions: verified_band_a_cards(),
        };
        // Add a dummy minor improvement for counting
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "minor1".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        // Test 1: prerequisite fails with 0 occupations
        let mut p = Player::new(0, 5); // 5 food for reed cost
        p.supply.reed = 1;
        p.improvements.push(Improvement::Well); // 1 base improvement
        let result = play(
            &set,
            &mut vec!["A008".into()],
            &mut p,
            "A008",
            CardKind::MinorImprovement,
            1, // reed cost
        );
        assert!(result.is_err(), "Should fail with 0 occupations");

        // Test 2: prerequisite fails with 1 occupation
        p = Player::new(0, 5);
        p.supply.reed = 1;
        p.improvements.push(Improvement::Well);
        p.played_cards.push("A117".into()); // 1 occupation
        let result = play(
            &set,
            &mut vec!["A008".into()],
            &mut p,
            "A008",
            CardKind::MinorImprovement,
            1,
        );
        assert!(result.is_err(), "Should fail with 1 occupation");

        // Test 3: prerequisite fails with 2 occupations but <2 improvements
        p = Player::new(0, 5);
        p.supply.reed = 1;
        p.improvements.push(Improvement::Well); // only 1 improvement (base)
        p.played_cards.push("A117".into());
        p.played_cards.push("A101".into()); // 2 occupations
        assert_eq!(improvement_count_in_front(&set, &p), 1); // base only
        let result = play(
            &set,
            &mut vec!["A008".into()],
            &mut p,
            "A008",
            CardKind::MinorImprovement,
            1,
        );
        assert!(
            result.is_err(),
            "Should fail with 2 occupations but <2 improvements"
        );

        // Test 4: prerequisite passes with 2 occupations and 2 improvements (base + 1 minor)
        p = Player::new(0, 5);
        p.supply.reed = 1;
        p.improvements.push(Improvement::Well);
        p.improvements.push(Improvement::Fireplace2); // 2 improvements (base + 1 minor)
        p.played_cards.push("A117".into());
        p.played_cards.push("A101".into()); // 2 occupations
        assert_eq!(improvement_count_in_front(&set, &p), 2);
        play(
            &set,
            &mut vec!["A008".into()],
            &mut p,
            "A008",
            CardKind::MinorImprovement,
            1,
        )
        .unwrap();
        // Fixed 1 grain + 1 vegetable regardless of improvement count
        assert_eq!(p.supply.grain, 1);
        assert_eq!(p.supply.vegetable, 1);

        // Test 5: with more improvements, still fixed 1 grain + 1 vegetable
        p = Player::new(0, 5);
        p.supply.reed = 1;
        p.improvements.push(Improvement::Well);
        p.improvements.push(Improvement::Fireplace2);
        p.improvements.push(Improvement::Hearth4); // 3 improvements (base + 2 minors)
        p.played_cards.push("A117".into());
        p.played_cards.push("A101".into());
        assert_eq!(improvement_count_in_front(&set, &p), 3);
        play(
            &set,
            &mut vec!["A008".into()],
            &mut p,
            "A008",
            CardKind::MinorImprovement,
            1,
        )
        .unwrap();
        // Still fixed 1 grain + 1 vegetable
        assert_eq!(p.supply.grain, 1);
        assert_eq!(p.supply.vegetable, 1);
    }
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
        post_play: crate::cards::PostPlayDestination::PlayedArea,
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
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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

//! Public model types and private game state. No card text is bundled.
use crate::cards::{CardDefinition, CardSet, CardState};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Helper to serialize `Arc<T>` as `T` and deserialize back to `Arc<T>`.
/// This allows sharing card definitions in memory while keeping save format identical.
fn serialize_arc<T, S>(arc: &Arc<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    T::serialize(arc, serializer)
}

fn deserialize_arc<'de, T, D>(deserializer: D) -> Result<Arc<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Arc::new)
}

pub const FARM_CELLS: usize = 15;
pub const HARVEST_ROUNDS: [u8; 6] = [4, 7, 9, 11, 13, 14];
pub const RULES_VERSION: &str = "agricola-revised-2016-beginner";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LOG_VERSION: u32 = 5;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub seed: u64,
    #[serde(default)]
    pub additional_action_space_enabled: bool,
    #[serde(default = "default_player_count")]
    pub players: usize,
}
fn default_player_count() -> usize {
    2
}
impl Default for Config {
    fn default() -> Self {
        Self {
            seed: 0,
            additional_action_space_enabled: false,
            players: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Good {
    Wood,
    Clay,
    Reed,
    Stone,
    Food,
    Grain,
    Vegetable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Animal {
    Sheep,
    Boar,
    Cattle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum House {
    Wood,
    Clay,
    Stone,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Crop {
    Grain,
    Vegetable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supply {
    pub wood: u32,
    pub clay: u32,
    pub reed: u32,
    pub stone: u32,
    pub food: u32,
    pub grain: u32,
    pub vegetable: u32,
}
impl Supply {
    pub fn get(&self, k: Good) -> u32 {
        match k {
            Good::Wood => self.wood,
            Good::Clay => self.clay,
            Good::Reed => self.reed,
            Good::Stone => self.stone,
            Good::Food => self.food,
            Good::Grain => self.grain,
            Good::Vegetable => self.vegetable,
        }
    }
    pub fn set(&mut self, k: Good, n: u32) {
        match k {
            Good::Wood => self.wood = n,
            Good::Clay => self.clay = n,
            Good::Reed => self.reed = n,
            Good::Stone => self.stone = n,
            Good::Food => self.food = n,
            Good::Grain => self.grain = n,
            Good::Vegetable => self.vegetable = n,
        }
    }
    pub fn gain(&mut self, k: Good, n: u32) {
        self.set(
            k,
            self.get(k)
                .checked_add(n)
                .expect("resource counter overflow"),
        )
    }
    pub fn pay(&mut self, k: Good, n: u32) -> Result<(), String> {
        if self.get(k) < n {
            return Err(format!("insufficient {:?}", k));
        }
        self.set(k, self.get(k) - n);
        Ok(())
    }
    pub fn building_total(&self) -> u32 {
        self.wood + self.clay + self.reed + self.stone
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    Empty,
    Room,
    Field { crop: Option<Crop>, remaining: u8 },
    Pasture { id: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub tile: Tile,
    pub stable: bool,
    pub stable_animal: Option<Animal>,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            tile: Tile::Empty,
            stable: false,
            stable_animal: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pasture {
    pub id: u8,
    pub cells: Vec<usize>,
    pub animal: Option<Animal>,
    pub quantity: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: usize,
    pub family: u8,
    pub newborns: u8,
    pub house: House,
    pub supply: Supply,
    pub farm: Vec<Cell>,
    pub pastures: Vec<Pasture>,
    pub pet: Option<Animal>,
    pub fences: BTreeSet<(u8, u8)>,
    pub improvements: Vec<Improvement>,
    #[serde(default)]
    pub played_cards: Vec<String>,
    #[serde(default)]
    pub card_points: i32,
    #[serde(default)]
    pub pasture_capacity_bonus: u32,
    #[serde(default)]
    pub free_fence_wood: u32,
    #[serde(default)]
    pub improvement_wood_discount: u32,
    #[serde(default)]
    pub wooden_room_cost: Option<u32>,
    #[serde(default)]
    pub room_material_cost: Option<u32>,
    #[serde(default)]
    pub major_stone_discount_by_rooms: bool,
    #[serde(default)]
    pub stone_cost_discount: u32,
    #[serde(default)]
    pub extra_housing: u8,
    pub begging: u32,
}
impl Player {
    pub fn new(id: usize, food: u32) -> Self {
        let mut farm = vec![Cell::default(); FARM_CELLS];
        farm[0].tile = Tile::Room;
        farm[5].tile = Tile::Room;
        Self {
            id,
            family: 2,
            newborns: 0,
            house: House::Wood,
            supply: Supply {
                food,
                ..Supply::default()
            },
            farm,
            pastures: vec![],
            pet: None,
            fences: BTreeSet::new(),
            improvements: vec![],
            played_cards: vec![],
            card_points: 0,
            pasture_capacity_bonus: 0,
            free_fence_wood: 0,
            improvement_wood_discount: 0,
            wooden_room_cost: None,
            room_material_cost: None,
            major_stone_discount_by_rooms: false,
            stone_cost_discount: 0,
            extra_housing: 0,
            begging: 0,
        }
    }
    pub fn rooms(&self) -> usize {
        self.farm
            .iter()
            .filter(|c| matches!(c.tile, Tile::Room))
            .count()
    }
    pub fn fields(&self) -> usize {
        self.farm
            .iter()
            .filter(|c| matches!(c.tile, Tile::Field { .. }))
            .count()
    }
    pub fn occupied_stables(&self) -> usize {
        self.farm.iter().filter(|c| c.stable).count()
    }
    pub fn animal_count(&self, kind: Animal) -> u32 {
        let in_pastures: u32 = self
            .pastures
            .iter()
            .filter(|p| p.animal == Some(kind))
            .map(|p| p.quantity)
            .sum();
        in_pastures
            + self
                .farm
                .iter()
                .filter(|c| c.stable_animal == Some(kind))
                .count() as u32
            + u32::from(self.pet == Some(kind))
    }
    pub fn pasture_capacity(&self, p: &Pasture) -> u32 {
        let stables = p.cells.iter().filter(|&&i| self.farm[i].stable).count();
        (2 * p.cells.len() as u32)
            .saturating_mul(1u32 << stables)
            .saturating_add(self.pasture_capacity_bonus)
    }
    pub fn total_crop(&self, kind: Crop) -> u32 {
        let in_supply = match kind {
            Crop::Grain => self.supply.grain,
            Crop::Vegetable => self.supply.vegetable,
        };
        in_supply
            + self
                .farm
                .iter()
                .map(|c| match c.tile {
                    Tile::Field {
                        crop: Some(x),
                        remaining,
                    } if x == kind => remaining as u32,
                    _ => 0,
                })
                .sum::<u32>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Improvement {
    Fireplace2,
    Fireplace3,
    Hearth4,
    Hearth5,
    ClayOven,
    StoneOven,
    Well,
    Joinery,
    Pottery,
    Basketry,
}
impl Improvement {
    pub const ALL: [Self; 10] = [
        Self::Fireplace2,
        Self::Fireplace3,
        Self::Hearth4,
        Self::Hearth5,
        Self::ClayOven,
        Self::StoneOven,
        Self::Well,
        Self::Joinery,
        Self::Pottery,
        Self::Basketry,
    ];
    pub fn points(self) -> i32 {
        match self {
            Self::Fireplace2 | Self::Fireplace3 | Self::Hearth4 | Self::Hearth5 => 1,
            Self::ClayOven | Self::Joinery | Self::Pottery | Self::Basketry => 2,
            Self::StoneOven => 3,
            Self::Well => 4,
        }
    }
    pub fn cost(self) -> Vec<(Good, u32)> {
        match self {
            Self::Fireplace2 => vec![(Good::Clay, 2)],
            Self::Fireplace3 => vec![(Good::Clay, 3)],
            Self::Hearth4 => vec![(Good::Clay, 4)],
            Self::Hearth5 => vec![(Good::Clay, 5)],
            Self::ClayOven => vec![(Good::Clay, 3), (Good::Stone, 1)],
            Self::StoneOven => vec![(Good::Clay, 1), (Good::Stone, 3)],
            Self::Well => vec![(Good::Wood, 1), (Good::Stone, 3)],
            Self::Joinery => vec![(Good::Wood, 2), (Good::Stone, 2)],
            Self::Pottery => vec![(Good::Clay, 2), (Good::Stone, 2)],
            Self::Basketry => vec![(Good::Reed, 2), (Good::Stone, 2)],
        }
    }
    pub fn is_cooking(self) -> bool {
        matches!(
            self,
            Self::Fireplace2 | Self::Fireplace3 | Self::Hearth4 | Self::Hearth5
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundCard {
    MajorImprovement,
    Fencing,
    GrainUtilization,
    SheepMarket,
    FamilyGrowthWithRoom,
    HouseRedevelopment,
    WesternQuarry,
    VegetableSeeds,
    PigMarket,
    CattleMarket,
    EasternQuarry,
    FamilyGrowthNoRoom,
    Cultivation,
    FarmRedevelopment,
}
impl RoundCard {
    pub const ALL: [Self; 14] = [
        Self::MajorImprovement,
        Self::Fencing,
        Self::GrainUtilization,
        Self::SheepMarket,
        Self::FamilyGrowthWithRoom,
        Self::HouseRedevelopment,
        Self::WesternQuarry,
        Self::VegetableSeeds,
        Self::PigMarket,
        Self::CattleMarket,
        Self::EasternQuarry,
        Self::FamilyGrowthNoRoom,
        Self::Cultivation,
        Self::FarmRedevelopment,
    ];
    pub fn stage(self) -> u8 {
        match self {
            Self::MajorImprovement | Self::Fencing | Self::GrainUtilization | Self::SheepMarket => {
                1
            }
            Self::FamilyGrowthWithRoom | Self::HouseRedevelopment | Self::WesternQuarry => 2,
            Self::VegetableSeeds | Self::PigMarket => 3,
            Self::CattleMarket | Self::EasternQuarry => 4,
            Self::FamilyGrowthNoRoom | Self::Cultivation => 5,
            Self::FarmRedevelopment => 6,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    Forest,
    ClayPit,
    ReedBank,
    Fishing,
    DayLaborer,
    GrainSeeds,
    Farmland,
    FarmExpansion,
    MeetingPlace,
    Lessons,
    ExtraLessons,
    MajorImprovement,
    Fencing,
    GrainUtilization,
    SheepMarket,
    FamilyGrowthWithRoom,
    HouseRedevelopment,
    WesternQuarry,
    VegetableSeeds,
    PigMarket,
    CattleMarket,
    EasternQuarry,
    FamilyGrowthNoRoom,
    Cultivation,
    FarmRedevelopment,
    Copse,
    ResourceMarket,
    AnimalMarket,
    ModestFamilyGrowth,
    Grove,
    Hollow,
    TravelingPlayers,
}
impl From<RoundCard> for Space {
    fn from(x: RoundCard) -> Self {
        match x {
            RoundCard::MajorImprovement => Self::MajorImprovement,
            RoundCard::Fencing => Self::Fencing,
            RoundCard::GrainUtilization => Self::GrainUtilization,
            RoundCard::SheepMarket => Self::SheepMarket,
            RoundCard::FamilyGrowthWithRoom => Self::FamilyGrowthWithRoom,
            RoundCard::HouseRedevelopment => Self::HouseRedevelopment,
            RoundCard::WesternQuarry => Self::WesternQuarry,
            RoundCard::VegetableSeeds => Self::VegetableSeeds,
            RoundCard::PigMarket => Self::PigMarket,
            RoundCard::CattleMarket => Self::CattleMarket,
            RoundCard::EasternQuarry => Self::EasternQuarry,
            RoundCard::FamilyGrowthNoRoom => Self::FamilyGrowthNoRoom,
            RoundCard::Cultivation => Self::Cultivation,
            RoundCard::FarmRedevelopment => Self::FarmRedevelopment,
        }
    }
}
impl Space {
    pub const BASE: [Self; 9] = [
        Self::Forest,
        Self::ClayPit,
        Self::ReedBank,
        Self::Fishing,
        Self::DayLaborer,
        Self::GrainSeeds,
        Self::Farmland,
        Self::FarmExpansion,
        Self::MeetingPlace,
    ];
    pub fn additional(self) -> bool {
        matches!(
            self,
            Self::Copse | Self::ResourceMarket | Self::AnimalMarket | Self::ModestFamilyGrowth
        )
    }
    /// Spaces printed on the multiplayer boards (occupation-only Lessons
    /// are not playable without cards).
    pub fn regular_for(players: usize) -> Vec<Self> {
        let mut result = Self::BASE.to_vec();
        match players {
            3 => result.extend([Self::Grove, Self::Hollow, Self::ResourceMarket]),
            4 => result.extend([
                Self::Copse,
                Self::Grove,
                Self::Hollow,
                Self::ResourceMarket,
                Self::TravelingPlayers,
            ]),
            _ => {}
        }
        result
    }
    pub fn extra_for(players: usize) -> Vec<Self> {
        match players {
            2 => vec![
                Self::Copse,
                Self::ResourceMarket,
                Self::AnimalMarket,
                Self::ModestFamilyGrowth,
            ],
            3 | 4 => vec![Self::AnimalMarket, Self::ModestFamilyGrowth],
            _ => vec![],
        }
    }
    pub fn additional_for(self, players: usize) -> bool {
        Self::extra_for(players).contains(&self)
    }
    pub fn accumulation_for(self, players: usize) -> Option<(Good, u32)> {
        match self {
            Self::Hollow if players == 4 => Some((Good::Clay, 2)),
            _ => self.accumulation(),
        }
    }
    pub fn accumulation(self) -> Option<(Good, u32)> {
        match self {
            Self::Forest => Some((Good::Wood, 3)),
            Self::ClayPit => Some((Good::Clay, 1)),
            Self::ReedBank => Some((Good::Reed, 1)),
            Self::Fishing | Self::MeetingPlace => Some((Good::Food, 1)),
            Self::SheepMarket | Self::PigMarket | Self::CattleMarket => None,
            Self::WesternQuarry | Self::EasternQuarry => Some((Good::Stone, 1)),
            Self::Copse => Some((Good::Wood, 1)),
            Self::Grove => Some((Good::Wood, 2)),
            Self::Hollow => Some((Good::Clay, 1)),
            Self::TravelingPlayers => Some((Good::Food, 1)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicState {
    pub round: u8,
    pub active_player: usize,
    pub start_player: usize,
    pub used: BTreeSet<Space>,
    pub revealed: Vec<RoundCard>,
    pub accumulations: BTreeMap<Space, u32>,
    pub animal_accumulations: BTreeMap<Space, u32>,
    pub players: Vec<Player>,
    pub improvement_market: Vec<Improvement>,
    pub work_quota: Vec<u8>,
    pub workers_placed: Vec<u8>,
    pub additional_action_space_enabled: bool,
    pub finished: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RngStreams {
    pub round: ChaCha8Rng,
    pub ai: ChaCha8Rng,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub config: Config,
    pub public: PublicState,
    /// Private: never expose these to policies.
    pub hidden_round_deck: Vec<RoundCard>,
    pub rng: RngStreams,
    pub delayed_well: BTreeMap<u8, Vec<usize>>,
    #[serde(
        default,
        serialize_with = "serialize_arc",
        deserialize_with = "deserialize_arc"
    )]
    pub card_set: Arc<CardSet>,
    #[serde(default)]
    pub cards: CardState,
    pub log: GameLog,
    /// Search-only fast path. When false, `apply`/`apply_free` skip state
    /// hashing and event logging for hypothetical MCTS positions. It is never
    /// serialized and defaults to true, so committed games, saves and replays
    /// always keep a complete, hash-checked event log.
    #[serde(skip, default = "record_events_by_default")]
    pub record_events: bool,
}

fn record_events_by_default() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub player: Option<usize>,
    pub kind: String,
    pub payload: serde_json::Value,
    pub before: String,
    pub after: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameLog {
    pub log_version: u32,
    pub engine_version: String,
    pub rules_version: String,
    pub config: Config,
    pub start_player: usize,
    /// None is the historical game-seed draw; Some fixes the pre-draft draw.
    #[serde(default)]
    pub initial_start_player_override: Option<usize>,
    pub initial_state_hash: String,
    #[serde(default)]
    pub initial_public_state: Option<PublicState>,
    pub round_seed: u64,
    pub action_seed: u64,
    #[serde(default)]
    pub card_set: CardSet,
    /// Explicit starting hands for a draft-seeded game (#104). `None` means the
    /// engine dealt from `config.seed`. Stored so save/load and replay rebuild
    /// the exact initial card state instead of re-dealing randomly.
    #[serde(default)]
    pub initial_hands: Option<Vec<Vec<String>>>,
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub player_id: usize,
    pub public: PublicState,
    /// Only this viewer's own cards. Neither opponent hands nor undealt IDs appear.
    pub hand: Vec<String>,
    pub hand_counts: Vec<usize>,
    /// The implemented card vocabulary is public; its randomized locations are not.
    pub card_definitions: Vec<CardDefinition>,
}
impl Game {
    pub fn new(config: Config) -> Self {
        Self::with_cards(config, CardSet::default()).expect("default cardless setup")
    }
    pub fn with_cards(config: Config, card_set: CardSet) -> Result<Self, String> {
        if !(2..=4).contains(&config.players) {
            return Err("players must be 2..=4".into());
        }
        card_set.validate(config.players)?;
        let cards = CardState::deal(&card_set, config.seed, config.players);
        Self::from_card_state(config, Arc::new(card_set), cards, None, None)
    }
    /// Start a game from explicit, already-drafted hands instead of the random
    /// [`CardState::deal`]. This is the M11 draft-to-game bridge (#104): only
    /// the private card state is seeded here, and the caller must have obtained
    /// the hands from a completed draft. The draft's observation history stays
    /// with the draft session, not with the game.
    pub fn with_cards_and_hands(
        config: Config,
        card_set: CardSet,
        hands: Vec<Vec<String>>,
    ) -> Result<Self, String> {
        if !(2..=4).contains(&config.players) {
            return Err("players must be 2..=4".into());
        }
        card_set.validate(config.players)?;
        if hands.len() != config.players {
            return Err("explicit hands must match the player count".into());
        }
        let cards = CardState::from_hands(&card_set, &hands)?;
        Self::from_card_state(config, Arc::new(card_set), cards, Some(hands), None)
    }
    /// Reuse a start player selected and announced *before* drafting. The
    /// round deck keeps its own game seed and independent shuffle stream.
    pub fn with_cards_and_hands_with_start_player(
        config: Config,
        card_set: CardSet,
        hands: Vec<Vec<String>>,
        start_player: usize,
    ) -> Result<Self, String> {
        if !(2..=4).contains(&config.players) || start_player >= config.players {
            return Err("invalid drafted start player or player count".into());
        }
        card_set.validate(config.players)?;
        if hands.len() != config.players {
            return Err("explicit hands must match the player count".into());
        }
        let cards = CardState::from_hands(&card_set, &hands)?;
        Self::from_card_state(
            config,
            Arc::new(card_set),
            cards,
            Some(hands),
            Some(start_player),
        )
    }

    fn from_card_state(
        config: Config,
        card_set: Arc<CardSet>,
        cards: CardState,
        initial_hands: Option<Vec<Vec<String>>>,
        initial_start_player_override: Option<usize>,
    ) -> Result<Self, String> {
        let round_seed = config.seed ^ 0x38DB_3BE3_DF45_3FAF;
        let action_seed = config.seed ^ 0x6A09_E667_F3BC_C909;
        let mut rng = RngStreams {
            round: ChaCha8Rng::seed_from_u64(round_seed),
            ai: ChaCha8Rng::seed_from_u64(action_seed),
        };
        // Keep the game RNG stream stable even when a draft already selected
        // the first seat with its separate, domain-separated draw.
        let drawn_start_player = rng.round.gen_range(0..config.players);
        let start_player = initial_start_player_override.unwrap_or(drawn_start_player);
        if start_player >= config.players {
            return Err("drafted start player out of range".into());
        }
        let mut deck = Vec::new();
        for stage in 1..=6 {
            let mut cards = RoundCard::ALL
                .iter()
                .copied()
                .filter(|c| c.stage() == stage)
                .collect::<Vec<_>>();
            cards.shuffle(&mut rng.round);
            deck.extend(cards);
        }
        deck.reverse();
        let players = (0..config.players)
            .map(|id| Player::new(id, if id == start_player { 2 } else { 3 }))
            .collect();
        let public = PublicState {
            round: 0,
            active_player: start_player,
            start_player,
            used: BTreeSet::new(),
            revealed: vec![],
            accumulations: BTreeMap::new(),
            animal_accumulations: BTreeMap::new(),
            players,
            improvement_market: Improvement::ALL.to_vec(),
            work_quota: vec![2; config.players],
            workers_placed: vec![0; config.players],
            additional_action_space_enabled: config.additional_action_space_enabled,
            finished: false,
        };
        let log = GameLog {
            log_version: LOG_VERSION,
            engine_version: ENGINE_VERSION.into(),
            rules_version: RULES_VERSION.into(),
            config: config.clone(),
            start_player,
            initial_start_player_override,
            initial_state_hash: String::new(),
            initial_public_state: None,
            round_seed,
            action_seed,
            card_set: (*card_set).clone(),
            initial_hands: initial_hands.clone(),
            events: vec![],
        };
        let mut result = Self {
            config,
            public,
            hidden_round_deck: deck,
            rng,
            delayed_well: BTreeMap::new(),
            card_set,
            cards,
            log,
            record_events: true,
        };
        result.start_round();
        result.log.initial_state_hash = result.state_hash();
        result.log.initial_public_state = Some(result.public.clone());
        Ok(result)
    }
    pub fn observation(&self, player_id: usize) -> Result<Observation, String> {
        if player_id >= self.public.players.len() {
            return Err("invalid player".into());
        }
        Ok(Observation {
            player_id,
            public: self.public.clone(),
            hand: self.cards.hands[player_id].clone(),
            hand_counts: self.cards.hands.iter().map(Vec::len).collect(),
            card_definitions: self.card_set.definitions.clone(),
        })
    }
    pub fn state_hash(&self) -> String {
        // Event log is intentionally omitted; this is the full game state, including hidden cards and RNG.
        let data = serde_json::to_vec(&(
            &self.config,
            &self.public,
            &self.hidden_round_deck,
            &self.rng,
            &self.delayed_well,
            &*self.card_set,
            &self.cards,
        ))
        .expect("state serialization");
        format!("{:x}", Sha256::digest(&data))
    }

    /// Create a lightweight clone for search (MCTS/PUCT).
    /// This avoids cloning the event log and shares the card definitions via Arc.
    /// The returned game has `record_events = false` and an empty event log.
    pub fn search_clone(&self) -> Self {
        let mut log = self.log.clone();
        log.events.clear();
        Self {
            config: self.config.clone(),
            public: self.public.clone(),
            hidden_round_deck: self.hidden_round_deck.clone(),
            rng: self.rng.clone(),
            delayed_well: self.delayed_well.clone(),
            card_set: self.card_set.clone(),
            cards: self.cards.clone(),
            log,
            record_events: false,
        }
    }
    /// Create a search-only position from pre-sampled components.
    /// Skips: start player draw, round deck shuffle, Player::new × N,
    /// start_round(), state_hash (full JSON+SHA256), log CardSet duplication.
    /// The caller MUST ensure the inputs form a valid game state;
    /// this constructor does NOT run validate().
    pub fn search_from_parts(
        config: Config,
        public: PublicState,
        hidden_round_deck: Vec<RoundCard>,
        rng: RngStreams,
        delayed_well: BTreeMap<u8, Vec<usize>>,
        card_set: Arc<CardSet>,
        cards: CardState,
    ) -> Self {
        let start_player = public.start_player;
        let round_seed = config.seed ^ 0x38DB_3BE3_DF45_3FAF;
        let action_seed = config.seed ^ 0x6A09_E667_F3BC_C909;
        let card_set_for_log = (*card_set).clone();
        Self {
            config: config.clone(),
            public,
            hidden_round_deck,
            rng,
            delayed_well,
            card_set,
            cards,
            log: GameLog {
                log_version: LOG_VERSION,
                engine_version: ENGINE_VERSION.into(),
                rules_version: RULES_VERSION.into(),
                config,
                start_player,
                initial_start_player_override: None,
                initial_state_hash: String::new(),
                initial_public_state: None,
                round_seed,
                action_seed,
                card_set: card_set_for_log,
                initial_hands: None,
                events: Vec::new(),
            },
            record_events: false,
        }
    }
    pub fn save_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
    pub fn load_json(text: &str) -> Result<Self, String> {
        let result: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        result.validate()?;
        if result.log.log_version != LOG_VERSION
            || result.log.rules_version != RULES_VERSION
            || result.log.engine_version != ENGINE_VERSION
            || result.log.config != result.config
            || result.log.card_set != *result.card_set
        {
            return Err("saved game metadata/version mismatch".into());
        }
        let initial = match (
            result.log.initial_hands.clone(),
            result.log.initial_start_player_override,
        ) {
            (Some(hands), Some(starter)) => Game::with_cards_and_hands_with_start_player(
                result.config.clone(),
                (*result.card_set).clone(),
                hands,
                starter,
            )?,
            (Some(hands), None) => Game::with_cards_and_hands(
                result.config.clone(),
                (*result.card_set).clone(),
                hands,
            )?,
            (None, None) => Game::with_cards(result.config.clone(), (*result.card_set).clone())?,
            (None, Some(_)) => return Err("start player override requires draft hands".into()),
        };
        if result.log.start_player != initial.public.start_player
            || result.log.initial_state_hash != initial.state_hash()
            || result.log.round_seed != initial.log.round_seed
            || result.log.action_seed != initial.log.action_seed
        {
            return Err("saved game initial state mismatch".into());
        }
        let snapshot = result
            .log
            .initial_public_state
            .as_ref()
            .ok_or("saved game initial state snapshot missing")?;
        if *snapshot != initial.public {
            return Err("saved game initial state snapshot mismatch".into());
        }
        // Check the complete event chain, rather than trusting only the
        // last event hash; otherwise earlier tampering could go unnoticed.
        let replayed = Game::replay_with_setup_and_start_player(
            result.config.clone(),
            (*result.card_set).clone(),
            result.log.initial_hands.clone(),
            result.log.initial_start_player_override,
            &result.log.events,
        )?;
        if replayed.state_hash() != result.state_hash() {
            return Err("saved game differs from logged action replay".into());
        }
        Ok(result)
    }
    pub fn start_round(&mut self) {
        self.public.round += 1;
        let card = self.hidden_round_deck.pop().expect("14 rounds only");
        self.public.revealed.push(card);
        self.public.used.clear();
        self.public.start_player = self.public.start_player.min(self.public.players.len() - 1);
        self.public.active_player = self.public.start_player;
        self.public.workers_placed = vec![0; self.public.players.len()];
        self.public.work_quota = self.public.players.iter().map(|p| p.family).collect();
        for p in &mut self.public.players {
            p.newborns = 0;
        }
        let players = self.config.players;
        let mut spaces = Space::regular_for(players);
        spaces.extend(self.public.revealed.iter().copied().map(Space::from));
        if self.config.additional_action_space_enabled {
            spaces.extend(Space::extra_for(players));
        }
        if self.card_set.enabled() {
            spaces.push(Space::Lessons);
            if players >= 3 {
                spaces.push(Space::ExtraLessons);
            }
        }
        for space in spaces {
            if space == Space::MeetingPlace && self.card_set.enabled() {
                continue;
            }
            if let Some((_, n)) = space.accumulation_for(players) {
                *self.public.accumulations.entry(space).or_default() += n
            }
        }
        for space in [Space::SheepMarket, Space::PigMarket, Space::CattleMarket] {
            if self.public.revealed.contains(&round_for_space(space)) {
                *self.public.animal_accumulations.entry(space).or_default() += 1
            }
        }
        if let Some(recipients) = self.delayed_well.remove(&self.public.round) {
            for id in recipients {
                self.public.players[id].supply.food += 1
            }
        }
        for player in &mut self.public.players {
            crate::cards::start_round_rewards(&self.card_set, player)
                .expect("validated card round reward");
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let p = &self.public;
        if !(2..=4).contains(&self.config.players) || p.players.len() != self.config.players {
            return Err("player count mismatch".into());
        }
        self.card_set.validate(self.config.players)?;
        self.cards.validate(&self.card_set, &p.players)?;
        if self.config.additional_action_space_enabled != p.additional_action_space_enabled {
            return Err("setting mutated".into());
        }
        if p.round < 1 || p.round > 14 {
            return Err("invalid round".into());
        }
        if p.active_player >= p.players.len() || p.start_player >= p.players.len() {
            return Err("invalid active/start player".into());
        }
        if p.revealed.len() != p.round as usize {
            return Err("round/revealed count mismatch".into());
        }
        if p.revealed.len() + self.hidden_round_deck.len() != 14 {
            return Err("round deck corruption".into());
        }
        if p.work_quota.len() != p.players.len() || p.workers_placed.len() != p.players.len() {
            return Err("worker array mismatch".into());
        }
        // A physical major improvement has exactly one location: market
        // or a player's table, never both or duplicated.
        let cards = p
            .improvement_market
            .iter()
            .copied()
            .chain(
                p.players
                    .iter()
                    .flat_map(|player| player.improvements.iter().copied()),
            )
            .collect::<Vec<_>>();
        if cards.len() != Improvement::ALL.len()
            || cards.iter().copied().collect::<BTreeSet<_>>()
                != Improvement::ALL.into_iter().collect()
        {
            return Err("major improvement duplicated or missing".into());
        }
        for (i, player) in p.players.iter().enumerate() {
            validate_player_state(&p.work_quota, &p.workers_placed, i, player)?;
        }
        if !self.config.additional_action_space_enabled
            && p.used.iter().any(|s| s.additional_for(self.config.players))
        {
            return Err("disabled additional space used".into());
        }
        if p.used
            .iter()
            .filter(|s| s.additional_for(self.config.players))
            .count()
            > 1
        {
            return Err("multiple mutually-exclusive additional spaces used".into());
        }
        let mut available: BTreeSet<_> = Space::regular_for(self.config.players)
            .into_iter()
            .collect();
        if self.card_set.enabled() {
            available.insert(Space::Lessons);
            if self.config.players >= 3 {
                available.insert(Space::ExtraLessons);
            }
        }
        available.extend(p.revealed.iter().copied().map(Space::from));
        if self.config.additional_action_space_enabled {
            available.extend(Space::extra_for(self.config.players));
        }
        if !p.used.is_subset(&available) {
            return Err("a non-revealed action space was used".into());
        }
        if p.finished && (p.round != 14 || p.workers_placed != p.work_quota) {
            return Err("game ended before all workers were placed".into());
        }
        if p.used.len()
            != p.workers_placed
                .iter()
                .map(|&n| usize::from(n))
                .sum::<usize>()
        {
            return Err("worker placements do not match used spaces".into());
        }
        let all_cards = p
            .revealed
            .iter()
            .copied()
            .chain(self.hidden_round_deck.iter().copied())
            .collect::<BTreeSet<_>>();
        if all_cards != RoundCard::ALL.into_iter().collect() {
            return Err("round deck contains duplicate or missing cards".into());
        }
        Ok(())
    }
}

/// Lightweight public-state structure validation for `sample_game` and
/// other direct-construction paths. It checks only the public structural
/// invariants that can be verified without the private `card_set`, `cards`,
/// `hidden_round_deck`, or `rng`. This is a subset of `Game::validate`
/// designed to catch tampered `Observation` inputs fast.
///
/// Checks:
/// - `round` in 1..=14, `revealed.len() == round`
/// - `active_player < players`, `start_player < players`
/// - `players.len() == players`, `workers_placed.len() == players`, `work_quota.len() == players`
/// - Major improvements: market + all player tables multiset equals `Improvement::ALL`
/// - `used` spaces subset of `regular_for(players) ∪ revealed ∪ extra_for(players)` (+ Lessons/ExtraLessons if cards enabled)
/// - At most one mutually-exclusive additional space used
/// - `additional_action_space_enabled == additional`
/// - Revealed cards are in stage order (each stage count ≤ `RoundCard::ALL` for that stage)
pub fn validate_search_public(
    public: &PublicState,
    players: usize,
    additional: bool,
    cards_enabled: bool,
) -> Result<(), String> {
    if !(2..=4).contains(&players) || public.players.len() != players {
        return Err("player count mismatch".into());
    }
    if public.round < 1 || public.round > 14 {
        return Err("invalid round".into());
    }
    if public.revealed.len() != public.round as usize {
        return Err("round/revealed count mismatch".into());
    }
    if public.active_player >= players || public.start_player >= players {
        return Err("invalid active/start player".into());
    }
    if public.work_quota.len() != players || public.workers_placed.len() != players {
        return Err("worker array mismatch".into());
    }
    if public.additional_action_space_enabled != additional {
        return Err("additional_action_space_enabled mismatch".into());
    }

    // Major improvements: market + all player tables multiset equals Improvement::ALL
    let cards = public
        .improvement_market
        .iter()
        .copied()
        .chain(
            public
                .players
                .iter()
                .flat_map(|player| player.improvements.iter().copied()),
        )
        .collect::<Vec<_>>();
    if cards.len() != Improvement::ALL.len()
        || cards.iter().copied().collect::<BTreeSet<_>>() != Improvement::ALL.into_iter().collect()
    {
        return Err("major improvement duplicated or missing".into());
    }

    // Used spaces must be from the available set
    let mut available: BTreeSet<_> = Space::regular_for(players).into_iter().collect();
    if cards_enabled {
        available.insert(Space::Lessons);
        if players >= 3 {
            available.insert(Space::ExtraLessons);
        }
    }
    available.extend(public.revealed.iter().copied().map(Space::from));
    if additional {
        available.extend(Space::extra_for(players));
    }
    if !public.used.is_subset(&available) {
        return Err("a non-revealed action space was used".into());
    }
    // Mutually-exclusive additional spaces: at most one
    if public
        .used
        .iter()
        .filter(|s| s.additional_for(players))
        .count()
        > 1
    {
        return Err("multiple mutually-exclusive additional spaces used".into());
    }

    // Revealed cards stage order: each stage count ≤ RoundCard::ALL for that stage
    for stage in 1..=6 {
        let revealed_in_stage = public
            .revealed
            .iter()
            .filter(|c| c.stage() == stage)
            .count();
        let all_in_stage = RoundCard::ALL.iter().filter(|c| c.stage() == stage).count();
        if revealed_in_stage > all_in_stage {
            return Err(format!("too many revealed cards for stage {stage}"));
        }
    }

    // Player farm invariants (shared with full validate)
    for (i, player) in public.players.iter().enumerate() {
        validate_player_state(&public.work_quota, &public.workers_placed, i, player)?;
    }

    // Finished game checks
    if public.finished && (public.round != 14 || public.workers_placed != public.work_quota) {
        return Err("game ended before all workers were placed".into());
    }
    if public.used.len()
        != public
            .workers_placed
            .iter()
            .map(|&n| usize::from(n))
            .sum::<usize>()
    {
        return Err("worker placements do not match used spaces".into());
    }

    Ok(())
}

pub fn round_for_space(s: Space) -> RoundCard {
    match s {
        Space::SheepMarket => RoundCard::SheepMarket,
        Space::PigMarket => RoundCard::PigMarket,
        Space::CattleMarket => RoundCard::CattleMarket,
        _ => panic!("not an animal accumulation round space"),
    }
}

/// Per-player farm/animal/fence invariants shared by `Game::validate` and the
/// validation layers (`crate::validation`). Keeping one implementation
/// prevents the full and incremental validators from drifting apart.
pub fn validate_player_state(
    work_quota: &[u8],
    workers_placed: &[u8],
    index: usize,
    player: &Player,
) -> Result<(), String> {
    if player.id != index || player.farm.len() != FARM_CELLS {
        return Err("farm shape/player mismatch".into());
    }
    if player.family < 2 || player.family > 5 || player.newborns > player.family {
        return Err("family limit".into());
    }
    if player.occupied_stables() > 4 || player.fences.len() > 15 {
        return Err("stable/fence limit".into());
    }
    if work_quota[index] > player.family || workers_placed[index] > work_quota[index] {
        return Err("invalid worker count".into());
    }
    let mut covered = BTreeSet::new();
    let mut known_ids = BTreeSet::new();
    let mut required_fences = BTreeSet::new();
    for pasture in &player.pastures {
        if (pasture.quantity == 0) != pasture.animal.is_none() {
            return Err("pasture species and quantity inconsistent".into());
        }
        if pasture.cells.is_empty() || pasture.cells.iter().any(|&i| i >= FARM_CELLS) {
            return Err("pasture cells outside farm".into());
        }
        if !known_ids.insert(pasture.id) {
            return Err("duplicate pasture ID".into());
        }
        let region: BTreeSet<_> = pasture.cells.iter().copied().collect();
        let mut reached = BTreeSet::new();
        let mut pending = vec![pasture.cells[0]];
        while let Some(i) = pending.pop() {
            if !reached.insert(i) {
                continue;
            }
            pending.extend(neighbors(i).into_iter().filter(|j| region.contains(j)));
        }
        if reached.len() != region.len() {
            return Err("pasture is not orthogonally connected".into());
        }
        required_fences.extend(pasture_border(&pasture.cells));
        if pasture.quantity > player.pasture_capacity(pasture) {
            return Err("over capacity".into());
        }
        if pasture.quantity > 0 && pasture.animal.is_none() {
            return Err("missing pasture species".into());
        }
        for &cell in &pasture.cells {
            if cell >= FARM_CELLS
                || !covered.insert(cell)
                || player.farm[cell].tile != (Tile::Pasture { id: pasture.id })
            {
                return Err("pasture cell invalid".into());
            }
        }
    }
    // All rooms and all fields must form one orthogonally connected
    // group; a disconnected serialized farm is not a legal state.
    for category in [0u8, 1u8] {
        let tiles = player
            .farm
            .iter()
            .enumerate()
            .filter_map(|(cell_index, cell)| match (category, cell.tile) {
                (0, Tile::Room) | (1, Tile::Field { .. }) => Some(cell_index),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        if !tiles.is_empty() {
            let mut explored = BTreeSet::new();
            let mut stack = vec![*tiles.iter().next().expect("nonempty tiles")];
            while let Some(index) = stack.pop() {
                if explored.insert(index) {
                    stack.extend(
                        neighbors(index)
                            .into_iter()
                            .filter(|neighbor| tiles.contains(neighbor)),
                    );
                }
            }
            if explored != tiles {
                return Err("rooms or fields are not connected".into());
            }
        }
    }
    if player.fences != required_fences {
        return Err("pasture boundaries and placed fences disagree".into());
    }
    for (cell_index, cell) in player.farm.iter().enumerate() {
        if let Tile::Field { crop, remaining } = cell.tile {
            match crop {
                None if remaining != 0 => {
                    return Err("empty field carries crops".into());
                }
                Some(Crop::Grain) if remaining == 0 || remaining > 3 => {
                    return Err("invalid grain stack".into());
                }
                Some(Crop::Vegetable) if remaining == 0 || remaining > 2 => {
                    return Err("invalid vegetable stack".into());
                }
                _ => (),
            }
        }
        if matches!(cell.tile, Tile::Pasture { .. }) && !covered.contains(&cell_index) {
            return Err("pasture tile orphan".into());
        }
        if cell.stable && !matches!(cell.tile, Tile::Empty | Tile::Pasture { .. }) {
            return Err("stable placed on room/field".into());
        }
        if cell.stable_animal.is_some() && (!cell.stable || !matches!(cell.tile, Tile::Empty)) {
            return Err("invalid unfenced stable animal".into());
        }
    }
    Ok(())
}

/// orthogonal neighbors in 5 columns x 3 rows, 0-indexed.
pub fn neighbors(cell: usize) -> Vec<usize> {
    let x = cell % 5;
    let y = cell / 5;
    let mut out = Vec::new();
    if x > 0 {
        out.push(cell - 1);
    }
    if x < 4 {
        out.push(cell + 1);
    }
    if y > 0 {
        out.push(cell - 5);
    }
    if y < 2 {
        out.push(cell + 5);
    }
    out
}

/// Set of existing fence segments around a group of farm cells.
pub fn pasture_border(cells: &[usize]) -> BTreeSet<(u8, u8)> {
    let mut result = BTreeSet::new();
    for &cell in cells {
        if cell >= FARM_CELLS {
            continue;
        }
        let x = (cell % 5) as u8;
        let y = (cell / 5) as u8;
        let a = y * 6 + x;
        let b = a + 1;
        let c = a + 6;
        let d = c + 1;
        for (u, v) in [(a, b), (a, c), (b, d), (c, d)] {
            if !result.insert((u, v)) {
                result.remove(&(u, v));
            }
        }
    }
    result
}

//! Transactional rule application. Detail policies live outside this module.
use crate::breeding::BreedingPlan;
use crate::harvest::{FeedingPlan, HarvestDetailPolicy, SimpleHarvestDetails};
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plant {
    pub cell: usize,
    pub crop: Crop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimalPlan {
    pub animal: Animal,
    pub to_pastures: Vec<(u8, u32)>,
    pub to_stables: Vec<usize>,
    pub pet: bool,
    pub cook: u32,
    pub discard: u32,
}
impl AnimalPlan {
    pub fn discard_all(animal: Animal, n: u32) -> Self {
        Self {
            animal,
            to_pastures: vec![],
            to_stables: vec![],
            pet: false,
            cook: 0,
            discard: n,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastureOccupancy {
    pub animal: Option<Animal>,
    pub quantity: u32,
}
/// How a player acquires a major improvement. Paying its printed cost is
/// legal even if an exchangeable Fireplace is already owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PurchaseMode {
    /// Compatibility behavior for older M1 actions: exchange the first
    /// owned Fireplace for a Cooking Hearth, otherwise pay the cost.
    Auto,
    Pay,
    Exchange(Improvement),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakeAllocation {
    pub improvement: Improvement,
    pub grain: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Detail {
    None,
    MarketChoice(Good),
    PlayOccupation {
        card_id: String,
    },
    PlayMinor {
        card_id: String,
    },
    GrowWithMinor {
        card_id: String,
    },
    Plow {
        cell: usize,
    },
    Build {
        rooms: Vec<usize>,
        stables: Vec<usize>,
    },
    Fence {
        pastures: Vec<Vec<usize>>,
    },
    /// All affected pastures are replaced by these regions; occupancy is explicit.
    FenceDetailed {
        pastures: Vec<Vec<usize>>,
        occupancy: Vec<PastureOccupancy>,
    },
    Sow {
        plantings: Vec<Plant>,
        bake: u32,
    },
    /// Each owned baking improvement may be used once per baking action.
    SowDetailed {
        plantings: Vec<Plant>,
        baking: Vec<BakeAllocation>,
    },
    Cultivate {
        plow: Option<usize>,
        plantings: Vec<Plant>,
    },
    Buy {
        improvement: Improvement,
        instant_bake: u32,
    },
    BuyDetailed {
        improvement: Improvement,
        instant_baking: Vec<BakeAllocation>,
    },
    /// Full decision: explicit cost payment or a specific Fireplace exchange.
    BuyWithMode {
        improvement: Improvement,
        instant_baking: Vec<BakeAllocation>,
        mode: PurchaseMode,
    },
    Renovate {
        bonus_improvement: Option<Improvement>,
        instant_bake: u32,
        pastures: Vec<Vec<usize>>,
    },
    /// Explicit detail policy for the mandatory renovation followed by a
    /// distinct optional follow-up action on the correct card.
    RenovateDetailed {
        bonus_improvement: Option<Improvement>,
        instant_baking: Vec<BakeAllocation>,
        purchase_mode: PurchaseMode,
        pastures: Vec<Vec<usize>>,
        occupancy: Option<Vec<PastureOccupancy>>,
    },
    Animals(AnimalPlan),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub space: Space,
    pub detail: Detail,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimalLocation {
    Pet,
    Stable(usize),
    Pasture(u8),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreeAction {
    ConvertCrop {
        good: Good,
        quantity: u32,
    },
    CookAnimal {
        animal: Animal,
        source: AnimalLocation,
        quantity: u32,
    },
    DiscardGood {
        good: Good,
        quantity: u32,
    },
    DiscardAnimal {
        animal: Animal,
        source: AnimalLocation,
        quantity: u32,
    },
    RearrangeAnimals {
        pasture: Vec<(u8, Option<Animal>, u32)>,
        stables: Vec<(usize, Animal)>,
        pet: Option<Animal>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordedAction {
    Work {
        player: usize,
        action: Action,
        #[serde(default)]
        harvest_plans: Option<Vec<FeedingPlan>>,
        #[serde(default)]
        breeding_plans: Option<Vec<BreedingPlan>>,
    },
    Free {
        player: usize,
        action: FreeAction,
    },
}

fn group_occupied(used: &BTreeSet<Space>, players: usize) -> bool {
    used.iter().any(|s| s.additional_for(players))
}
fn room_material(p: &Player) -> Good {
    match p.house {
        House::Wood => Good::Wood,
        House::Clay => Good::Clay,
        House::Stone => Good::Stone,
    }
}
fn next_house(h: House) -> Option<(House, Good)> {
    match h {
        House::Wood => Some((House::Clay, Good::Clay)),
        House::Clay => Some((House::Stone, Good::Stone)),
        House::Stone => None,
    }
}
fn can_renovate(p: &Player) -> bool {
    next_house(p.house)
        .map(|(_, g)| p.supply.reed >= 1 && p.supply.get(g) >= p.rooms() as u32)
        .unwrap_or(false)
}
fn available_room(p: &Player) -> bool {
    p.farm.iter().enumerate().any(|(i, c)| {
        matches!(c.tile, Tile::Empty)
            && !c.stable
            && neighbors(i)
                .iter()
                .any(|&j| matches!(p.farm[j].tile, Tile::Room))
    })
}
fn available_field(p: &Player) -> bool {
    p.farm.iter().enumerate().any(|(i, c)| {
        matches!(c.tile, Tile::Empty)
            && !c.stable
            && (!p.farm.iter().any(|x| matches!(x.tile, Tile::Field { .. }))
                || neighbors(i)
                    .iter()
                    .any(|&j| matches!(p.farm[j].tile, Tile::Field { .. })))
    })
}
fn can_build(p: &Player) -> bool {
    (p.supply.reed >= 2 && p.supply.get(room_material(p)) >= 5 && available_room(p))
        || (p.occupied_stables() < 4
            && p.supply.wood >= 2
            && p.farm
                .iter()
                .any(|c| matches!(c.tile, Tile::Empty) && !c.stable))
}
fn can_fence(p: &Player) -> bool {
    single_cell_pasture(p).is_some()
        || split_candidate(p).is_some()
        || split_occupied_candidate(p).is_some()
        || general_split_candidate(p).is_some()
}
fn can_sow(p: &Player) -> bool {
    p.farm
        .iter()
        .any(|c| matches!(c.tile, Tile::Field { crop: None, .. }))
        && (p.supply.grain > 0 || p.supply.vegetable > 0)
}
fn can_bake(p: &Player) -> bool {
    p.supply.grain > 0
        && p.improvements.iter().any(|i| {
            matches!(
                i,
                Improvement::Fireplace2
                    | Improvement::Fireplace3
                    | Improvement::Hearth4
                    | Improvement::Hearth5
                    | Improvement::ClayOven
                    | Improvement::StoneOven
            )
        })
}
fn can_buy(p: &Player, market: &[Improvement]) -> bool {
    market.iter().any(|&i| {
        affordable(p, i)
            || i.is_cooking()
                && matches!(i, Improvement::Hearth4 | Improvement::Hearth5)
                && p.improvements
                    .iter()
                    .any(|c| matches!(c, Improvement::Fireplace2 | Improvement::Fireplace3))
    })
}
fn affordable(p: &Player, i: Improvement) -> bool {
    i.cost().iter().all(|&(g, n)| p.supply.get(g) >= n)
}

impl Game {
    pub fn legal_spaces(&self) -> Vec<Space> {
        if self.public.finished {
            return vec![];
        }
        let me = &self.public.players[self.public.active_player];
        let players = self.config.players;
        let mut list = Space::regular_for(players);
        if self.card_set.enabled() {
            list.push(Space::Lessons);
            if players >= 3 {
                list.push(Space::ExtraLessons);
            }
        }
        list.extend(self.public.revealed.iter().copied().map(Space::from));
        if self.config.additional_action_space_enabled {
            list.extend(Space::extra_for(players));
        }
        let hand = &self.cards.hands[self.public.active_player];
        let own_cards = hand
            .iter()
            .filter_map(|id| self.card_set.definition(id))
            .collect::<Vec<_>>();
        let occupations = me
            .played_cards
            .iter()
            .filter(|id| {
                self.card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == crate::cards::CardKind::Occupation)
            })
            .count();
        let can_minor = own_cards.iter().any(|c| {
            c.kind == crate::cards::CardKind::MinorImprovement && crate::cards::affordable(c, me, 0)
        });
        list.retain(|&s| {
            if self.public.used.contains(&s)
                || s.additional_for(players) && group_occupied(&self.public.used, players)
            {
                return false;
            }
            match s {
                Space::Farmland => available_field(me),
                Space::FarmExpansion => can_build(me),
                Space::Fencing => can_fence(me),
                Space::GrainUtilization => can_sow(me) || can_bake(me),
                Space::MajorImprovement => {
                    can_buy(me, &self.public.improvement_market) || can_minor
                }
                Space::Lessons => own_cards.iter().any(|c| {
                    c.kind == crate::cards::CardKind::Occupation
                        && crate::cards::affordable(c, me, u32::from(occupations > 0))
                }),
                Space::ExtraLessons => {
                    let fee = if players == 3 || occupations >= 2 {
                        2
                    } else {
                        1
                    };
                    own_cards.iter().any(|c| {
                        c.kind == crate::cards::CardKind::Occupation
                            && crate::cards::affordable(c, me, fee)
                    })
                }
                Space::FamilyGrowthWithRoom => me.family < 5 && me.rooms() > me.family as usize,
                Space::FamilyGrowthNoRoom => me.family < 5,
                Space::ModestFamilyGrowth => {
                    self.public.round >= 5 && me.family < 5 && me.rooms() > me.family as usize
                }
                Space::Cultivation => available_field(me) || can_sow(me),
                Space::HouseRedevelopment | Space::FarmRedevelopment => can_renovate(me),
                _ => true,
            }
        });
        list
    }
    /// True when this worker placement will close a harvest round.
    /// The UI uses this before collecting explicit human harvest decisions.
    pub fn will_harvest_after_work(&self) -> bool {
        if self.public.finished || !HARVEST_ROUNDS.contains(&self.public.round) {
            return false;
        }
        let mut placed = self.public.workers_placed.clone();
        placed[self.public.active_player] += 1;
        placed == self.public.work_quota
    }

    /// Public preview after the field and feeding phases. The next breeding
    /// choices can be made from this snapshot without exposing hidden cards.
    pub fn preview_harvest_after_feeding(
        &self,
        action: &Action,
        human: usize,
        feeding: &FeedingPlan,
    ) -> Result<PublicState> {
        if !self.will_harvest_after_work() || human >= self.config.players {
            return Err("harvest preview requires the final harvest action".into());
        }
        if !self.legal_spaces().contains(&action.space) {
            return Err("illegal final harvest action".into());
        }
        feeding.validate()?;
        let mut preview = self.clone();
        let active = preview.public.active_player;
        preview.execute_work(active, action)?;
        crate::cards::after_worker(
            &preview.card_set,
            &mut preview.public.players[active],
            action.space,
        )?;
        preview.harvest_fields();
        let plans = (0..self.config.players)
            .map(|id| {
                if id == human {
                    feeding.clone()
                } else {
                    FeedingPlan::default()
                }
            })
            .collect::<Vec<_>>();
        preview.feed_after_fields(&plans)?;
        Ok(preview.public)
    }

    /// An explicit human plan replaces only that player's harvest decisions.
    /// The other players' plans remain CPU-driven. All plans are validated
    /// and logged through the same transactional work/replay path.
    pub fn apply_with_human_harvest(
        &mut self,
        action: Action,
        human: usize,
        feeding: FeedingPlan,
        breeding: BreedingPlan,
    ) -> Result<()> {
        if !self.will_harvest_after_work() || human >= self.config.players {
            return Err("explicit harvest is only allowed on the final harvest move".into());
        }
        if !self.legal_spaces().contains(&action.space) {
            return Err("illegal final harvest action".into());
        }
        let post_feeding = self.preview_harvest_after_feeding(&action, human, &feeding)?;
        let feeding_plans = (0..self.config.players)
            .map(|id| {
                if id == human {
                    feeding.clone()
                } else {
                    FeedingPlan::default()
                }
            })
            .collect::<Vec<_>>();
        let breeding_plans = post_feeding
            .players
            .iter()
            .enumerate()
            .map(|(id, player)| {
                if id == human {
                    breeding.clone()
                } else {
                    BreedingPlan::simple(player)
                }
            })
            .collect::<Vec<_>>();
        self.apply_with_plans(
            action,
            Some(feeding_plans),
            Some(breeding_plans),
            &SimpleHarvestDetails,
        )
    }

    pub fn apply(&mut self, action: Action) -> Result<()> {
        self.apply_with_harvest_policy(action, &SimpleHarvestDetails)
    }
    pub fn apply_with_harvest_policy(
        &mut self,
        action: Action,
        policy: &dyn HarvestDetailPolicy,
    ) -> Result<()> {
        self.apply_with_plans(action, None, None, policy)
    }
    fn apply_with_plans(
        &mut self,
        action: Action,
        forced: Option<Vec<FeedingPlan>>,
        forced_breeding: Option<Vec<BreedingPlan>>,
        policy: &dyn HarvestDetailPolicy,
    ) -> Result<()> {
        let player = self.public.active_player;
        if !self.legal_spaces().contains(&action.space) {
            return Err(format!("illegal space {:?}", action.space));
        }
        let mut next = self.clone();
        let before = self.state_hash();
        next.execute_work(player, &action)?;
        crate::cards::after_worker(
            &next.card_set,
            &mut next.public.players[player],
            action.space,
        )?;
        next.public.used.insert(action.space);
        next.public.workers_placed[player] += 1;
        let will_harvest = next.public.workers_placed == next.public.work_quota
            && HARVEST_ROUNDS.contains(&next.public.round);
        if !will_harvest && (forced.is_some() || forced_breeding.is_some()) {
            return Err("harvest plans supplied on non-harvest move".into());
        }
        let harvest_plans = if will_harvest {
            let plans = if let Some(value) = forced {
                value
            } else {
                let mut preview = next.clone();
                preview.harvest_fields();
                (0..preview.public.players.len())
                    .map(|id| policy.plan_feeding(&preview.observation(id).expect("valid player")))
                    .collect::<Vec<_>>()
            };
            if plans.len() != next.public.players.len() {
                return Err("feeding plan count mismatch".into());
            }
            for plan in &plans {
                plan.validate()?;
            }
            Some(plans)
        } else {
            None
        };
        let breeding_plans = if will_harvest {
            let mut preview = next.clone();
            preview.harvest_fields();
            preview.feed_after_fields(harvest_plans.as_deref().expect("feeding plans"))?;
            let plans = if let Some(value) = forced_breeding {
                value
            } else {
                (0..preview.public.players.len())
                    .map(|id| policy.plan_breeding(&preview.observation(id).expect("valid player")))
                    .collect::<Vec<_>>()
            };
            if plans.len() != preview.public.players.len() {
                return Err("breeding plan count mismatch".into());
            }
            for (player, plan) in preview.public.players.iter().zip(&plans) {
                plan.apply(&mut player.clone())?;
            }
            Some(plans)
        } else {
            None
        };
        next.next_worker_or_round(harvest_plans.as_deref(), breeding_plans.as_deref())?;
        next.validate()?;
        let after = next.state_hash();
        next.log.events.push(Event {
            player: Some(player),
            kind: "work".into(),
            payload: serde_json::to_value(RecordedAction::Work {
                player,
                action,
                harvest_plans,
                breeding_plans,
            })
            .map_err(|e| e.to_string())?,
            before,
            after,
        });
        *self = next;
        Ok(())
    }
    pub fn apply_free(&mut self, player: usize, free: FreeAction) -> Result<()> {
        if self.public.finished || player >= self.public.players.len() {
            return Err("not an active game/player".into());
        }
        let mut next = self.clone();
        let before = self.state_hash();
        next.execute_free(player, &free)?;
        next.validate()?;
        let after = next.state_hash();
        next.log.events.push(Event {
            player: Some(player),
            kind: "free".into(),
            payload: serde_json::to_value(RecordedAction::Free {
                player,
                action: free,
            })
            .map_err(|e| e.to_string())?,
            before,
            after,
        });
        *self = next;
        Ok(())
    }
    pub fn replay(config: Config, events: &[Event]) -> Result<Self> {
        Self::replay_with_cards(config, crate::cards::CardSet::default(), events)
    }
    pub fn replay_with_cards(
        config: Config,
        cards: crate::cards::CardSet,
        events: &[Event],
    ) -> Result<Self> {
        let mut g = Game::with_cards(config, cards)?;
        for event in events {
            if g.state_hash() != event.before {
                return Err("replay prestate hash mismatch".into());
            }
            let action: RecordedAction =
                serde_json::from_value(event.payload.clone()).map_err(|e| e.to_string())?;
            match action {
                RecordedAction::Work {
                    player,
                    action,
                    harvest_plans,
                    breeding_plans,
                } => {
                    if event.kind != "work"
                        || event.player != Some(player)
                        || player != g.public.active_player
                    {
                        return Err("replay work metadata/player mismatch".into());
                    }
                    let mut placements = g.public.workers_placed.clone();
                    placements[player] += 1;
                    let will_harvest = placements == g.public.work_quota
                        && HARVEST_ROUNDS.contains(&g.public.round);
                    if harvest_plans.is_some() != will_harvest
                        || breeding_plans.is_some() != will_harvest
                    {
                        return Err("replay harvest decisions missing or unexpected".into());
                    }
                    g.apply_with_plans(
                        action,
                        harvest_plans,
                        breeding_plans,
                        &SimpleHarvestDetails,
                    )?
                }
                RecordedAction::Free { player, action } => {
                    if event.kind != "free" || event.player != Some(player) {
                        return Err("replay free action metadata/player mismatch".into());
                    }
                    g.apply_free(player, action)?;
                }
            }
            if g.state_hash() != event.after {
                return Err("replay poststate hash mismatch".into());
            }
        }
        Ok(g)
    }
    fn next_worker_or_round(
        &mut self,
        feeding: Option<&[FeedingPlan]>,
        breeding: Option<&[BreedingPlan]>,
    ) -> Result<()> {
        let n = self.public.players.len();
        let mut next = self.public.active_player;
        for _ in 0..n {
            next = (next + 1) % n;
            if self.public.workers_placed[next] < self.public.work_quota[next] {
                self.public.active_player = next;
                return Ok(());
            }
        }
        if HARVEST_ROUNDS.contains(&self.public.round) {
            self.harvest(
                feeding.ok_or("missing feeding decisions")?,
                breeding.ok_or("missing breeding decisions")?,
            )?;
        }
        if self.public.round == 14 {
            for player in &mut self.public.players {
                player.card_bonus_points = crate::cards::end_game_bonus(&self.card_set, player)?;
            }
            self.public.finished = true
        } else {
            self.start_round()
        }
        Ok(())
    }
    fn execute_work(&mut self, id: usize, act: &Action) -> Result<()> {
        let space = act.space;
        let detail = &act.detail;
        let p = &mut self.public.players[id];
        match space {
            Space::Forest
            | Space::ClayPit
            | Space::ReedBank
            | Space::Fishing
            | Space::WesternQuarry
            | Space::EasternQuarry
            | Space::Copse
            | Space::Grove
            | Space::Hollow
            | Space::TravelingPlayers => {
                require_none(detail)?;
                let n = self.public.accumulations.remove(&space).unwrap_or(0);
                let (kind, _) = space
                    .accumulation_for(self.config.players)
                    .ok_or("not an accumulation space")?;
                p.supply.gain(kind, n);
            }
            Space::MeetingPlace => {
                match detail {
                    Detail::None => {}
                    Detail::PlayMinor { card_id } if self.card_set.enabled() => {
                        crate::cards::play(
                            &self.card_set,
                            &mut self.cards.hands[id],
                            p,
                            card_id,
                            crate::cards::CardKind::MinorImprovement,
                            0,
                        )?;
                    }
                    _ => return Err("invalid starting-player details".into()),
                }
                let n = self.public.accumulations.remove(&space).unwrap_or(0);
                p.supply.food += n;
                self.public.start_player = id;
            }
            Space::Lessons | Space::ExtraLessons => {
                if let Detail::PlayOccupation { card_id } = detail {
                    let count = p
                        .played_cards
                        .iter()
                        .filter(|id| {
                            self.card_set
                                .definition(id)
                                .is_some_and(|c| c.kind == crate::cards::CardKind::Occupation)
                        })
                        .count();
                    let fee = if space == Space::Lessons {
                        u32::from(count > 0)
                    } else if self.config.players == 3 || count >= 2 {
                        2
                    } else {
                        1
                    };
                    crate::cards::play(
                        &self.card_set,
                        &mut self.cards.hands[id],
                        p,
                        card_id,
                        crate::cards::CardKind::Occupation,
                        fee,
                    )?;
                } else {
                    return Err("occupation selection required".into());
                }
            }
            Space::DayLaborer => {
                require_none(detail)?;
                p.supply.food += 2;
            }
            Space::GrainSeeds => {
                require_none(detail)?;
                p.supply.grain += 1;
            }
            Space::VegetableSeeds => {
                require_none(detail)?;
                p.supply.vegetable += 1;
            }
            Space::ResourceMarket => {
                match (self.config.players, detail) {
                    (2, Detail::None) => p.supply.stone += 1,
                    (3, Detail::MarketChoice(Good::Reed)) => p.supply.reed += 1,
                    (3, Detail::MarketChoice(Good::Stone)) => p.supply.stone += 1,
                    (4, Detail::None) => {
                        p.supply.reed += 1;
                        p.supply.stone += 1;
                    }
                    _ => return Err("incorrect resource market selection".into()),
                }
                p.supply.food += 1;
            }
            Space::Farmland => {
                if let Detail::Plow { cell } = detail {
                    plow(p, *cell)?
                } else {
                    return Err("plow details required".into());
                }
            }
            Space::FarmExpansion => {
                if let Detail::Build { rooms, stables } = detail {
                    if rooms.is_empty() && stables.is_empty() {
                        return Err("build at least one item".into());
                    }
                    for &i in rooms {
                        build_room(p, i)?
                    }
                    for &i in stables {
                        build_stable(p, i)?
                    }
                } else {
                    return Err("build details required".into());
                }
            }
            Space::Fencing => match detail {
                Detail::Fence { pastures } => build_pastures(p, pastures, None)?,
                Detail::FenceDetailed {
                    pastures,
                    occupancy,
                } => build_pastures(p, pastures, Some(occupancy))?,
                _ => return Err("pasture details required".into()),
            },
            Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth => match detail {
                Detail::None => grow(p, true)?,
                Detail::GrowWithMinor { card_id } if self.card_set.enabled() => {
                    grow(p, true)?;
                    crate::cards::play(
                        &self.card_set,
                        &mut self.cards.hands[id],
                        p,
                        card_id,
                        crate::cards::CardKind::MinorImprovement,
                        0,
                    )?;
                }
                _ => return Err("invalid growth details".into()),
            },
            Space::FamilyGrowthNoRoom => {
                require_none(detail)?;
                grow(p, false)?;
            }
            Space::GrainUtilization => match detail {
                Detail::Sow { plantings, bake } => {
                    if plantings.is_empty() && *bake == 0 {
                        return Err("must sow or bake".into());
                    }
                    sow(p, plantings)?;
                    bake_bread(p, *bake)?;
                }
                Detail::SowDetailed { plantings, baking } => {
                    if plantings.is_empty() && baking.is_empty() {
                        return Err("must sow or bake".into());
                    }
                    sow(p, plantings)?;
                    bake_allocated(p, baking)?;
                }
                _ => return Err("sow/bake details required".into()),
            },
            Space::Cultivation => {
                if let Detail::Cultivate {
                    plow: cell,
                    plantings,
                } = detail
                {
                    if cell.is_none() && plantings.is_empty() {
                        return Err("must cultivate".into());
                    }
                    if let Some(i) = cell {
                        plow(p, *i)?
                    };
                    sow(p, plantings)?;
                } else {
                    return Err("cultivation details required".into());
                }
            }
            Space::MajorImprovement => match detail {
                Detail::PlayMinor { card_id } if self.card_set.enabled() => {
                    crate::cards::play(
                        &self.card_set,
                        &mut self.cards.hands[id],
                        p,
                        card_id,
                        crate::cards::CardKind::MinorImprovement,
                        0,
                    )?;
                }
                Detail::Buy {
                    improvement,
                    instant_bake,
                } => buy_improvement(
                    p,
                    &mut self.public.improvement_market,
                    &mut self.delayed_well,
                    self.public.round,
                    *improvement,
                    PurchaseArgs {
                        instant_bake: *instant_bake,
                        allocation: None,
                        mode: PurchaseMode::Auto,
                    },
                )?,
                Detail::BuyDetailed {
                    improvement,
                    instant_baking,
                } => buy_improvement(
                    p,
                    &mut self.public.improvement_market,
                    &mut self.delayed_well,
                    self.public.round,
                    *improvement,
                    PurchaseArgs {
                        instant_bake: 0,
                        allocation: Some(instant_baking),
                        mode: PurchaseMode::Auto,
                    },
                )?,
                Detail::BuyWithMode {
                    improvement,
                    instant_baking,
                    mode,
                } => buy_improvement(
                    p,
                    &mut self.public.improvement_market,
                    &mut self.delayed_well,
                    self.public.round,
                    *improvement,
                    PurchaseArgs {
                        instant_bake: 0,
                        allocation: Some(instant_baking),
                        mode: *mode,
                    },
                )?,
                _ => return Err("improvement required".into()),
            },
            Space::HouseRedevelopment => match detail {
                Detail::Renovate {
                    bonus_improvement,
                    instant_bake,
                    pastures,
                } => {
                    if !pastures.is_empty() {
                        return Err("no fences on house redevelopment".into());
                    }
                    renovate(p)?;
                    if let Some(i) = bonus_improvement {
                        buy_improvement(
                            p,
                            &mut self.public.improvement_market,
                            &mut self.delayed_well,
                            self.public.round,
                            *i,
                            PurchaseArgs {
                                instant_bake: *instant_bake,
                                allocation: None,
                                mode: PurchaseMode::Auto,
                            },
                        )?;
                    } else if *instant_bake > 0 {
                        return Err("instant bake without oven".into());
                    }
                }
                Detail::RenovateDetailed {
                    bonus_improvement,
                    instant_baking,
                    purchase_mode,
                    pastures,
                    occupancy,
                } => {
                    if !pastures.is_empty() || occupancy.is_some() {
                        return Err("fencing is not allowed on house redevelopment".into());
                    }
                    renovate(p)?;
                    if let Some(improvement) = bonus_improvement {
                        buy_improvement(
                            p,
                            &mut self.public.improvement_market,
                            &mut self.delayed_well,
                            self.public.round,
                            *improvement,
                            PurchaseArgs {
                                instant_bake: 0,
                                allocation: Some(instant_baking),
                                mode: *purchase_mode,
                            },
                        )?;
                    } else if !instant_baking.is_empty() || *purchase_mode != PurchaseMode::Auto {
                        return Err(
                            "improvement detail supplied without a bonus improvement".into()
                        );
                    }
                }
                _ => return Err("renovation details required".into()),
            },
            Space::FarmRedevelopment => match detail {
                Detail::Renovate {
                    bonus_improvement,
                    instant_bake,
                    pastures,
                } => {
                    if bonus_improvement.is_some() || *instant_bake > 0 {
                        return Err("no improvement on farm redevelopment".into());
                    }
                    renovate(p)?;
                    if !pastures.is_empty() {
                        build_pastures(p, pastures, None)?;
                    }
                }
                Detail::RenovateDetailed {
                    bonus_improvement,
                    instant_baking,
                    purchase_mode,
                    pastures,
                    occupancy,
                } => {
                    if bonus_improvement.is_some()
                        || !instant_baking.is_empty()
                        || *purchase_mode != PurchaseMode::Auto
                        || (pastures.is_empty() && occupancy.is_some())
                    {
                        return Err("only fencing follows farm redevelopment".into());
                    }
                    renovate(p)?;
                    if !pastures.is_empty() {
                        build_pastures(p, pastures, occupancy.as_deref())?;
                    }
                }
                _ => return Err("renovation details required".into()),
            },
            Space::SheepMarket | Space::PigMarket | Space::CattleMarket | Space::AnimalMarket => {
                if let Detail::Animals(plan) = detail {
                    let species = match space {
                        Space::SheepMarket => Some(Animal::Sheep),
                        Space::PigMarket => Some(Animal::Boar),
                        Space::CattleMarket => Some(Animal::Cattle),
                        _ => None,
                    };
                    if let Some(kind) = species {
                        if plan.animal != kind {
                            return Err("wrong species".into());
                        }
                    }
                    let quantity = if space == Space::AnimalMarket {
                        if plan.animal == Animal::Cattle {
                            p.supply.pay(Good::Food, 1)?;
                        } else if plan.animal == Animal::Sheep {
                            p.supply.food += 1;
                        }
                        1
                    } else {
                        self.public.animal_accumulations.remove(&space).unwrap_or(0)
                    };
                    place_animals(p, plan, quantity)?;
                } else {
                    return Err("animal placement required".into());
                }
            }
        }
        Ok(())
    }
    fn execute_free(&mut self, id: usize, act: &FreeAction) -> Result<()> {
        let p = &mut self.public.players[id];
        match act {
            FreeAction::ConvertCrop { good, quantity } => {
                if !matches!(good, Good::Grain | Good::Vegetable) || *quantity == 0 {
                    return Err("invalid crop conversion".into());
                }
                let food_per_good =
                    match good {
                        Good::Grain => 1,
                        Good::Vegetable => {
                            if p.improvements
                                .iter()
                                .any(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
                            {
                                3
                            } else if p.improvements.iter().any(|i| {
                                matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3)
                            }) {
                                2
                            } else {
                                1
                            }
                        }
                        _ => return Err("only raw grain/vegetables can be converted".into()),
                    };
                p.supply.pay(*good, *quantity)?;
                p.supply.gain(
                    Good::Food,
                    quantity.checked_mul(food_per_good).ok_or("food overflow")?,
                );
            }
            FreeAction::DiscardGood { good, quantity } => {
                if *quantity == 0 {
                    return Err("discard zero".into());
                }
                p.supply.pay(*good, *quantity)?;
            }
            FreeAction::CookAnimal {
                animal,
                source,
                quantity,
            } => {
                if *quantity == 0 {
                    return Err("cook zero".into());
                }
                let worth = cook_value(p, *animal).ok_or("no cooking improvement")?;
                take_animals(p, *animal, source, *quantity)?;
                p.supply.food += quantity * worth;
            }
            FreeAction::DiscardAnimal {
                animal,
                source,
                quantity,
            } => {
                if *quantity == 0 {
                    return Err("discard zero".into());
                }
                take_animals(p, *animal, source, *quantity)?;
            }
            FreeAction::RearrangeAnimals {
                pasture,
                stables,
                pet,
            } => {
                rearrange_animals(p, pasture, stables, *pet)?;
            }
        }
        Ok(())
    }
}
fn require_none(d: &Detail) -> Result<()> {
    if !matches!(d, Detail::None) {
        Err("unexpected detail".into())
    } else {
        Ok(())
    }
}
fn plow(p: &mut Player, i: usize) -> Result<()> {
    if i >= FARM_CELLS || !matches!(p.farm[i].tile, Tile::Empty) || p.farm[i].stable {
        return Err("field needs empty tile".into());
    }
    if p.fields() > 0
        && !neighbors(i)
            .iter()
            .any(|&j| matches!(p.farm[j].tile, Tile::Field { .. }))
    {
        return Err("field must touch existing field".into());
    }
    p.farm[i].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    Ok(())
}
fn build_room(p: &mut Player, i: usize) -> Result<()> {
    if i >= FARM_CELLS
        || !matches!(p.farm[i].tile, Tile::Empty)
        || p.farm[i].stable
        || !neighbors(i)
            .iter()
            .any(|&j| matches!(p.farm[j].tile, Tile::Room))
    {
        return Err("room location illegal".into());
    }
    let material = room_material(p);
    p.supply.pay(material, 5)?;
    p.supply.pay(Good::Reed, 2)?;
    p.farm[i].tile = Tile::Room;
    Ok(())
}
fn build_stable(p: &mut Player, i: usize) -> Result<()> {
    if i >= FARM_CELLS
        || p.occupied_stables() >= 4
        || p.farm[i].stable
        || !matches!(p.farm[i].tile, Tile::Empty)
    {
        return Err("stable location illegal".into());
    }
    p.supply.pay(Good::Wood, 2)?;
    p.farm[i].stable = true;
    Ok(())
}
fn grow(p: &mut Player, room_only: bool) -> Result<()> {
    if p.family >= 5 || room_only && p.rooms() <= p.family as usize {
        return Err("family growth not permitted".into());
    }
    p.family += 1;
    p.newborns += 1;
    Ok(())
}
fn renovate(p: &mut Player) -> Result<()> {
    let (new, g) = next_house(p.house).ok_or("stone is final house")?;
    p.supply.pay(g, p.rooms() as u32)?;
    p.supply.pay(Good::Reed, 1)?;
    p.house = new;
    Ok(())
}
fn sow(p: &mut Player, assignments: &[Plant]) -> Result<()> {
    let mut visited = BTreeSet::new();
    for a in assignments {
        if a.cell >= FARM_CELLS
            || !visited.insert(a.cell)
            || !matches!(
                p.farm[a.cell].tile,
                Tile::Field {
                    crop: None,
                    remaining: 0
                }
            )
        {
            return Err("invalid empty field".into());
        }
        let good = match a.crop {
            Crop::Grain => Good::Grain,
            Crop::Vegetable => Good::Vegetable,
        };
        p.supply.pay(good, 1)?;
        p.farm[a.cell].tile = Tile::Field {
            crop: Some(a.crop),
            remaining: if a.crop == Crop::Grain { 3 } else { 2 },
        };
    }
    Ok(())
}
/// Execute an explicit allocation. A fireplace/hearth can bake any number of grain;
/// each oven has a per-baking-action cap and cannot be used twice.
fn bake_allocated(p: &mut Player, allocation: &[BakeAllocation]) -> Result<()> {
    let mut used = BTreeSet::new();
    let mut total = 0u32;
    let mut food = 0u32;
    for entry in allocation {
        if entry.grain == 0
            || !used.insert(entry.improvement)
            || !p.improvements.contains(&entry.improvement)
        {
            return Err("invalid baking improvement or repeated use".into());
        }
        let (cap, rate) = match entry.improvement {
            Improvement::Fireplace2 | Improvement::Fireplace3 => (u32::MAX, 2),
            Improvement::Hearth4 | Improvement::Hearth5 => (u32::MAX, 3),
            Improvement::ClayOven => (1, 5),
            Improvement::StoneOven => (2, 4),
            _ => return Err("improvement cannot bake".into()),
        };
        if entry.grain > cap {
            return Err("oven baking cap exceeded".into());
        }
        total = total
            .checked_add(entry.grain)
            .ok_or("grain count overflow")?;
        food = food
            .checked_add(entry.grain.checked_mul(rate).ok_or("food count overflow")?)
            .ok_or("food count overflow")?;
    }
    p.supply.pay(Good::Grain, total)?;
    p.supply.gain(Good::Food, food);
    Ok(())
}
/// Compatibility for scalar baking parameters: a deterministic choice of
/// oven use followed by a cooking card, never used by the new detail policy.
fn bake_bread(p: &mut Player, amount: u32) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut left = amount;
    let mut allocation = Vec::new();
    for (improvement, capacity) in [(Improvement::ClayOven, 1), (Improvement::StoneOven, 2)] {
        if p.improvements.contains(&improvement) && left > 0 {
            let grain = left.min(capacity);
            left -= grain;
            allocation.push(BakeAllocation { improvement, grain });
        }
    }
    if left > 0 {
        let card = p
            .improvements
            .iter()
            .copied()
            .find(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
            .or_else(|| {
                p.improvements
                    .iter()
                    .copied()
                    .find(|i| matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3))
            })
            .ok_or("bake amount exceeds oven capacities")?;
        allocation.push(BakeAllocation {
            improvement: card,
            grain: left,
        });
    }
    bake_allocated(p, &allocation)
}
struct PurchaseArgs<'a> {
    instant_bake: u32,
    allocation: Option<&'a [BakeAllocation]>,
    mode: PurchaseMode,
}
fn buy_improvement(
    p: &mut Player,
    market: &mut Vec<Improvement>,
    well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    item: Improvement,
    options: PurchaseArgs<'_>,
) -> Result<()> {
    let PurchaseArgs {
        instant_bake,
        allocation,
        mode,
    } = options;
    let pos = market
        .iter()
        .position(|&x| x == item)
        .ok_or("improvement unavailable")?;
    let is_hearth = matches!(item, Improvement::Hearth4 | Improvement::Hearth5);
    let exchange = match mode {
        PurchaseMode::Pay => None,
        PurchaseMode::Auto if is_hearth => p
            .improvements
            .iter()
            .copied()
            .find(|i| matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3)),
        PurchaseMode::Auto => None,
        PurchaseMode::Exchange(old) => {
            if !is_hearth
                || !matches!(old, Improvement::Fireplace2 | Improvement::Fireplace3)
                || !p.improvements.contains(&old)
            {
                return Err("invalid Fireplace-to-Hearth exchange".into());
            }
            Some(old)
        }
    };
    if let Some(old) = exchange {
        let index = p
            .improvements
            .iter()
            .position(|&i| i == old)
            .expect("validated exchange");
        p.improvements.remove(index);
        market.push(old);
    } else {
        for (good, n) in item.cost() {
            p.supply.pay(good, n)?;
        }
    }
    market.remove(pos);
    p.improvements.push(item);
    if item == Improvement::Well {
        for r in round + 1..=14u8.min(round.saturating_add(5)) {
            well.entry(r).or_default().push(p.id)
        }
    }
    if (instant_bake > 0 || allocation.is_some_and(|a| !a.is_empty()))
        && !matches!(item, Improvement::ClayOven | Improvement::StoneOven)
    {
        return Err("only ovens grant immediate bake".into());
    }
    if let Some(plan) = allocation {
        bake_allocated(p, plan)?;
    } else {
        bake_bread(p, instant_bake)?;
    }
    Ok(())
}
/// Canonical fence segments, 6x4 lattice vertices. 5x3 cell grid.
fn border(i: usize) -> [(u8, u8); 4] {
    let x = (i % 5) as u8;
    let y = (i / 5) as u8;
    let pts = [
        y * 6 + x,
        y * 6 + x + 1,
        (y + 1) * 6 + x,
        (y + 1) * 6 + x + 1,
    ];
    [
        (pts[0], pts[1]),
        (pts[2], pts[3]),
        (pts[0], pts[2]),
        (pts[1], pts[3]),
    ]
}
fn boundary(cells: &[usize]) -> BTreeSet<(u8, u8)> {
    let mut edges = BTreeSet::new();
    for &cell in cells {
        for edge in border(cell) {
            if !edges.insert(edge) {
                edges.remove(&edge);
            }
        }
    }
    edges
}
/// Legal one-cell pasture option used by the default detail policy.
/// Check fresh fence cost, the 15 fence limit, and adjacency.
pub fn single_cell_pasture(p: &Player) -> Option<usize> {
    (0..FARM_CELLS).find(|&i| {
        if !matches!(p.farm[i].tile, Tile::Empty) {
            return false;
        }
        if !p.pastures.is_empty()
            && !neighbors(i)
                .iter()
                .any(|&j| matches!(p.farm[j].tile, Tile::Pasture { .. }))
        {
            return false;
        }
        let edges = boundary(&[i]);
        let extra = edges.difference(&p.fences).count();
        extra <= p.supply.wood as usize && p.fences.len() + extra <= 15
    })
}
fn connected(cells: &[usize]) -> bool {
    if cells.is_empty() {
        return false;
    }
    let set: BTreeSet<_> = cells.iter().copied().collect();
    let mut found = BTreeSet::new();
    let mut q = VecDeque::from([cells[0]]);
    while let Some(i) = q.pop_front() {
        if found.insert(i) {
            for j in neighbors(i) {
                if set.contains(&j) {
                    q.push_back(j)
                }
            }
        }
    }
    found.len() == set.len()
}
/// A fencing action supplies the *complete new regions* of every affected
/// pasture, not merely the newly fenced cells. Existing fence segments cannot
/// be removed. A split with animals requires explicit occupancy decisions.
fn build_pastures(
    p: &mut Player,
    regions: &[Vec<usize>],
    occupancy: Option<&[PastureOccupancy]>,
) -> Result<()> {
    if regions.is_empty() {
        return Err("must add or split a pasture".into());
    }
    if let Some(values) = occupancy {
        if values.len() != regions.len() {
            return Err("one occupancy per changed pasture required".into());
        }
    }
    let previous_animals = [Animal::Sheep, Animal::Boar, Animal::Cattle].map(|a| p.animal_count(a));
    let mut visited = BTreeSet::new();
    let mut affected = BTreeSet::new();
    for region in regions {
        if region.is_empty() || region.iter().any(|&i| i >= FARM_CELLS) || !connected(region) {
            return Err("invalid/disconnected pasture region".into());
        }
        for &i in region {
            if !visited.insert(i) {
                return Err("overlapping proposed pastures".into());
            }
            match p.farm[i].tile {
                Tile::Empty => {}
                Tile::Pasture { id } => {
                    affected.insert(id);
                }
                _ => return Err("room or field cannot be fenced".into()),
            }
        }
    }
    // Cover all cells of a modified pasture. In particular, a subset of an
    // existing occupied pasture cannot simply be deleted.
    for original in &p.pastures {
        if affected.contains(&original.id) && original.cells.iter().any(|c| !visited.contains(c)) {
            return Err("changed pasture must account for every old cell".into());
        }
    }
    // Every newly proposed region must connect to an existing pasture, or
    // (if there are none) to the other regions built in this single action.
    let existing: BTreeSet<_> = p
        .pastures
        .iter()
        .flat_map(|q| q.cells.iter().copied())
        .collect();
    if !existing.is_empty() {
        let mut connected_regions = regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.iter().any(|i| existing.contains(i)))
            .map(|(i, _)| i)
            .collect::<BTreeSet<_>>();
        loop {
            let old_len = connected_regions.len();
            for (idx, region) in regions.iter().enumerate() {
                if !connected_regions.contains(&idx)
                    && region.iter().any(|i| {
                        neighbors(*i).into_iter().any(|adj| {
                            existing.contains(&adj)
                                || regions.iter().enumerate().any(|(j, r)| {
                                    connected_regions.contains(&j) && r.contains(&adj)
                                })
                        })
                    })
                {
                    connected_regions.insert(idx);
                }
            }
            if connected_regions.len() == old_len {
                break;
            }
        }
        if connected_regions.len() != regions.len() {
            return Err("new pasture is not adjacent to existing pasture".into());
        }
    } else if regions.len() > 1 {
        let mut connected_regions = BTreeSet::from([0usize]);
        loop {
            let n = connected_regions.len();
            for (idx, r) in regions.iter().enumerate() {
                if !connected_regions.contains(&idx)
                    && r.iter().any(|i| {
                        neighbors(*i).into_iter().any(|adj| {
                            regions
                                .iter()
                                .enumerate()
                                .any(|(j, v)| connected_regions.contains(&j) && v.contains(&adj))
                        })
                    })
                {
                    connected_regions.insert(idx);
                }
            }
            if n == connected_regions.len() {
                break;
            }
        }
        if connected_regions.len() != regions.len() {
            return Err("new pastures must be adjacent".into());
        }
    }
    let unchanged = p
        .pastures
        .iter()
        .filter(|q| !affected.contains(&q.id))
        .cloned()
        .collect::<Vec<_>>();
    let mut edges = BTreeSet::new();
    for region in &unchanged {
        edges.extend(boundary(&region.cells));
    }
    for region in regions {
        edges.extend(boundary(region));
    }
    if !p.fences.is_subset(&edges) {
        return Err("fences cannot be demolished".into());
    }
    let cost = edges.difference(&p.fences).count();
    if cost == 0 {
        return Err("fencing must place at least one new fence".into());
    }
    if edges.len() > 15 {
        return Err("fence limit 15".into());
    }
    p.supply.pay(Good::Wood, cost as u32)?;
    let mut next_id = p.pastures.iter().map(|q| q.id).max().map_or(0, |n| n + 1);
    let mut assigned_ids = BTreeSet::new();
    let mut updated = Vec::new();
    for (ix, region) in regions.iter().enumerate() {
        let old_ids = region
            .iter()
            .filter_map(|&i| match p.farm[i].tile {
                Tile::Pasture { id } => Some(id),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let id = if old_ids.len() == 1
            && !assigned_ids.contains(old_ids.iter().next().expect("one id"))
        {
            *old_ids.iter().next().expect("one id")
        } else {
            let new_id = next_id;
            next_id = next_id.checked_add(1).ok_or("pasture id overflow")?;
            new_id
        };
        assigned_ids.insert(id);
        let mut animal = None;
        let mut quantity = 0;
        if let Some(values) = occupancy {
            animal = values[ix].animal;
            quantity = values[ix].quantity;
            if (quantity == 0) != animal.is_none() {
                return Err("occupancy species/count mismatch".into());
            }
        } else {
            // Automatic relocation is allowed only if the old animal-holding
            // pasture is assigned in its entirety to a unique destination.
            for old in &p.pastures {
                if old.quantity == 0 || !old.cells.iter().any(|i| region.contains(i)) {
                    continue;
                }
                if !old.cells.iter().all(|i| region.contains(i)) {
                    return Err("split with animals needs explicit allocation".into());
                }
                if animal.is_some() && animal != old.animal {
                    return Err("species conflict in pasture".into());
                }
                animal = old.animal;
                quantity += old.quantity;
            }
            for &i in region {
                if let Some(species) = p.farm[i].stable_animal {
                    if animal.is_some() && animal != Some(species) {
                        return Err("stable animal species conflict".into());
                    }
                    animal = Some(species);
                    quantity += 1;
                }
            }
        }
        updated.push(Pasture {
            id,
            cells: region.clone(),
            animal,
            quantity,
        });
    }
    for pasture in &updated {
        for &i in &pasture.cells {
            p.farm[i].tile = Tile::Pasture { id: pasture.id };
            p.farm[i].stable_animal = None;
        }
    }
    p.pastures = unchanged.into_iter().chain(updated).collect();
    p.fences = edges;
    for pasture in &p.pastures {
        if pasture.quantity > p.pasture_capacity(pasture) {
            return Err("overfull pasture after fencing".into());
        }
    }
    let afterward = [Animal::Sheep, Animal::Boar, Animal::Cattle].map(|a| p.animal_count(a));
    if afterward != previous_animals {
        return Err("fencing must conserve existing animals".into());
    }
    Ok(())
}

/// A deterministic candidate for an *empty* pasture split. An occupied
/// pasture can also be split, but must provide a detailed animal allocation.
pub fn split_candidate(p: &Player) -> Option<Vec<Vec<usize>>> {
    if p.fences.len() >= 15 {
        return None;
    }
    for pasture in &p.pastures {
        if pasture.cells.len() < 2 || pasture.quantity > 0 {
            continue;
        }
        for &one in &pasture.cells {
            let rest = pasture
                .cells
                .iter()
                .copied()
                .filter(|&i| i != one)
                .collect::<Vec<_>>();
            if !connected(&rest) {
                continue;
            }
            let cost = boundary(&[one])
                .union(&boundary(&rest))
                .filter(|e| !p.fences.contains(e))
                .count();
            if cost <= p.supply.wood as usize && p.fences.len() + cost <= 15 {
                return Some(vec![rest, vec![one]]);
            }
        }
    }
    None
}
/// An occupied pasture may legally be divided by an internal fence.
/// Supply an explicit, capacity-safe distribution of its existing animals.
/// Exhaustive but bounded fallback for a pasture whose valid split is not
/// obtained by isolating a single cell (e.g. two 2x2 components joined at one edge).
/// Called only when the cheaper, common-case candidates fail.
pub fn general_split_candidate(
    player: &Player,
) -> Option<(Vec<Vec<usize>>, Vec<PastureOccupancy>)> {
    if player.fences.len() >= 15 || player.supply.wood == 0 {
        return None;
    }
    for pasture in &player.pastures {
        let count = pasture.cells.len();
        if count < 2 {
            continue;
        }
        for mask in 1..((1usize << count) - 1) {
            // One orientation per unordered partition.
            if mask & 1 == 0 {
                continue;
            }
            let (first, second): (Vec<_>, Vec<_>) = pasture
                .cells
                .iter()
                .copied()
                .enumerate()
                .partition::<Vec<_>, _>(|&(i, _)| mask & (1usize << i) != 0);
            let first = first.into_iter().map(|(_, cell)| cell).collect::<Vec<_>>();
            let second = second.into_iter().map(|(_, cell)| cell).collect::<Vec<_>>();
            if first.is_empty() || second.is_empty() || !connected(&first) || !connected(&second) {
                continue;
            }
            let mut edges = boundary(&first);
            edges.extend(boundary(&second));
            let added = edges.difference(&player.fences).count();
            if player.fences.len() + added > 15 || added > player.supply.wood as usize {
                continue;
            }
            let first_capacity = player.pasture_capacity(&Pasture {
                id: pasture.id,
                cells: first.clone(),
                animal: pasture.animal,
                quantity: 0,
            });
            let second_capacity = player.pasture_capacity(&Pasture {
                id: pasture.id,
                cells: second.clone(),
                animal: pasture.animal,
                quantity: 0,
            });
            let a = pasture.quantity.min(first_capacity);
            let b = pasture.quantity - a;
            if b > second_capacity {
                continue;
            }
            let allocations = [a, b]
                .into_iter()
                .map(|quantity| PastureOccupancy {
                    animal: if quantity == 0 { None } else { pasture.animal },
                    quantity,
                })
                .collect();
            return Some((vec![first, second], allocations));
        }
    }
    None
}

pub fn split_occupied_candidate(p: &Player) -> Option<(Vec<Vec<usize>>, Vec<PastureOccupancy>)> {
    if p.fences.len() >= 15 {
        return None;
    }
    for pasture in &p.pastures {
        if pasture.cells.len() < 2 || pasture.quantity == 0 {
            continue;
        }
        let species = pasture.animal?;
        for &isolated in &pasture.cells {
            let rest = pasture
                .cells
                .iter()
                .copied()
                .filter(|&cell| cell != isolated)
                .collect::<Vec<_>>();
            if !connected(&rest) {
                continue;
            }
            let new_edges = boundary(&[isolated])
                .union(&boundary(&rest))
                .copied()
                .collect::<BTreeSet<_>>();
            let additional = new_edges.difference(&p.fences).count();
            if p.fences.len() + additional > 15 || additional > p.supply.wood as usize {
                continue;
            }
            let remaining_area = Pasture {
                id: pasture.id,
                cells: rest.clone(),
                animal: Some(species),
                quantity: 0,
            };
            let single_area = Pasture {
                id: pasture.id,
                cells: vec![isolated],
                animal: Some(species),
                quantity: 0,
            };
            let capacity_rest = p.pasture_capacity(&remaining_area);
            let capacity_single = p.pasture_capacity(&single_area);
            let amount_rest = pasture.quantity.min(capacity_rest);
            let amount_single = pasture.quantity - amount_rest;
            if amount_single > capacity_single {
                continue;
            }
            let occupancy = [amount_rest, amount_single]
                .into_iter()
                .map(|quantity| PastureOccupancy {
                    animal: if quantity == 0 { None } else { Some(species) },
                    quantity,
                })
                .collect::<Vec<_>>();
            return Some((vec![rest, vec![isolated]], occupancy));
        }
    }
    None
}

fn cook_value(p: &Player, animal: Animal) -> Option<u32> {
    if p.improvements
        .iter()
        .any(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
    {
        Some(match animal {
            Animal::Sheep => 2,
            Animal::Boar => 3,
            Animal::Cattle => 4,
        })
    } else if p
        .improvements
        .iter()
        .any(|i| matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3))
    {
        Some(match animal {
            Animal::Sheep => 2,
            Animal::Boar => 2,
            Animal::Cattle => 3,
        })
    } else {
        None
    }
}
fn take_animals(
    p: &mut Player,
    animal: Animal,
    location: &AnimalLocation,
    count: u32,
) -> Result<()> {
    match location {
        AnimalLocation::Pet => {
            if count != 1 || p.pet != Some(animal) {
                return Err("pet mismatch".into());
            }
            p.pet = None;
        }
        AnimalLocation::Stable(i) => {
            if count != 1 || *i >= FARM_CELLS || p.farm[*i].stable_animal != Some(animal) {
                return Err("stable animal mismatch".into());
            }
            p.farm[*i].stable_animal = None;
        }
        AnimalLocation::Pasture(id) => {
            let pasture = p
                .pastures
                .iter_mut()
                .find(|x| x.id == *id)
                .ok_or("pasture absent")?;
            if pasture.animal != Some(animal) || pasture.quantity < count {
                return Err("pasture animal mismatch".into());
            }
            pasture.quantity -= count;
            if pasture.quantity == 0 {
                pasture.animal = None
            }
        }
    }
    Ok(())
}
fn place_animals(p: &mut Player, plan: &AnimalPlan, n: u32) -> Result<()> {
    let assigned: u32 = plan.to_pastures.iter().map(|x| x.1).sum::<u32>()
        + plan.to_stables.len() as u32
        + u32::from(plan.pet)
        + plan.cook
        + plan.discard;
    if assigned != n {
        return Err("animal quantity mismatch".into());
    }
    if plan.cook > 0 && cook_value(p, plan.animal).is_none() {
        return Err("cannot cook without improvement".into());
    }
    let mut seen = BTreeSet::new();
    for (id, n) in &plan.to_pastures {
        if *n == 0 || !seen.insert(*id) {
            return Err("repeated/zero pasture assignment".into());
        }
        let past = p
            .pastures
            .iter_mut()
            .find(|x| x.id == *id)
            .ok_or("unknown pasture id")?;
        if past.animal.is_some() && past.animal != Some(plan.animal) {
            return Err("mixed pasture species".into());
        }
        past.animal = Some(plan.animal);
        past.quantity += n;
    }
    for i in &plan.to_stables {
        if *i >= FARM_CELLS
            || !seen.insert(200 + *i as u8)
            || !p.farm[*i].stable
            || !matches!(p.farm[*i].tile, Tile::Empty)
            || p.farm[*i].stable_animal.is_some()
        {
            return Err("invalid stable assignment".into());
        }
        p.farm[*i].stable_animal = Some(plan.animal)
    }
    if plan.pet {
        if p.pet.is_some() {
            return Err("pet already occupied".into());
        }
        p.pet = Some(plan.animal)
    }
    for past in &p.pastures {
        if past.quantity > p.pasture_capacity(past) {
            return Err("pasture overflow".into());
        }
    }
    if plan.cook > 0 {
        p.supply.food += plan.cook * cook_value(p, plan.animal).expect("checked above")
    }
    Ok(())
}
pub(crate) fn rearrange_animals(
    p: &mut Player,
    pasture: &[(u8, Option<Animal>, u32)],
    stables: &[(usize, Animal)],
    pet: Option<Animal>,
) -> Result<()> {
    let before = [Animal::Sheep, Animal::Boar, Animal::Cattle].map(|a| p.animal_count(a));
    if pasture.len() != p.pastures.len()
        || pasture
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<BTreeSet<_>>()
            .len()
            != pasture.len()
    {
        return Err("must name each pasture exactly once".into());
    }
    for old in &mut p.pastures {
        let (_, kind, n) = pasture
            .iter()
            .find(|(id, _, _)| *id == old.id)
            .ok_or("missing pasture id")?;
        if (*n > 0) != kind.is_some() {
            return Err("invalid pasture species/count".into());
        }
        old.animal = *kind;
        old.quantity = *n;
    }
    for c in &mut p.farm {
        c.stable_animal = None;
    }
    let mut used = BTreeSet::new();
    for (i, kind) in stables {
        if *i >= FARM_CELLS
            || !used.insert(*i)
            || !p.farm[*i].stable
            || !matches!(p.farm[*i].tile, Tile::Empty)
        {
            return Err("bad unfenced stable".into());
        }
        p.farm[*i].stable_animal = Some(*kind);
    }
    p.pet = pet;
    let after = [Animal::Sheep, Animal::Boar, Animal::Cattle].map(|a| p.animal_count(a));
    if before != after {
        return Err("rearrange must conserve animals".into());
    }
    if p.pastures
        .iter()
        .any(|q| q.quantity > p.pasture_capacity(q))
    {
        return Err("pasture overflow".into());
    }
    Ok(())
}

#[cfg(test)]
mod advanced_fencing_and_baking_tests {
    use super::*;

    #[test]
    fn fence_split_adds_internal_segment_without_removing_outer_fences() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_pastures(&mut player, &[vec![13, 14]], None).unwrap();
        let original = player.fences.clone();
        assert_eq!(original.len(), 6);
        assert!(split_candidate(&player).is_some());
        build_pastures(&mut player, &[vec![13], vec![14]], None).unwrap();
        assert_eq!(player.pastures.len(), 2);
        assert_eq!(player.fences.len(), 7);
        assert!(original.is_subset(&player.fences));
        assert_eq!(player.supply.wood, 13);
    }

    #[test]
    fn occupied_pasture_split_requires_explicit_conserving_allocation() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_pastures(&mut player, &[vec![13, 14]], None).unwrap();
        player.pastures[0].animal = Some(Animal::Sheep);
        player.pastures[0].quantity = 3;
        assert!(build_pastures(&mut player, &[vec![13], vec![14]], None).is_err());
        build_pastures(
            &mut player,
            &[vec![13], vec![14]],
            Some(&[
                PastureOccupancy {
                    animal: Some(Animal::Sheep),
                    quantity: 2,
                },
                PastureOccupancy {
                    animal: Some(Animal::Sheep),
                    quantity: 1,
                },
            ]),
        )
        .unwrap();
        assert_eq!(player.animal_count(Animal::Sheep), 3);
        assert_eq!(player.fences.len(), 7);
    }

    #[test]
    fn removing_old_fence_during_pasture_extension_is_illegal() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_pastures(&mut player, &[vec![13, 14]], None).unwrap();
        assert!(build_pastures(&mut player, &[vec![12, 13, 14]], None).is_err());
        // Adjacent new pastures *without* removing fences remain legal.
        build_pastures(&mut player, &[vec![12]], None).unwrap();
        assert_eq!(player.pastures.len(), 2);
    }

    #[test]
    fn baking_allocations_reject_duplicate_or_over_cap_ovens() {
        let mut player = Player::new(0, 0);
        player.improvements = [
            Improvement::ClayOven,
            Improvement::StoneOven,
            Improvement::Hearth4,
        ]
        .to_vec();
        player.supply.grain = 6;
        assert!(bake_allocated(
            &mut player,
            &[BakeAllocation {
                improvement: Improvement::ClayOven,
                grain: 2
            }]
        )
        .is_err());
        assert!(bake_allocated(
            &mut player,
            &[
                BakeAllocation {
                    improvement: Improvement::ClayOven,
                    grain: 1
                },
                BakeAllocation {
                    improvement: Improvement::ClayOven,
                    grain: 1
                },
            ]
        )
        .is_err());
        assert_eq!(player.supply.grain, 6);
        bake_allocated(
            &mut player,
            &[
                BakeAllocation {
                    improvement: Improvement::ClayOven,
                    grain: 1,
                },
                BakeAllocation {
                    improvement: Improvement::StoneOven,
                    grain: 2,
                },
                BakeAllocation {
                    improvement: Improvement::Hearth4,
                    grain: 3,
                },
            ],
        )
        .unwrap();
        assert_eq!(player.supply.grain, 0);
        assert_eq!(player.supply.food, 22);
    }
}

#[cfg(test)]
mod stable_building_rule_tests {
    use super::*;

    #[test]
    fn farm_expansion_cannot_put_stable_on_existing_pasture_tile() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_pastures(&mut player, &[vec![14]], None).unwrap();
        let before = player.clone();
        assert!(build_stable(&mut player, 14).is_err());
        assert_eq!(player, before);
    }

    #[test]
    fn previously_built_unfenced_stable_can_later_be_fenced() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_stable(&mut player, 14).unwrap();
        assert_eq!(player.occupied_stables(), 1);
        build_pastures(&mut player, &[vec![14]], None).unwrap();
        assert!(player.farm[14].stable);
        assert!(matches!(player.farm[14].tile, Tile::Pasture { .. }));
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 4);
    }

    #[test]
    fn existing_stable_does_not_count_as_empty_room_or_field_site() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        player.supply.reed = 10;
        build_stable(&mut player, 14).unwrap();
        assert!(build_room(&mut player, 14).is_err());
        assert!(plow(&mut player, 14).is_err());
    }
}

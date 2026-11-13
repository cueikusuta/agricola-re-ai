//! Transactional rule application. Detail policies live outside this module.
use crate::breeding::BreedingPlan;
use crate::card_events::dispatch_play;
use crate::card_zones::{play_from_hand, CardMoveCtx};
use crate::harvest::{FeedingPlan, HarvestDetailPolicy, SimpleHarvestDetails};
use crate::model::*;
use crate::pending::Transition;
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
/// Staged detail query for the active player: elementary parameters that are
/// individually legal right now. It is a *hint*, not a legality definition;
/// the engine still validates the complete action. Card lists come only from
/// the active player's own hand and the public improvement market.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailHints {
    pub plow_cells: Vec<usize>,
    pub room_cells: Vec<usize>,
    pub stable_cells: Vec<usize>,
    pub sow_fields: Vec<usize>,
    pub occupations: Vec<String>,
    pub minors: Vec<String>,
    pub improvements: Vec<Improvement>,
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
    /// A pending decision was resolved (Issue #268 replay integrity).
    PendingResolution {
        player: usize,
        decision_id: u64,
        resolution: crate::pending::PendingResolution,
    },
}

pub(crate) fn is_bake_bread_detail(detail: &Detail) -> bool {
    match detail {
        Detail::Sow { bake, .. }
        | Detail::Buy {
            instant_bake: bake, ..
        }
        | Detail::Renovate {
            instant_bake: bake, ..
        } => *bake > 0,
        Detail::SowDetailed { baking, .. }
        | Detail::BuyDetailed {
            instant_baking: baking,
            ..
        }
        | Detail::BuyWithMode {
            instant_baking: baking,
            ..
        }
        | Detail::RenovateDetailed {
            instant_baking: baking,
            ..
        } => !baking.is_empty(),
        _ => false,
    }
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
        .map(|(_, g)| {
            let cost = (p.rooms() as u32).saturating_sub(if g == Good::Stone {
                p.stone_cost_discount
            } else {
                0
            });
            p.supply.reed >= 1 && p.supply.get(g) >= cost
        })
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
fn room_build_cost(p: &Player) -> u32 {
    let normal = p.room_material_cost.unwrap_or(5);
    if p.house == House::Wood {
        normal.min(p.wooden_room_cost.unwrap_or(5))
    } else if p.house == House::Stone {
        normal.saturating_sub(p.stone_cost_discount)
    } else {
        normal
    }
}
/// A stable does not occupy a room/field tile. A fenced pasture is still
/// a legal site (2016 Lookout revised rulebook, p. 8).
/// This predicate checks the site and stable supply, but not wood payment.
pub(crate) fn stable_site_available(p: &Player, i: usize) -> bool {
    i < FARM_CELLS
        && p.occupied_stables() < 4
        && !p.farm[i].stable
        && matches!(p.farm[i].tile, Tile::Empty | Tile::Pasture { .. })
}
fn can_build(p: &Player) -> bool {
    (p.supply.reed >= 2
        && p.supply.get(room_material(p)) >= room_build_cost(p)
        && available_room(p))
        || (p.supply.wood >= 2 && (0..FARM_CELLS).any(|i| stable_site_available(p, i)))
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
fn major_cost(p: &Player, i: Improvement) -> Vec<(Good, u32)> {
    i.cost()
        .into_iter()
        .map(|(good, amount)| {
            (
                good,
                match good {
                    Good::Wood => amount.saturating_sub(p.improvement_wood_discount),
                    Good::Stone => {
                        let bonus = if p.major_stone_discount_by_rooms {
                            p.rooms().saturating_sub(2) as u32
                        } else {
                            0
                        };
                        amount.saturating_sub(p.stone_cost_discount.saturating_add(bonus))
                    }
                    _ => amount,
                },
            )
        })
        .collect()
}
fn affordable(p: &Player, i: Improvement) -> bool {
    major_cost(p, i).iter().all(|&(g, n)| p.supply.get(g) >= n)
}

impl Game {
    /// Apply post-placement card rewards identically in committed moves and
    /// harvest previews, before the space becomes occupied.
    ///
    /// All three triggers go through the phase dispatcher rather than the legacy
    /// `cards::*` hooks, so the event budget and per-card use limits apply on
    /// the production path too. The legacy functions stay for the equivalence
    /// tests: #266 requires staged migration, so the old path is kept as the
    /// differential reference rather than deleted.
    ///
    /// Returns the ChainStep where a pause occurred (if any) so the caller can
    /// record the correct resume position (Issue #268 P1-3).
    ///
    /// When `allow_pause` is false (one-shot `apply` API), ChooseTriggerOrder
    /// pauses are disabled in the post-work chain so the one-shot path never
    /// encounters a pause it cannot represent. When `allow_pause` is true
    /// (`apply_with_plans`), pauses are enabled and propagated to the engine
    /// for the full resume contract.
    fn apply_post_work_card_effects(
        &mut self,
        player: usize,
        action: &Action,
        previous: &Player,
        allow_pause: bool,
    ) -> std::result::Result<(Transition, Option<crate::pending::ChainStep>), String> {
        // No cards means no card effects; skip the dispatch entirely so the
        // cardless hot path keeps its Step 2 cost.
        if !self.card_set.enabled() {
            return Ok((Transition::Completed, None));
        }
        // Bake trigger: only when this action actually baked bread, matching
        // cards::after_harvest_baking's own gate (round >= 2, harvest round).
        if is_bake_bread_detail(&action.detail) {
            let result = crate::card_events::dispatch_after_harvest_bake(
                &self.card_set,
                player,
                &mut self.cards,
                &mut self.public.players,
                &mut self.public.improvement_market,
                &mut self.delayed_well,
                self.public.round,
                &self.public.used,
                &mut self.event_budget,
                &mut self.card_uses,
                &self.card_use_limits,
                &mut self.pending,
                allow_pause, // post-work chain: allow ChooseTriggerOrder pause only when allow_pause=true
            )?;
            if let Transition::Paused(_) = result {
                return Ok((result, Some(crate::pending::ChainStep::AfterHarvestBake)));
            }
        }
        // Clay-construction trigger: `previous` is the pre-action player, so a
        // longer improvements list means this action bought one.
        let bought = if previous.improvements.len() < self.public.players[player].improvements.len()
        {
            self.public.players[player].improvements.last().copied()
        } else {
            None
        };
        let result = crate::card_events::dispatch_after_clay_construction(
            &self.card_set,
            player,
            previous,
            bought,
            &mut self.cards,
            &mut self.public.players,
            &mut self.public.improvement_market,
            &mut self.delayed_well,
            self.public.round,
            &self.public.used,
            &mut self.event_budget,
            &mut self.card_uses,
            &self.card_use_limits,
            &mut self.pending,
            allow_pause, // post-work chain: allow ChooseTriggerOrder pause only when allow_pause=true
        )?;
        if let Transition::Paused(_) = result {
            return Ok((
                result,
                Some(crate::pending::ChainStep::AfterClayConstruction),
            ));
        }
        // Worker trigger: AfterWorkerGain and AfterWorkerGainIfUsed, in the
        // active player's played_cards order.
        let result = crate::card_events::dispatch_after_worker(
            &self.card_set,
            player,
            action.space,
            &mut self.cards,
            &mut self.public.players,
            &mut self.public.improvement_market,
            &mut self.delayed_well,
            self.public.round,
            &self.public.used,
            &mut self.event_budget,
            &mut self.card_uses,
            &self.card_use_limits,
            &mut self.pending,
            allow_pause, // post-work chain: allow ChooseTriggerOrder pause only when allow_pause=true
        )?;
        if let Transition::Paused(_) = result {
            return Ok((result, Some(crate::pending::ChainStep::AfterWorker)));
        }
        Ok((Transition::Completed, None))
    }

    /// Spaces that exist on the board for the current mode/round, before the
    /// unused/extra-exclusivity and per-space rule filters are applied.
    fn space_pool(&self) -> Vec<Space> {
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
        list
    }

    /// Structurally available spaces: revealed, unused, and not blocked by the
    /// mutually exclusive extra-space rule. This deliberately ignores whether a
    /// candidate generator can produce a detail, so the rule engine accepts
    /// every legal detail, including ones the default policies would never
    /// propose. Detail-level legality is enforced by `execute_work` and the
    /// final `validate` invariant check in the transactional apply path.
    pub fn available_spaces(&self) -> Vec<Space> {
        if self.public.finished || self.pending.is_some() {
            return vec![];
        }
        self.available_spaces_for_resume()
    }

    /// Same space filter as [`Self::available_spaces`] but WITHOUT the
    /// `pending.is_some()` short-circuit. The resume legality re-check runs
    /// while the decision is still stored, so it needs the revealed/unused/
    /// mutual-exclusion filter on its own (Issue #268 R2-P1-1).
    pub(crate) fn available_spaces_for_resume(&self) -> Vec<Space> {
        if self.public.finished {
            return vec![];
        }
        let players = self.config.players;
        let mut list = self.space_pool();
        list.retain(|&s| {
            !self.public.used.contains(&s)
                && !(s.additional_for(players) && group_occupied(&self.public.used, players))
        });
        list
    }

    /// Candidate-narrowed listing used by AI policies. It is a subset of
    /// `available_spaces` and must not be used to define legal action sets.
    pub fn legal_spaces(&self) -> Vec<Space> {
        if self.public.finished {
            return vec![];
        }
        let me = &self.public.players[self.public.active_player];
        let players = self.config.players;
        let mut list = self.available_spaces();
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
            c.kind == crate::cards::CardKind::MinorImprovement
                && crate::cards::can_play(&self.card_set, c, me, 0)
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
                        && crate::cards::can_play(&self.card_set, c, me, u32::from(occupations > 0))
                }),
                Space::ExtraLessons => {
                    let fee = if players == 3 || occupations >= 2 {
                        2
                    } else {
                        1
                    };
                    own_cards.iter().any(|c| {
                        c.kind == crate::cards::CardKind::Occupation
                            && crate::cards::can_play(&self.card_set, c, me, fee)
                    })
                }
                Space::FamilyGrowthWithRoom => {
                    me.family < 5 && me.rooms() + usize::from(me.extra_housing) > me.family as usize
                }
                Space::FamilyGrowthNoRoom => me.family < 5,
                Space::ModestFamilyGrowth => {
                    self.public.round >= 5
                        && me.family < 5
                        && me.rooms() + usize::from(me.extra_housing) > me.family as usize
                }
                Space::Cultivation => available_field(me) || can_sow(me),
                Space::HouseRedevelopment | Space::FarmRedevelopment => can_renovate(me),
                _ => true,
            }
        });
        list
    }
    /// One-shot transactional apply that CANNOT represent a card-effect
    /// interrupt.
    ///
    /// Contract (Issue #268 / R2-P0-2): if the action's post-work card chain
    /// would pause on an Interrupt-phase choice, the call returns a precise
    /// `Err` and the game state is left byte-for-byte unchanged (the pause is
    /// detected on a working clone before any commit). Callers that must
    /// support interrupts use [`Self::apply_with_plans`] and then
    /// `Game::resolve_pending_and_resume`.
    pub fn apply(&mut self, action: Action) -> Result<()> {
        if self.pending.is_some() {
            return Err("pending decision must be resolved before new action".into());
        }
        self.apply_with_harvest_policy(action, &SimpleHarvestDetails)
    }
    /// One-shot apply with an explicit harvest policy. Same interrupt contract
    /// as [`Self::apply`]: a pause is rejected with a precise `Err` and no
    /// state change.
    pub fn apply_with_harvest_policy(
        &mut self,
        action: Action,
        policy: &dyn HarvestDetailPolicy,
    ) -> Result<()> {
        self.apply_with_plans_impl(action, None, None, policy, false)
            .map(|_| ())
    }

    /// Read-only, transactional validation using the exact same engine path as
    /// `apply`. Human input and AI selections share this validator; it never
    /// modifies `self` and rejects illegal details before any state change.
    pub fn validate_action(&self, action: &Action) -> Result<()> {
        let mut probe = self.search_clone();
        probe
            .apply_with_plans(action.clone(), None, None, &SimpleHarvestDetails)
            .map(|_| ())
    }

    /// Staged detail query for the active player. See [`DetailHints`].
    pub fn detail_hints(&self, space: Space) -> DetailHints {
        use crate::cards::CardKind;
        let active = self.public.active_player;
        let me = &self.public.players[active];
        let mut hints = DetailHints::default();
        let plow_legal = |i: usize| {
            matches!(me.farm[i].tile, Tile::Empty)
                && !me.farm[i].stable
                && (me.fields() == 0
                    || neighbors(i)
                        .iter()
                        .any(|&j| matches!(me.farm[j].tile, Tile::Field { .. })))
        };
        let room_legal = |i: usize| {
            matches!(me.farm[i].tile, Tile::Empty)
                && !me.farm[i].stable
                && neighbors(i)
                    .iter()
                    .any(|&j| matches!(me.farm[j].tile, Tile::Room))
        };
        if matches!(space, Space::Farmland | Space::Cultivation) {
            hints.plow_cells = (0..FARM_CELLS).filter(|&i| plow_legal(i)).collect();
        }
        if matches!(space, Space::FarmExpansion) {
            hints.room_cells = (0..FARM_CELLS).filter(|&i| room_legal(i)).collect();
            hints.stable_cells = (0..FARM_CELLS)
                .filter(|&i| stable_site_available(me, i))
                .collect();
        }
        if matches!(space, Space::GrainUtilization | Space::Cultivation) {
            hints.sow_fields = (0..FARM_CELLS)
                .filter(|&i| {
                    matches!(
                        me.farm[i].tile,
                        Tile::Field {
                            crop: None,
                            remaining: 0
                        }
                    )
                })
                .collect();
        }
        let occupations = me
            .played_cards
            .iter()
            .filter(|id| {
                self.card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == CardKind::Occupation)
            })
            .count();
        let hand = &self.cards.hands[active];
        let playable = |kind: CardKind, fee: u32| -> Vec<String> {
            hand.iter()
                .filter(|id| {
                    self.card_set.definition(id).is_some_and(|card| {
                        card.kind == kind && crate::cards::can_play(&self.card_set, card, me, fee)
                    })
                })
                .cloned()
                .collect()
        };
        match space {
            Space::Lessons => {
                hints.occupations = playable(CardKind::Occupation, u32::from(occupations > 0));
            }
            Space::ExtraLessons => {
                let fee = if self.config.players == 3 || occupations >= 2 {
                    2
                } else {
                    1
                };
                hints.occupations = playable(CardKind::Occupation, fee);
            }
            Space::MajorImprovement => {
                hints.minors = playable(CardKind::MinorImprovement, 0);
                hints.improvements = self
                    .public
                    .improvement_market
                    .iter()
                    .copied()
                    .filter(|&i| {
                        affordable(me, i)
                            || i.is_cooking()
                                && matches!(i, Improvement::Hearth4 | Improvement::Hearth5)
                                && me.improvements.iter().any(|c| {
                                    matches!(c, Improvement::Fireplace2 | Improvement::Fireplace3)
                                })
                    })
                    .collect();
            }
            Space::MeetingPlace | Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth => {
                hints.minors = playable(CardKind::MinorImprovement, 0);
            }
            _ => {}
        }
        hints
    }

    /// Bounded, read-only fencing suggestions for the active player. It does
    /// not enumerate every connected subset: it offers single- and two-cell
    /// new pastures, one-cell extensions of existing pastures, and the
    /// deterministic splits the default policy already finds. The engine still
    /// validates any submitted construction.
    pub fn fence_hints(&self) -> crate::hints::FenceHints {
        use crate::hints::{FenceHints, FenceProposal};
        let p = &self.public.players[self.public.active_player];
        let free = p.free_fence_wood;
        let budget = p.supply.wood.saturating_add(free);
        let remaining = 15usize.saturating_sub(p.fences.len());
        let mut proposals: Vec<FenceProposal> = Vec::new();
        let mut extendable = BTreeSet::new();
        for pasture in &p.pastures {
            for &cell in &pasture.cells {
                for adj in neighbors(cell) {
                    if matches!(p.farm[adj].tile, Tile::Empty) {
                        extendable.insert(adj);
                    }
                }
            }
        }
        let touches_existing = |region: &[usize]| {
            region.iter().any(|&i| {
                neighbors(i)
                    .iter()
                    .any(|&j| matches!(p.farm[j].tile, Tile::Pasture { .. }))
            })
        };
        // Splits first: they are rare but important, so never crowd them out.
        if let Some(regions) = split_candidate(p) {
            if let Some(proposal) = fence_proposal(p, budget, regions, None, "split") {
                proposals.push(proposal);
            }
        }
        if let Some((regions, occupancy)) = split_occupied_candidate(p) {
            if let Some(proposal) = fence_proposal(p, budget, regions, Some(occupancy), "split") {
                proposals.push(proposal);
            }
        } else if let Some((regions, occupancy)) = general_split_candidate(p) {
            if let Some(proposal) = fence_proposal(p, budget, regions, Some(occupancy), "split") {
                proposals.push(proposal);
            }
        }
        // Two-cell new pastures: guarantee multi-cell shapes are visible even
        // when there are many single-cell options.
        let mut multi = 0usize;
        for a in 0..FARM_CELLS {
            if multi >= 8 {
                break;
            }
            if !matches!(p.farm[a].tile, Tile::Empty) {
                continue;
            }
            for b in neighbors(a) {
                if multi >= 8 {
                    break;
                }
                if b < a || !matches!(p.farm[b].tile, Tile::Empty) {
                    continue;
                }
                if !p.pastures.is_empty() && !touches_existing(&[a, b]) {
                    continue;
                }
                if let Some(proposal) = fence_proposal(p, budget, vec![vec![a, b]], None, "new") {
                    proposals.push(proposal);
                    multi += 1;
                }
            }
        }
        // Single-cell new pastures, bounded so the dialog stays readable.
        let mut singles = 0usize;
        for i in 0..FARM_CELLS {
            if singles >= 8 {
                break;
            }
            if !matches!(p.farm[i].tile, Tile::Empty) {
                continue;
            }
            if !p.pastures.is_empty() && !touches_existing(&[i]) {
                continue;
            }
            if let Some(proposal) = fence_proposal(p, budget, vec![vec![i]], None, "new") {
                proposals.push(proposal);
                singles += 1;
            }
        }
        FenceHints {
            available_wood: p.supply.wood,
            free_fence_wood: free,
            remaining_segments: remaining,
            extendable_cells: extendable.into_iter().collect(),
            proposals,
        }
    }

    /// Full transactional apply that CAN report a card-effect interrupt.
    ///
    /// Contract (Issue #268): a work action whose post-work card chain hits an
    /// Interrupt-phase choice returns [`Transition::Paused`]; the mid-action
    /// state is committed and the decision (with its typed continuation) is
    /// stored in `Game::pending`. The caller completes the action with
    /// `Game::resolve_pending_and_resume`.
    pub(crate) fn apply_with_plans(
        &mut self,
        action: Action,
        forced: Option<Vec<FeedingPlan>>,
        forced_breeding: Option<Vec<BreedingPlan>>,
        policy: &dyn HarvestDetailPolicy,
    ) -> std::result::Result<Transition, String> {
        self.apply_with_plans_impl(action, forced, forced_breeding, policy, true)
    }

    /// Shared implementation of the transactional apply. `allow_pause` selects
    /// the interrupt contract: when `true` a pause is committed and returned
    /// (the full API); when `false` a pause is rejected with a precise `Err`
    /// BEFORE any state is committed, leaving the game untouched (the one-shot
    /// `apply`/`apply_with_harvest_policy` API, which cannot represent a paused
    /// state). See [`Self::apply`].
    fn apply_with_plans_impl(
        &mut self,
        action: Action,
        forced: Option<Vec<FeedingPlan>>,
        forced_breeding: Option<Vec<BreedingPlan>>,
        policy: &dyn HarvestDetailPolicy,
        allow_pause: bool,
    ) -> std::result::Result<Transition, String> {
        let player = self.public.active_player;
        if !self.available_spaces().contains(&action.space) {
            return Err(format!("illegal space {:?}", action.space));
        }
        let mut next = self.clone();
        let record = self.record_events;
        let before = if record {
            self.state_hash()
        } else {
            String::new()
        };
        // The event budget guards against unbounded effect chains WITHIN one
        // action, so each action starts from the full budget again.
        next.event_budget = crate::model::DEFAULT_EVENT_BUDGET;
        next.execute_work(player, &action)?;
        let (card_transition, pause_step) = next.apply_post_work_card_effects(
            player,
            &action,
            &self.public.players[player],
            allow_pause,
        )?;
        match card_transition {
            Transition::Completed => {}
            Transition::Paused(_) if !allow_pause => {
                // R2-P0-2: `apply`/`apply_with_harvest_policy` cannot represent a
                // paused state. Reject BEFORE committing `next`, so the game is
                // byte-for-byte unchanged and the caller gets a precise reason
                // instead of a later opaque "illegal space" refusal.
                return Err(
                    "action paused on a pending decision; apply cannot represent an \
                     interrupt — use apply_with_plans and Game::resolve_pending_and_resume"
                        .into(),
                );
            }
            Transition::Paused(mut pending) => {
                // P0-1: Commit the mid-action state (worker action + partial card effects applied)
                // so resume does not lose mid-action modifications.
                *self = next;
                // Stamp the decision with engine-level metadata (Issue #268):
                // - decision ID
                // - state_hash_before (state hash before the action started)
                // - increment next_decision_id
                // - continuation position for resume
                let decision_id = self.next_decision_id;
                self.next_decision_id += 1;
                pending.id = decision_id;
                pending.state_hash_before = self.state_hash();
                // Build continuation from the pause context (card index, effect index)
                // and the work completion data needed to finish the action.
                let after = pending
                    .context
                    .get("card_index")
                    .and_then(|v| v.as_u64())
                    .and_then(|ci| {
                        pending
                            .context
                            .get("effect_index")
                            .and_then(|v| v.as_u64())
                            .map(|ei| crate::pending::ChainPosition {
                                card_index: ci as usize,
                                effect_index: ei as usize,
                            })
                    })
                    .unwrap_or(crate::pending::ChainPosition {
                        card_index: 0,
                        effect_index: 0,
                    });
                // P1-3: Pass the actual ChainStep from apply_post_work_card_effects through the pause path
                let step = pause_step.unwrap_or(crate::pending::ChainStep::AfterWorker);
                // The phase the pause occurred in: a #270 ChooseTriggerOrder
                // pause records the phase it collected the simultaneous
                // triggers in ("After") in its context; the classic #268
                // Interrupt pause has none, so it defaults to Interrupt.
                let pause_phase = pending
                    .context
                    .get("phase")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "After" => crate::card_events::TriggerPhase::After,
                        _ => crate::card_events::TriggerPhase::Interrupt,
                    })
                    .unwrap_or(crate::card_events::TriggerPhase::Interrupt);
                let resume = crate::pending::PostWorkResume {
                    step,
                    phase: pause_phase,
                    after,
                };
                let completion = Box::new(crate::pending::WorkCompletion {
                    action: action.clone(),
                    previous_player: self.public.players[player].clone(),
                    harvest_plans: forced.clone(),
                    breeding_plans: forced_breeding.clone(),
                    logged_before: if record {
                        before.clone()
                    } else {
                        String::new()
                    },
                });
                pending.continuation = crate::pending::Continuation::PostWork {
                    player,
                    resume,
                    completion,
                };
                // Commit the stamped decision to self so it survives.
                self.pending = Some((*pending).clone());
                // Record the work event NOW, with `after` = the paused state.
                // The mid-action state is committed (P0-1), so the event log must
                // be able to reproduce it; the resolution event (logged by
                // `resolve_pending_and_resume`) then chains to the final state.
                if record {
                    let payload = serde_json::to_value(RecordedAction::Work {
                        player,
                        action: action.clone(),
                        harvest_plans: forced.clone(),
                        breeding_plans: forced_breeding.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                    self.log.events.push(Event {
                        player: Some(player),
                        kind: "work".into(),
                        payload,
                        before: before.clone(),
                        after: self.state_hash(),
                    });
                }
                return Ok(Transition::Paused(pending));
            }
        }
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
        crate::validation::validate_incremental_or_full(&next, self)?;
        if record {
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
        }
        *self = next;
        Ok(Transition::Completed)
    }
    /// Does the current placement finish a round that includes a harvest?
    /// The UI uses this to pause before the harvest choices are committed.
    pub fn will_harvest_after_work(&self) -> bool {
        if self.public.finished {
            return false;
        }
        let mut placed = self.public.workers_placed.clone();
        placed[self.public.active_player] += 1;
        placed == self.public.work_quota && HARVEST_ROUNDS.contains(&self.public.round)
    }

    /// Read-only harvest preview for an explicitly chosen final placement.
    /// The result contains public information only; the internal preview
    /// retains hidden data but is never sent to the browser.
    pub fn preview_harvest(
        &self,
        action: &Action,
        feeding: Option<&[FeedingPlan]>,
    ) -> Result<PublicState> {
        if !self.will_harvest_after_work() || !self.available_spaces().contains(&action.space) {
            return Err("not a legal last placement before harvest".into());
        }
        let mut next = self.clone();
        let id = next.public.active_player;
        next.execute_work(id, action)?;
        let (transition, _) =
            next.apply_post_work_card_effects(id, action, &self.public.players[id], false)?;
        if let Transition::Paused(_) = transition {
            return Err("preview cannot handle paused decision".into());
        }
        next.public.used.insert(action.space);
        next.public.workers_placed[id] += 1;
        next.harvest_fields();
        if let Some(plans) = feeding {
            next.feed_after_fields(plans)?;
        }
        Ok(next.public)
    }

    /// Read-only legality probe for free actions. Reuse the *same* transactional
    /// path as `apply_free`, including its invariant checks, so the probe
    /// cannot disagree with the committed rule engine. No events or state are
    /// added to `self` (only to the discarded clone).
    pub fn validate_free(&self, player: usize, free: &FreeAction) -> Result<()> {
        let mut probe = self.search_clone();
        probe.apply_free(player, free.clone())
    }

    pub fn apply_free(&mut self, player: usize, free: FreeAction) -> Result<()> {
        if self.public.finished || player >= self.public.players.len() {
            return Err("not an active game/player".into());
        }
        if self.pending.is_some() {
            return Err("pending decision must be resolved before new action".into());
        }
        let mut next = self.clone();
        let record = self.record_events;
        let before = if record {
            self.state_hash()
        } else {
            String::new()
        };
        // Same per-action budget reset as apply_with_plans.
        next.event_budget = crate::model::DEFAULT_EVENT_BUDGET;
        next.execute_free(player, &free)?;
        crate::validation::validate_free_action_or_full(&next, self, player)?;
        if record {
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
        }
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
        Self::replay_with_setup(config, cards, None, events)
    }
    /// Replay with explicitly drafted hands, or with a seed-based random deal.
    /// Existing callers preserve the historical game-seed start-player draw.
    pub fn replay_with_setup(
        config: Config,
        cards: crate::cards::CardSet,
        hands: Option<Vec<Vec<String>>>,
        events: &[Event],
    ) -> Result<Self> {
        Self::replay_with_setup_and_start_player(config, cards, hands, None, events)
    }

    /// Replay a draft whose first start player was publicly drawn before picking.
    pub fn replay_with_setup_and_start_player(
        config: Config,
        cards: crate::cards::CardSet,
        hands: Option<Vec<Vec<String>>>,
        first_start_player: Option<usize>,
        events: &[Event],
    ) -> Result<Self> {
        Self::replay_with_setup_and_start_player_inner(
            config,
            cards,
            hands,
            first_start_player,
            events,
            false,
        )
    }
    /// Internal replay with optional pause-at-end handling for load_json verification.
    pub(crate) fn replay_with_setup_and_start_player_inner(
        config: Config,
        cards: crate::cards::CardSet,
        hands: Option<Vec<Vec<String>>>,
        first_start_player: Option<usize>,
        events: &[Event],
        expect_pause_at_end: bool,
    ) -> Result<Self> {
        let mut g = match (hands, first_start_player) {
            (Some(hands), Some(starter)) => {
                Game::with_cards_and_hands_with_start_player(config, cards, hands, starter)?
            }
            (Some(hands), None) => Game::with_cards_and_hands(config, cards, hands)?,
            (None, None) => Game::with_cards(config, cards)?,
            (None, Some(_)) => return Err("start player override requires draft hands".into()),
        };
        // Replay events with pause/resume handling. A work event that paused
        // in the original game is followed by a pending_resolution event.
        // We detect this by looking ahead one event.
        let mut event_iter = events.iter().peekable();
        while let Some(event) = event_iter.next() {
            let action: RecordedAction =
                serde_json::from_value(event.payload.clone()).map_err(|e| e.to_string())?;
            let is_work = matches!(action, RecordedAction::Work { .. });
            if is_work {
                // Check if next event is pending_resolution (indicating a pause occurred)
                let next_is_resolution = event_iter
                    .peek()
                    .map(|e| e.kind == "pending_resolution")
                    .unwrap_or(false);
                // Also check if this is the last event and we expect a pause at the end
                let is_last_event = event_iter.peek().is_none();
                if next_is_resolution || (expect_pause_at_end && is_last_event) {
                    // Original game paused: use apply_with_plans and process resolution next
                    g.apply_recorded_with_pause(event)?;
                    // The next event (pending_resolution) will be processed in the next iteration
                } else {
                    // Original game did not pause: use one-shot apply_with_harvest_policy
                    g.apply_recorded_no_pause(event)?;
                }
            } else {
                // Free actions and pending_resolution events use the standard path
                g.apply_recorded(event)?;
            }
        }
        Ok(g)
    }
    /// Apply one committed event, checking the before/after state hashes.
    /// [`Self::replay_with_setup`] and the M12 episode-data conversion share
    /// this single validation path so they cannot drift apart.
    pub fn apply_recorded(&mut self, event: &Event) -> Result<()> {
        if self.state_hash() != event.before {
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
                    || player != self.public.active_player
                {
                    return Err("replay work metadata/player mismatch".into());
                }
                // Determine if this is a harvest turn by checking the state
                // before the action (current replay state).
                let is_harvest_turn = self.will_harvest_after_work();
                if is_harvest_turn {
                    // Harvest turn: the original game used apply_with_harvest_policy
                    // with the recorded feeding/breeding plans.
                    struct ReplayHarvestPolicy {
                        feeding: Vec<FeedingPlan>,
                        breeding: Vec<BreedingPlan>,
                    }
                    impl HarvestDetailPolicy for ReplayHarvestPolicy {
                        fn plan_feeding(&self, obs: &Observation) -> FeedingPlan {
                            self.feeding[obs.player_id].clone()
                        }
                        fn plan_breeding(&self, obs: &Observation) -> BreedingPlan {
                            self.breeding[obs.player_id].clone()
                        }
                    }
                    let feeding = harvest_plans
                        .unwrap_or_else(|| vec![FeedingPlan::default(); self.config.players]);
                    let breeding = breeding_plans.unwrap_or_else(|| {
                        (0..self.config.players)
                            .map(|i| BreedingPlan::simple(&self.public.players[i]))
                            .collect()
                    });
                    let policy = ReplayHarvestPolicy { feeding, breeding };
                    self.apply_with_harvest_policy(action, &policy)?;
                } else {
                    // Non-harvest turn: the original game used apply (one-shot).
                    // Use apply_with_harvest_policy with SimpleHarvestDetails to match.
                    self.apply_with_harvest_policy(action, &SimpleHarvestDetails)?;
                }
            }
            RecordedAction::Free { player, action } => {
                if event.kind != "free" || event.player != Some(player) {
                    return Err("replay free action metadata/player mismatch".into());
                }
                self.apply_free(player, action)?;
            }
            RecordedAction::PendingResolution {
                player,
                decision_id,
                resolution,
            } => {
                if event.kind != "pending_resolution" || event.player != Some(player) {
                    return Err("replay pending_resolution metadata/player mismatch".into());
                }
                // Verify the decision ID matches (if we have a pending decision)
                if let Some(pending) = self.pending.as_ref() {
                    if pending.id != decision_id {
                        return Err("replay pending decision ID mismatch".into());
                    }
                }
                // During replay, resolve the pending decision AND run its continuation
                // (if any) to match the original game's resume behavior.
                self.resolve_pending_and_resume(resolution).map(|_| ())?;
            }
        }
        if self.state_hash() != event.after {
            return Err("replay poststate hash mismatch".into());
        }
        Ok(())
    }

    /// Variant of `apply_recorded` for work events that DID pause in the
    /// original game. Uses `apply_with_plans` (allow_pause=true) to match
    /// the pausing behavior, and expects the next event to be a
    /// `pending_resolution` that will resolve the pause.
    fn apply_recorded_with_pause(&mut self, event: &Event) -> Result<()> {
        if self.state_hash() != event.before {
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
                    || player != self.public.active_player
                {
                    return Err("replay work metadata/player mismatch".into());
                }
                // Use apply_with_plans which allows pauses (allow_pause=true)
                let transition = self.apply_with_plans(
                    action,
                    harvest_plans.clone(),
                    breeding_plans.clone(),
                    &SimpleHarvestDetails,
                )?;
                // The work event's `after` hash should be the paused state (if paused)
                // or the final state (if completed). Since we detected a pending_resolution
                // next, we expect a pause.
                if transition == Transition::Completed
                    && (harvest_plans.is_some() || breeding_plans.is_some())
                {
                    return Err("replay harvest decisions missing or unexpected".into());
                }
            }
            RecordedAction::Free { player, action } => {
                if event.kind != "free" || event.player != Some(player) {
                    return Err("replay free action metadata/player mismatch".into());
                }
                self.apply_free(player, action)?;
            }
            RecordedAction::PendingResolution { .. } => {
                return Err("pending_resolution not expected in with-pause path".into());
            }
        }
        if self.state_hash() != event.after {
            return Err("replay poststate hash mismatch".into());
        }
        Ok(())
    }

    /// Variant of `apply_recorded` for work events that did NOT pause in the
    /// original game. Uses `apply_with_harvest_policy` (allow_pause=false) to
    /// match the one-shot apply behavior. Used for work events without a
    /// following pending_resolution, and for all free/pending_resolution events.
    fn apply_recorded_no_pause(&mut self, event: &Event) -> Result<()> {
        if self.state_hash() != event.before {
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
                    || player != self.public.active_player
                {
                    return Err("replay work metadata/player mismatch".into());
                }
                // Determine if this is a harvest turn by checking the state
                // before the action (current replay state).
                let is_harvest_turn = self.will_harvest_after_work();
                // The recorded plans must match the turn shape: a harvest turn
                // carries both feeding and breeding plans; a non-harvest turn
                // carries none. A missing/unexpected set is a tampered or
                // truncated log and must be rejected (replay_integrity).
                if is_harvest_turn {
                    if harvest_plans.is_none() || breeding_plans.is_none() {
                        return Err("replay harvest decisions missing or unexpected".into());
                    }
                } else if harvest_plans.is_some() || breeding_plans.is_some() {
                    return Err("replay harvest decisions missing or unexpected".into());
                }
                if is_harvest_turn {
                    // Harvest turn: the original game used apply_with_harvest_policy
                    // with the recorded feeding/breeding plans.
                    struct ReplayHarvestPolicy {
                        feeding: Vec<FeedingPlan>,
                        breeding: Vec<BreedingPlan>,
                    }
                    impl HarvestDetailPolicy for ReplayHarvestPolicy {
                        fn plan_feeding(&self, obs: &Observation) -> FeedingPlan {
                            self.feeding[obs.player_id].clone()
                        }
                        fn plan_breeding(&self, obs: &Observation) -> BreedingPlan {
                            self.breeding[obs.player_id].clone()
                        }
                    }
                    let feeding = harvest_plans
                        .unwrap_or_else(|| vec![FeedingPlan::default(); self.config.players]);
                    let breeding = breeding_plans.unwrap_or_else(|| {
                        (0..self.config.players)
                            .map(|i| BreedingPlan::simple(&self.public.players[i]))
                            .collect()
                    });
                    let policy = ReplayHarvestPolicy { feeding, breeding };
                    self.apply_with_harvest_policy(action, &policy)?;
                } else {
                    // Non-harvest turn: the original game used apply (one-shot).
                    // Use apply_with_harvest_policy with SimpleHarvestDetails to match.
                    self.apply_with_harvest_policy(action, &SimpleHarvestDetails)?;
                }
            }
            RecordedAction::Free { player, action } => {
                if event.kind != "free" || event.player != Some(player) {
                    return Err("replay free action metadata/player mismatch".into());
                }
                self.apply_free(player, action)?;
            }
            RecordedAction::PendingResolution { .. } => {
                return Err("pending_resolution not expected in no-pause path".into());
            }
        }
        if self.state_hash() != event.after {
            return Err("replay poststate hash mismatch".into());
        }
        Ok(())
    }
}
/// This is the single implementation shared by `next_worker_or_round`
/// and the validation layer (`validate_incremental`/`validate_free_action`).
/// Returns the index of the next player who has not placed all their workers,
/// or `n` (player count) if all players have placed their quota (round should advance).
pub fn expected_active_player(
    start_player: usize,
    workers_placed: &[u8],
    work_quota: &[u8],
) -> usize {
    let n = workers_placed.len();
    let mut next = start_player;
    for _ in 0..n {
        next = (next + 1) % n;
        if workers_placed[next] < work_quota[next] {
            return next;
        }
    }
    n // all players have placed their quota
}

impl Game {
    pub(crate) fn next_worker_or_round(
        &mut self,
        feeding: Option<&[FeedingPlan]>,
        breeding: Option<&[BreedingPlan]>,
    ) -> Result<()> {
        let n = self.public.players.len();
        let next = expected_active_player(
            self.public.active_player,
            &self.public.workers_placed,
            &self.public.work_quota,
        );
        if next < n {
            self.public.active_player = next;
            return Ok(());
        }
        if HARVEST_ROUNDS.contains(&self.public.round) {
            self.harvest(
                feeding.ok_or("missing feeding decisions")?,
                breeding.ok_or("missing breeding decisions")?,
            )?;
        }
        if self.public.round == 14 {
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
                // Handle card play first (before borrowing p for the rest).
                // PlayMinor is valid even when the card set is disabled: the
                // legacy main-branch semantics accept the detail and rely on
                // cards::play to reject plays when no set exists.
                match detail {
                    Detail::None => {}
                    Detail::PlayMinor { card_id } if self.card_set.enabled() => {
                        // Validate and pay costs
                        crate::cards::play_validate_pay(
                            &self.card_set,
                            &mut self.cards.hands[id],
                            &mut self.public.players[id],
                            card_id,
                            crate::cards::CardKind::MinorImprovement,
                            0,
                        )?;
                        // Canonical zone move: hand -> played (2016 Lookout p.6)
                        let mut ctx = CardMoveCtx {
                            cards: &mut self.cards,
                            players: &mut self.public.players,
                            improvement_market: &mut self.public.improvement_market,
                        };
                        play_from_hand(&mut ctx, id, card_id)?;
                        // Dispatch Replace/Before phases for the new card only
                        dispatch_play(
                            &self.card_set,
                            id,
                            card_id,
                            &mut self.cards,
                            &mut self.public.players,
                            &mut self.public.improvement_market,
                            &mut self.delayed_well,
                            self.public.round,
                            &self.public.used,
                            &mut self.event_budget,
                            &mut self.card_uses,
                            &self.card_use_limits,
                            &mut self.pending,
                        )?;
                        // Update card points
                        let card = self.card_set.definition(card_id).ok_or("unknown card")?;
                        crate::cards::update_card_points_on_play(
                            &mut self.public.players[id],
                            card.points,
                        )?;
                    }
                    _ => return Err("invalid starting-player details".into()),
                }
                // Now handle the rest of the action
                let n = self.public.accumulations.remove(&space).unwrap_or(0);
                self.public.players[id].supply.food += n;
                self.public.start_player = id;
            }
            Space::Lessons | Space::ExtraLessons => {
                // Occupation play: Lessons requires an occupation selection.
                // (2016 Lookout p.8: multiple card effects → active player's
                // chosen order; here the play IS the selection.)
                let Detail::PlayOccupation { card_id } = detail else {
                    return Err("occupation selection required".into());
                };
                let count = self.public.players[id]
                    .played_cards
                    .iter()
                    .filter(|pid| {
                        self.card_set
                            .definition(pid)
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
                crate::cards::play_validate_pay(
                    &self.card_set,
                    &mut self.cards.hands[id],
                    &mut self.public.players[id],
                    card_id,
                    crate::cards::CardKind::Occupation,
                    fee,
                )?;
                // Canonical zone move: hand -> played (2016 Lookout p.6)
                let mut ctx = CardMoveCtx {
                    cards: &mut self.cards,
                    players: &mut self.public.players,
                    improvement_market: &mut self.public.improvement_market,
                };
                play_from_hand(&mut ctx, id, card_id)?;
                // Dispatch Replace/Before phases for the new card only
                dispatch_play(
                    &self.card_set,
                    id,
                    card_id,
                    &mut self.cards,
                    &mut self.public.players,
                    &mut self.public.improvement_market,
                    &mut self.delayed_well,
                    self.public.round,
                    &self.public.used,
                    &mut self.event_budget,
                    &mut self.card_uses,
                    &self.card_use_limits,
                    &mut self.pending,
                )?;
                // Update card points
                let card = self.card_set.definition(card_id).ok_or("unknown card")?;
                crate::cards::update_card_points_on_play(
                    &mut self.public.players[id],
                    card.points,
                )?;
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
            Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth => {
                // Round gate for ModestFamilyGrowth must be checked BEFORE any card play
                // (legacy semantics: gate error before paying costs)
                if space == Space::ModestFamilyGrowth && self.public.round < 5 {
                    return Err("modest family growth is only available from round 5".into());
                }
                // Handle card play first (before borrowing p for the rest)
                if let Detail::GrowWithMinor { card_id } = detail {
                    if self.card_set.enabled() {
                        // Validate and pay costs (use internal fn; cards::play does full legacy for direct callers)
                        crate::cards::play_validate_pay(
                            &self.card_set,
                            &mut self.cards.hands[id],
                            &mut self.public.players[id],
                            card_id,
                            crate::cards::CardKind::MinorImprovement,
                            0,
                        )?;
                        // Move card from hand to played
                        let mut ctx = CardMoveCtx {
                            cards: &mut self.cards,
                            players: &mut self.public.players,
                            improvement_market: &mut self.public.improvement_market,
                        };
                        play_from_hand(&mut ctx, id, card_id)?;
                        // Dispatch Replace/Before phases
                        dispatch_play(
                            &self.card_set,
                            id,
                            card_id,
                            &mut self.cards,
                            &mut self.public.players,
                            &mut self.public.improvement_market,
                            &mut self.delayed_well,
                            self.public.round,
                            &self.public.used,
                            &mut self.event_budget,
                            &mut self.card_uses,
                            &self.card_use_limits,
                            &mut self.pending,
                        )?;
                        // Update card points
                        let card = self.card_set.definition(card_id).ok_or("unknown card")?;
                        crate::cards::update_card_points_on_play(
                            &mut self.public.players[id],
                            card.points,
                        )?;
                    }
                }
                // Now handle the rest of the action
                let p = &mut self.public.players[id];
                match detail {
                    Detail::None => grow(p, true)?,
                    Detail::GrowWithMinor { .. } => {} // Already handled above
                    _ => return Err("invalid growth details".into()),
                }
            }
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
                // Handle card play first (before borrowing p for the rest)
                Detail::PlayMinor { card_id } if self.card_set.enabled() => {
                    // Validate and pay costs (use internal fn; cards::play does full legacy for direct callers)
                    crate::cards::play_validate_pay(
                        &self.card_set,
                        &mut self.cards.hands[id],
                        &mut self.public.players[id],
                        card_id,
                        crate::cards::CardKind::MinorImprovement,
                        0,
                    )?;
                    // Move card from hand to played
                    let mut ctx = CardMoveCtx {
                        cards: &mut self.cards,
                        players: &mut self.public.players,
                        improvement_market: &mut self.public.improvement_market,
                    };
                    play_from_hand(&mut ctx, id, card_id)?;
                    // Dispatch Replace/Before phases
                    dispatch_play(
                        &self.card_set,
                        id,
                        card_id,
                        &mut self.cards,
                        &mut self.public.players,
                        &mut self.public.improvement_market,
                        &mut self.delayed_well,
                        self.public.round,
                        &self.public.used,
                        &mut self.event_budget,
                        &mut self.card_uses,
                        &self.card_use_limits,
                        &mut self.pending,
                    )?;
                    // Update card points
                    let card = self.card_set.definition(card_id).ok_or("unknown card")?;
                    crate::cards::update_card_points_on_play(
                        &mut self.public.players[id],
                        card.points,
                    )?;
                }
                Detail::Buy {
                    improvement,
                    instant_bake,
                } => {
                    let p = &mut self.public.players[id];
                    buy_improvement(
                        p,
                        &mut self.public.improvement_market,
                        &mut self.delayed_well,
                        &mut self.well_food_schedule,
                        self.public.round,
                        *improvement,
                        PurchaseArgs {
                            instant_bake: *instant_bake,
                            allocation: None,
                            mode: PurchaseMode::Auto,
                        },
                    )?;
                }
                Detail::BuyDetailed {
                    improvement,
                    instant_baking,
                } => {
                    let p = &mut self.public.players[id];
                    buy_improvement(
                        p,
                        &mut self.public.improvement_market,
                        &mut self.delayed_well,
                        &mut self.well_food_schedule,
                        self.public.round,
                        *improvement,
                        PurchaseArgs {
                            instant_bake: 0,
                            allocation: Some(instant_baking),
                            mode: PurchaseMode::Auto,
                        },
                    )?;
                }
                Detail::BuyWithMode {
                    improvement,
                    instant_baking,
                    mode,
                } => {
                    let p = &mut self.public.players[id];
                    buy_improvement(
                        p,
                        &mut self.public.improvement_market,
                        &mut self.delayed_well,
                        &mut self.well_food_schedule,
                        self.public.round,
                        *improvement,
                        PurchaseArgs {
                            instant_bake: 0,
                            allocation: Some(instant_baking),
                            mode: *mode,
                        },
                    )?;
                }
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
                            &mut self.well_food_schedule,
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
                            &mut self.well_food_schedule,
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
    let room_cost = room_build_cost(p);
    p.supply.pay(material, room_cost)?;
    p.supply.pay(Good::Reed, 2)?;
    p.farm[i].tile = Tile::Room;
    Ok(())
}
fn build_stable(p: &mut Player, i: usize) -> Result<()> {
    if !stable_site_available(p, i) {
        return Err("stable location illegal".into());
    }
    p.supply.pay(Good::Wood, 2)?;
    p.farm[i].stable = true;
    Ok(())
}
fn grow(p: &mut Player, room_only: bool) -> Result<()> {
    if p.family >= 5 || room_only && p.rooms() + usize::from(p.extra_housing) <= p.family as usize {
        return Err("family growth not permitted".into());
    }
    p.family += 1;
    p.newborns += 1;
    Ok(())
}
fn renovate(p: &mut Player) -> Result<()> {
    let (new, g) = next_house(p.house).ok_or("stone is final house")?;
    let cost = (p.rooms() as u32).saturating_sub(if g == Good::Stone {
        p.stone_cost_discount
    } else {
        0
    });
    p.supply.pay(g, cost)?;
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
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    well_food_schedule: &mut BTreeMap<u8, Vec<usize>>,
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
        for (good, n) in major_cost(p, item) {
            p.supply.pay(good, n)?;
        }
    }
    market.remove(pos);
    p.improvements.push(item);
    if item == Improvement::Well {
        for r in round + 1..=14u8.min(round.saturating_add(5)) {
            delayed_well.entry(r).or_default().push(p.id);
            well_food_schedule.entry(r).or_default().push(p.id);
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
/// Fence segments implied by the changed regions plus every unchanged pasture,
/// matching `build_pastures`. Rejects disconnected/out-of-bounds/overlapping
/// regions and any construction that would demolish fencing.
fn fence_edges(p: &Player, regions: &[Vec<usize>]) -> Option<BTreeSet<(u8, u8)>> {
    let mut edges = BTreeSet::new();
    let mut affected = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for region in regions {
        if region.is_empty() || region.iter().any(|&i| i >= FARM_CELLS) || !connected(region) {
            return None;
        }
        for &i in region {
            if !covered.insert(i) {
                return None;
            }
            if let Tile::Pasture { id } = p.farm[i].tile {
                affected.insert(id);
            }
        }
        edges.extend(boundary(region));
    }
    for old in &p.pastures {
        if affected.contains(&old.id) && old.cells.iter().any(|cell| !covered.contains(cell)) {
            return None;
        }
    }
    for pasture in p.pastures.iter().filter(|q| !affected.contains(&q.id)) {
        edges.extend(boundary(&pasture.cells));
    }
    if edges.len() > 15 || !p.fences.is_subset(&edges) {
        return None;
    }
    Some(edges)
}

/// A priced fencing proposal, or `None` if it is unaffordable or over the
/// 15-segment limit.
fn fence_proposal(
    p: &Player,
    budget: u32,
    regions: Vec<Vec<usize>>,
    occupancy: Option<Vec<PastureOccupancy>>,
    kind: &str,
) -> Option<crate::hints::FenceProposal> {
    let edges = fence_edges(p, &regions)?;
    let cost = edges.difference(&p.fences).count() as u32;
    if cost == 0 || cost > budget || p.fences.len() + cost as usize > 15 {
        return None;
    }
    Some(crate::hints::FenceProposal {
        pastures: regions,
        occupancy,
        cost_wood: cost,
        kind: kind.into(),
    })
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
        extra <= (p.supply.wood.saturating_add(p.free_fence_wood)) as usize
            && p.fences.len() + extra <= 15
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
    p.supply
        .pay(Good::Wood, (cost as u32).saturating_sub(p.free_fence_wood))?;
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
            if cost <= (p.supply.wood.saturating_add(p.free_fence_wood)) as usize
                && p.fences.len() + cost <= 15
            {
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
    if player.fences.len() >= 15 || player.supply.wood == 0 && player.free_fence_wood == 0 {
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
            if player.fences.len() + added > 15
                || added > (player.supply.wood.saturating_add(player.free_fence_wood)) as usize
            {
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
            if p.fences.len() + additional > 15
                || additional > (p.supply.wood.saturating_add(p.free_fence_wood)) as usize
            {
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
    fn farm_expansion_can_build_stable_in_existing_pasture() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_pastures(&mut player, &[vec![14]], None).unwrap();
        player.pastures[0].animal = Some(Animal::Sheep);
        player.pastures[0].quantity = 2;
        let pasture_id = player.pastures[0].id;
        let wood_before = player.supply.wood;

        assert!(stable_site_available(&player, 14));
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 2);
        build_stable(&mut player, 14).unwrap();
        assert_eq!(player.supply.wood, wood_before - 2);
        assert_eq!(player.occupied_stables(), 1);
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 4);
        assert_eq!(player.animal_count(Animal::Sheep), 2);
        assert_eq!(player.farm[14].tile, Tile::Pasture { id: pasture_id });

        let after = player.clone();
        assert!(!stable_site_available(&player, 14));
        assert!(build_stable(&mut player, 14).is_err());
        assert_eq!(player, after);
    }

    #[test]
    fn two_pasture_stables_double_capacity_twice_and_survive_split() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        build_pastures(&mut player, &[vec![13, 14]], None).unwrap();
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 4);
        build_stable(&mut player, 13).unwrap();
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 8);
        build_stable(&mut player, 14).unwrap();
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 16);
        player.pastures[0].animal = Some(Animal::Sheep);
        player.pastures[0].quantity = 6;

        build_pastures(
            &mut player,
            &[vec![13], vec![14]],
            Some(&[
                PastureOccupancy {
                    animal: Some(Animal::Sheep),
                    quantity: 3,
                },
                PastureOccupancy {
                    animal: Some(Animal::Sheep),
                    quantity: 3,
                },
            ]),
        )
        .unwrap();

        assert_eq!(player.animal_count(Animal::Sheep), 6);
        assert_eq!(player.pastures.len(), 2);
        for pasture in &player.pastures {
            assert_eq!(player.pasture_capacity(pasture), 4);
            assert_eq!(pasture.quantity, 3);
            assert!(player.farm[pasture.cells[0]].stable);
        }
    }

    #[test]
    fn stable_site_rejects_room_field_and_out_of_bounds() {
        let mut player = Player::new(0, 0);
        player.supply.wood = 20;
        player.farm[14].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        for i in [0, 14, FARM_CELLS] {
            assert!(!stable_site_available(&player, i));
            let before = player.clone();
            assert!(build_stable(&mut player, i).is_err());
            assert_eq!(player, before);
        }
    }

    #[test]
    fn pasture_stable_is_legal_through_action_validation_and_hints() {
        let mut game = Game::new(Config::default());
        let active = game.public.active_player;
        game.public.players[active].supply.wood = 20;
        build_pastures(&mut game.public.players[active], &[vec![14]], None).unwrap();
        game.public.players[active].supply.wood = 2;
        let action = Action {
            space: Space::FarmExpansion,
            detail: Detail::Build {
                rooms: vec![],
                stables: vec![14],
            },
        };

        assert!(game.available_spaces().contains(&Space::FarmExpansion));
        assert!(game
            .detail_hints(Space::FarmExpansion)
            .stable_cells
            .contains(&14));
        game.validate_action(&action).unwrap();
        game.apply(action).unwrap();
        let player = &game.public.players[active];
        assert_eq!(player.supply.wood, 0);
        assert!(player.farm[14].stable);
        assert_eq!(player.pasture_capacity(&player.pastures[0]), 4);
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

#[cfg(test)]
mod m6_passive_engine_tests {
    use super::*;
    use crate::cards::{play, verified_passive_cards, CardSet};
    fn activate(id: &str, p: &mut Player) {
        let set = CardSet {
            definitions: verified_passive_cards(),
        };
        let mut hand = vec![id.into()];
        play(&set, &mut hand, p, id, set.definition(id).unwrap().kind, 0).unwrap();
    }
    #[test]
    fn a012_capacity_with_stable() {
        let mut p = Player::new(0, 0);
        p.supply.clay = 1;
        activate("A012", &mut p);
        p.farm[2].stable = true;
        let pasture = Pasture {
            id: 0,
            cells: vec![2, 3],
            animal: None,
            quantity: 0,
        };
        assert_eq!(p.pasture_capacity(&pasture), 10);
    }
    #[test]
    fn a075_discount_wood_in_major_cost() {
        let mut p = Player::new(0, 0);
        p.supply.stone = 2;
        activate("A075", &mut p);
        assert!(major_cost(&p, Improvement::Well).contains(&(Good::Wood, 0)));
    }
    #[test]
    fn b013_wooden_rooms_only() {
        let mut p = Player::new(0, 0);
        p.supply.stone = 1;
        p.supply.wood = 20;
        p.supply.reed = 4;
        activate("B013", &mut p);
        assert_eq!(room_build_cost(&p), 2);
        let old = p.supply.wood;
        build_room(&mut p, 1).unwrap();
        assert_eq!(p.supply.wood, old - 2);
        p.house = House::Clay;
        assert_eq!(room_build_cost(&p), 5);
    }
    #[test]
    fn a088_fence_discount_per_action() {
        let mut p = Player::new(0, 0);
        activate("A088", &mut p);
        p.supply.wood = 1;
        assert!(single_cell_pasture(&p).is_some());
        build_pastures(&mut p, &[vec![14]], None).unwrap();
        assert_eq!(p.supply.wood, 0);
    }
    #[test]
    fn b095_stone_cost_tracks_new_rooms() {
        let mut p = Player::new(0, 0);
        activate("B095", &mut p);
        p.farm[1].tile = Tile::Room;
        p.farm[6].tile = Tile::Room;
        assert!(major_cost(&p, Improvement::Well).contains(&(Good::Stone, 1)));
    }
    #[test]
    fn b126_clay_room_cost_three() {
        let mut p = Player::new(0, 0);
        activate("B126", &mut p);
        p.house = House::Clay;
        p.supply.clay = 5;
        p.supply.reed = 2;
        assert_eq!(room_build_cost(&p), 3);
        build_room(&mut p, 1).unwrap();
        assert_eq!(p.supply.clay, 2);
    }
}

#[cfg(test)]
mod m6_extra_passive_engine_tests {
    use super::*;
    use crate::cards::{
        play, verified_extra_simple_cards, verified_multiplayer_passive_cards, CardKind, CardSet,
    };
    #[test]
    fn b010_enables_family_growth_without_physical_room() {
        let set = CardSet {
            definitions: verified_extra_simple_cards(),
        };
        let mut p = Player::new(0, 0);
        p.supply.wood = 3;
        p.supply.food = 3;
        assert!(grow(&mut p.clone(), true).is_err());
        play(
            &set,
            &mut vec!["B010".into()],
            &mut p,
            "B010",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        grow(&mut p, true).unwrap();
        assert_eq!(p.family, 3);
        assert_eq!(p.rooms(), 2);
    }
    #[test]
    fn a143_discount_on_major_and_renovation() {
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
        assert!(major_cost(&p, Improvement::Well).contains(&(Good::Stone, 2)));
        p.house = House::Clay;
        p.supply.reed = 1;
        p.supply.stone = 1;
        assert!(can_renovate(&p));
        renovate(&mut p).unwrap();
        assert_eq!(p.house, House::Stone);
    }
}

#[cfg(test)]
mod m6_bake_detection_tests {
    use super::*;
    #[test]
    fn detects_only_real_bake_bread_choices() {
        assert!(!is_bake_bread_detail(&Detail::None));
        assert!(!is_bake_bread_detail(&Detail::Sow {
            plantings: vec![],
            bake: 0
        }));
        assert!(is_bake_bread_detail(&Detail::Sow {
            plantings: vec![],
            bake: 1
        }));
        assert!(is_bake_bread_detail(&Detail::SowDetailed {
            plantings: vec![],
            baking: vec![BakeAllocation {
                improvement: Improvement::ClayOven,
                grain: 1
            }],
        }));
        assert!(!is_bake_bread_detail(&Detail::BuyDetailed {
            improvement: Improvement::ClayOven,
            instant_baking: vec![],
        }));
    }
}

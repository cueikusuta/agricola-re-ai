//! Card-set metadata is supplied by callers; copyrighted card text is never bundled.
//! This is a deliberately small deterministic effect vocabulary, not a claim that
//! the full Agricola Revised deck has been implemented.
use crate::model::{Animal, Crop, Good, House, Player, Space, Tile};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Occupation,
    MinorImprovement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardEffect {
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

    pub fn validate(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
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
    let next_points = player
        .card_points
        .checked_add(card.points)
        .ok_or("card points overflow")?;
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
    hand.retain(|x| x != id);
    player.played_cards.push(id.to_string());
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
    player
        .played_cards
        .iter()
        .filter_map(|id| set.definition(id))
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
                            set.definition(id)
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
                        set.definition(id).is_some_and(|card| {
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
                                set.definition(id)
                                    .is_some_and(|card| card.kind == CardKind::Occupation)
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
            id: "before_occ".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        set.definitions.push(CardDefinition {
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
}

/** Verified deterministic on-play effects for the 2016 A/B starter cards. */
pub fn verified_immediate_resource_cards() -> Vec<CardDefinition> {
    vec![
        CardDefinition {
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
}

/**
 * Verified 2016 base A/B occupations expressible entirely through the worker
 * completion hook. Only effect logic and opaque card IDs are stored here.
 * This subset alone is not a full playable 7+7-card deal.
 */
pub fn verified_worker_bonus_cards(players: usize) -> Vec<CardDefinition> {
    fn occupation(id: &str, effects: Vec<CardEffect>) -> CardDefinition {
        CardDefinition {
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
        assert_eq!(two.len(), 2);
        assert!(!two.iter().any(|c| c.id == "A155" || c.id == "B142"));
        assert_eq!(three.len(), 3);
        assert!(three.iter().any(|c| c.id == "B142"));
        assert!(!three.iter().any(|c| c.id == "A155"));
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
        assert_eq!(set.definitions.len(), 30);
        assert_eq!(
            set.definitions
                .iter()
                .filter(|x| x.kind == CardKind::MinorImprovement)
                .count(),
            16
        );
        assert_eq!(
            set.definitions
                .iter()
                .filter(|x| x.kind == CardKind::Occupation)
                .count(),
            14
        );
        assert!(verified_basic_card_set(3).is_err());
    }
}


pub fn verified_extra_simple_cards() -> Vec<CardDefinition> {
    vec![
        CardDefinition {
            id: "B010".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 3), (Good::Food, 3)],
            points: 0,
            effects: vec![CardEffect::Passive {
                rule: PassiveRule::HousingSpace(1),
            }],
        },
        CardDefinition {
            id: "A063".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 2), (Good::Stone, 2)],
            points: 2,
            effects: vec![CardEffect::AfterHarvestBakeGain { amount: 3 }],
        },
    ]
}
pub fn verified_multiplayer_passive_cards(players:usize) -> Vec<CardDefinition> {
    if players<3{return vec![];}
    vec![CardDefinition {
        id:"A143".into(),kind:CardKind::Occupation,cost:vec![],points:0,
        effects:vec![
            CardEffect::Prerequisite {rule:CardRequirement::MinimumPlayers(3)},
            CardEffect::Passive {rule:PassiveRule::StoneDiscount(1)},
        ],
    }]
}
#[cfg(test)]
mod m6_extra_passive_tests {
    use super::*;
    #[test] fn b010_adds_housing_without_extra_tile() {
        let set=CardSet {definitions:verified_extra_simple_cards()};
        let mut p=Player::new(0,0);p.supply.wood=3;p.supply.food=3;
        play(&set,&mut vec!["B010".into()],&mut p,"B010",CardKind::MinorImprovement,0).unwrap();
        assert_eq!(p.extra_housing,1);assert_eq!(p.rooms(),2);
    }
    #[test] fn a143_only_three_plus_and_reduces_stone() {
        assert!(verified_multiplayer_passive_cards(2).is_empty());
        let set=CardSet {definitions:verified_multiplayer_passive_cards(3)};
        let mut p=Player::new(0,0);
        play(&set,&mut vec!["A143".into()],&mut p,"A143",CardKind::Occupation,0).unwrap();
        assert_eq!(p.stone_cost_discount,1);
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
        let set = CardSet { definitions: verified_extra_simple_cards() };
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
        let set = CardSet { definitions: verified_extra_simple_cards() };
        let mut player = Player::new(0, 0);
        player.supply.wood = 2;
        player.supply.stone = 2;
        let mut hand = vec!["A063".into()];
        play(&set, &mut hand, &mut player, "A063", CardKind::MinorImprovement, 0).unwrap();
        assert_eq!(player.card_points, 2);
        assert_eq!(player.supply.wood, 0);
        assert_eq!(player.supply.stone, 0);
    }
}

//! C-deck band card definitions.
use super::{
    CardDefinition, CardEffect, CardKind, CardRequirement, ChoiceGainOption, CountKind,
    FeedingExchangeOption, HarvestFieldRule,
};
use crate::model::{Animal, Good, Space};

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
        // each later occupation play grants 1 wood per food paid as that
        // occupation's occupation cost (the Lessons/ExtraLessons fee; the
        // occupation's printed individual cost never counts).
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
        // C142 てき屋 (Market Crier): each time the owner uses GrainSeeds, the
        // owner may take 1 grain + 1 vegetable as a bundle; if taken, every
        // other player gains 1 grain. Opt-in as a whole, decided by the owner.
        // Band C card: eligible at 3+ players only (MinimumPlayers(3)),
        // deliberately NOT in verified_basic_card_set.
        card(
            "C142",
            occupation,
            vec![],
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                CardEffect::AfterWorkerOptionalBundleGain {
                    space: Space::GrainSeeds,
                    owner_goods: vec![(Good::Grain, 1), (Good::Vegetable, 1)],
                    others_good: Good::Grain,
                    others_amount: 1,
                },
            ],
        ),
        // C055 アトリエ (Studio): cost 1 clay and 1 reed, 1 printed point; in
        // each feeding phase the owner may convert exactly 1 wood or clay
        // into 2 food, or 1 stone into 3 food (BGA spec C055,
        // harvest_feeding optional exchange).
        CardDefinition {
            use_limit: None,
            id: "C055".to_owned(),
            kind: minor,
            cost: vec![(Good::Clay, 1), (Good::Reed, 1)],
            points: 1,
            effects: vec![CardEffect::HarvestFeedingOptionalExchange {
                options: vec![
                    FeedingExchangeOption {
                        give: vec![(Good::Wood, 1)],
                        take_field_grain: false,
                        receive_food: 2,
                        receive_points: 0,
                        receive_animal: false,
                    },
                    FeedingExchangeOption {
                        give: vec![(Good::Clay, 1)],
                        take_field_grain: false,
                        receive_food: 2,
                        receive_points: 0,
                        receive_animal: false,
                    },
                    FeedingExchangeOption {
                        give: vec![(Good::Stone, 1)],
                        take_field_grain: false,
                        receive_food: 3,
                        receive_points: 0,
                        receive_animal: false,
                    },
                ],
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C059 シュナップス蒸留所 (Schnapps Distillery): cost 2 stone and 1
        // vegetable, 2 printed points; in each feeding phase the owner may
        // cook 1 vegetable into 5 food (BGA spec C059). The end-game scoring
        // half (1 point at 5 vegetables, 2 at 6) is not yet wired to the
        // runtime scorer and is tracked as follow-up work.
        CardDefinition {
            use_limit: None,
            id: "C059".to_owned(),
            kind: minor,
            cost: vec![(Good::Stone, 2), (Good::Vegetable, 1)],
            points: 2,
            effects: vec![CardEffect::HarvestFeedingOptionalExchange {
                options: vec![FeedingExchangeOption {
                    give: vec![(Good::Vegetable, 1)],
                    take_field_grain: false,
                    receive_food: 5,
                    receive_points: 0,
                    receive_animal: false,
                }],
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C063 クラフトビール醸造所 (Craft Brewery): cost 2 wood and 1 clay;
        // in each feeding phase the owner may brew 1 grain from supply and 1
        // grain from a standing grain field into 4 food and 2 points
        // (BGA spec C063).
        CardDefinition {
            use_limit: None,
            id: "C063".to_owned(),
            kind: minor,
            cost: vec![(Good::Wood, 2), (Good::Clay, 1)],
            points: 0,
            effects: vec![CardEffect::HarvestFeedingOptionalExchange {
                options: vec![FeedingExchangeOption {
                    give: vec![(Good::Grain, 1)],
                    take_field_grain: true,
                    receive_food: 4,
                    receive_points: 2,
                    receive_animal: false,
                }],
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C006 石取り作業 (Stone Clearing): cost food1, prerequisite 1 occupation
        // played; on play place 1 stone on each empty field, harvested at next
        // field phase (fields count as cultivated until then). Pass left.
        card(
            "C006",
            CardKind::MinorImprovement,
            vec![(Good::Food, 1)],
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                CardEffect::OnPlayAddStoneToEmptyFields {
                    good: Good::Stone,
                    amount: 1,
                },
                CardEffect::PostPlayPassLeft,
            ],
        ),
        // C007 刈り鋏 (Blade Shears): cost wood1, prerequisite 1 pasture; on
        // play the owner chooses: gain 3 food, or gain 1 food per sheep.
        // Pass left.
        card(
            "C007",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 1)],
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::PastureCount(1),
                },
                CardEffect::OnPlayChoiceGain {
                    options: vec![
                        ChoiceGainOption {
                            good: Good::Food,
                            amount: 3,
                            per_animal: None,
                        },
                        ChoiceGainOption {
                            good: Good::Food,
                            amount: 1,
                            per_animal: Some(Animal::Sheep),
                        },
                    ],
                },
                CardEffect::PostPlayPassLeft,
            ],
        ),
    ]
}
#[cfg(test)]
mod verified_band_c_tests {
    use super::super::*;
    use crate::model::Pasture;

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
        for id in ["C076", "C106", "C111", "C116"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }

    #[test]
    fn c006_costs_one_food_and_has_zero_points() {
        let set = set();
        let card = set.definition("C006").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Food, 1)]);
        assert_eq!(card.points, 0);
    }

    #[test]
    fn c006_requires_one_occupation_and_one_food() {
        let mut set = set();
        let card_id = "C006".to_string();
        // No occupation played → cannot play even with food
        let mut p = Player::new(0, 0);
        p.supply.food = 1;
        let card = set.definition(&card_id).unwrap();
        assert!(!can_play(&set, card, &p, 0));
        // One occupation played → can play
        p.played_cards.push("occ1".into());
        set.definitions.extend([CardDefinition {
            use_limit: None,
            id: "occ1".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }]);
        let card = set.definition(&card_id).unwrap();
        assert!(can_play(&set, card, &p, 0));
    }

    #[test]
    fn c006_places_stone_on_each_empty_field() {
        let mut set = set();
        let card_id = "C006".to_string();
        let mut p = Player::new(0, 0);
        p.played_cards.push("occ1".into());
        set.definitions.extend([CardDefinition {
            use_limit: None,
            id: "occ1".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }]);
        p.supply.food = 1;
        // 2 empty fields + 1 occupied field
        p.farm[0].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[1].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut hand = vec!["C006".to_string()];
        play(
            &set,
            &mut hand,
            &mut p,
            &card_id,
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        // Each empty field gets 1 stone
        assert_eq!(p.farm[0].field_stones, 1);
        assert_eq!(p.farm[1].field_stones, 1);
        // Occupied field gets no stone
        assert_eq!(p.farm[2].field_stones, 0);
    }

    #[test]
    fn c006_zero_empty_fields_places_no_stones() {
        let mut set = set();
        let card_id = "C006".to_string();
        let mut p = Player::new(0, 0);
        p.played_cards.push("occ1".into());
        set.definitions.extend([CardDefinition {
            use_limit: None,
            id: "occ1".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }]);
        p.supply.food = 1;
        // No empty fields
        p.farm[0].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        // All fields occupied — no empty field to place stones on
        let mut hand = vec!["C006".to_string()];
        play(
            &set,
            &mut hand,
            &mut p,
            &card_id,
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        // No empty field means nothing placed
        assert_eq!(p.farm[2].field_stones, 0);
    }

    #[test]
    fn c006_stones_harvested_at_next_field_phase() {
        let set = set();
        let mut p = Player::new(0, 0);
        // 3 stones already on fields
        p.farm[0].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[0].field_stones = 3;
        p.farm[1].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[1].field_stones = 0;
        p.farm[2].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[2].field_stones = 0;
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.stone, 3);
        // Stones consumed after harvest
        assert_eq!(p.farm[0].field_stones, 0);
    }

    #[test]
    fn c007_costs_one_wood_and_has_zero_points() {
        let set = set();
        let card = set.definition("C007").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
    }

    #[test]
    fn c007_requires_one_pasture() {
        let set = set();
        let card = set.definition("C007").unwrap();
        // No pastures → cannot play
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        assert!(!can_play(&set, card, &p, 0));
        // One pasture → can play
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![],
            animal: None,
            quantity: 0,
        });
        assert!(can_play(&set, card, &p, 0));
    }

    #[test]
    fn c007_choice_gain_flat_three_food() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![],
            animal: None,
            quantity: 0,
        });
        p.supply.wood = 1;
        p.pastures.push(Pasture {
            id: 1,
            cells: vec![],
            animal: Some(Animal::Sheep),
            quantity: 1,
        });
        let mut hand = vec!["C007".to_string()];
        play(
            &set,
            &mut hand,
            &mut p,
            "C007",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        // First option (flat) applied: 3 food
        assert_eq!(p.supply.food, 3);
        // Sheep kept
        assert_eq!(p.animal_count(Animal::Sheep), 1);
    }

    #[test]
    fn c007_choice_gain_per_sheep() {
        // Custom card definition with per_sheep option first
        let mut set = set();
        set.definitions.retain(|c| c.id != "C007");
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "C007".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::PastureCount(1),
                },
                CardEffect::OnPlayChoiceGain {
                    options: vec![
                        ChoiceGainOption {
                            good: Good::Food,
                            amount: 1,
                            per_animal: Some(Animal::Sheep),
                        },
                        ChoiceGainOption {
                            good: Good::Food,
                            amount: 3,
                            per_animal: None,
                        },
                    ],
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        let mut p = Player::new(0, 0);
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![],
            animal: None,
            quantity: 0,
        });
        p.supply.wood = 1;
        p.pastures.push(Pasture {
            id: 1,
            cells: vec![],
            animal: Some(Animal::Sheep),
            quantity: 4,
        });
        let mut hand = vec!["C007".to_string()];
        play(
            &set,
            &mut hand,
            &mut p,
            "C007",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        // per_sheep: 1 food per sheep × 4 sheep = 4 food
        assert_eq!(p.supply.food, 4);
        assert_eq!(p.animal_count(Animal::Sheep), 4);
    }

    #[test]
    fn c007_choice_gain_zero_sheep() {
        // Custom card definition with per_sheep option first
        let mut set = set();
        set.definitions.retain(|c| c.id != "C007");
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "C007".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::PastureCount(1),
                },
                CardEffect::OnPlayChoiceGain {
                    options: vec![
                        ChoiceGainOption {
                            good: Good::Food,
                            amount: 1,
                            per_animal: Some(Animal::Sheep),
                        },
                        ChoiceGainOption {
                            good: Good::Food,
                            amount: 3,
                            per_animal: None,
                        },
                    ],
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        let mut p = Player::new(0, 0);
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![],
            animal: None,
            quantity: 0,
        });
        p.supply.wood = 1;
        // No sheep — zero-sheep case
        let mut hand = vec!["C007".to_string()];
        play(
            &set,
            &mut hand,
            &mut p,
            "C007",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        // per_sheep: 1 food per sheep × 0 sheep = 0 food
        assert_eq!(p.supply.food, 0);
        assert_eq!(p.animal_count(Animal::Sheep), 0);
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
    /// Pad the verified band-C pool up to the validated deck size (7 cards
    /// of each kind per player) with inert dummy definitions so the contract
    /// cases can drive the production `Game::apply` path.
    fn production_definitions() -> Vec<CardDefinition> {
        // 2-player fixture: cards whose MinimumPlayers prerequisite exceeds the
        // two seats have no active effects here, so `projected_for_players`
        // would reject them (C142). Same convention as
        // `audit_p2_1_exhaustive_tests::all_registry_cards_driven_through_production_paths`.
        let mut definitions: Vec<CardDefinition> = verified_band_c_cards()
            .into_iter()
            .filter(|c| {
                !c.effects.iter().any(|e| {
                    matches!(
                        e,
                        CardEffect::Prerequisite {
                            rule: CardRequirement::MinimumPlayers(min)
                        } if *min > 2
                    )
                })
            })
            .collect();
        let minors = definitions
            .iter()
            .filter(|c| c.kind == CardKind::MinorImprovement)
            .count();
        for i in 0..(14usize.saturating_sub(minors)) {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("M9X{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            });
        }
        let occupations = definitions
            .iter()
            .filter(|c| c.kind == CardKind::Occupation)
            .count();
        for i in 0..(14usize.saturating_sub(occupations)) {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("O9X{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            });
        }
        definitions
    }

    /// Deal a deterministic, valid hand: seat 0 holds the band-C minors and
    /// occupations it needs (with `focus` injected over a dummy), seat 1
    /// holds dummies.
    fn production_hands(focus: &str) -> Vec<Vec<String>> {
        let definitions = production_definitions();
        let band_minors: Vec<String> = definitions
            .iter()
            .filter(|c| c.kind == CardKind::MinorImprovement && c.id.starts_with('C'))
            .map(|c| c.id.clone())
            .collect();
        let band_occs: Vec<String> = definitions
            .iter()
            .filter(|c| c.kind == CardKind::Occupation && c.id.starts_with('C'))
            .map(|c| c.id.clone())
            .collect();
        let dummy_minors: Vec<String> = definitions
            .iter()
            .filter(|c| c.kind == CardKind::MinorImprovement && c.id.starts_with("M9X"))
            .map(|c| c.id.clone())
            .collect();
        let dummy_occs: Vec<String> = definitions
            .iter()
            .filter(|c| c.kind == CardKind::Occupation && c.id.starts_with("O9X"))
            .map(|c| c.id.clone())
            .collect();

        // Deal the real band-C cards across the seats first (each exactly
        // once), then top every seat up to seven of each kind from the
        // generated dummy pool. Dummy ids are taken from the pool instead of
        // being hard-coded: main keeps adding band-C cards (C083/C147), which
        // shrinks the pool the top-up in `production_definitions` generates.
        let mut minors0: Vec<String> = vec![focus.to_string()];
        let mut minors1: Vec<String> = Vec::new();
        let mut occs0: Vec<String> = Vec::new();
        let mut occs1: Vec<String> = Vec::new();
        let mut minors = band_minors.into_iter().filter(|id| id != focus);
        minors0.extend(minors.by_ref().take(4));
        minors1.extend(minors);
        assert!(
            minors1.len() <= 7,
            "band-C minor pool exceeds the two-seat deal (seat 1 would hold more than seven)"
        );
        let mut occs = band_occs.into_iter();
        occs0.extend(occs.by_ref().take(3));
        occs1.extend(occs);
        assert!(
            occs1.len() <= 7,
            "band-C occupation pool exceeds the two-seat deal (seat 1 would hold more than seven)"
        );
        let mut dummy_minors = dummy_minors.into_iter();
        while minors0.len() < 7 {
            minors0.push(
                dummy_minors
                    .next()
                    .expect("dummy minor pool must top seat 0 up to seven"),
            );
        }
        while minors1.len() < 7 {
            minors1.push(
                dummy_minors
                    .next()
                    .expect("dummy minor pool must top seat 1 up to seven"),
            );
        }
        let mut dummy_occs = dummy_occs.into_iter();
        while occs0.len() < 7 {
            occs0.push(
                dummy_occs
                    .next()
                    .expect("dummy occupation pool must top seat 0 up to seven"),
            );
        }
        while occs1.len() < 7 {
            occs1.push(
                dummy_occs
                    .next()
                    .expect("dummy occupation pool must top seat 1 up to seven"),
            );
        }
        minors0.extend(occs0);
        minors1.extend(occs1);
        assert_eq!(
            minors0.len(),
            14,
            "seat 0 must hold seven minors + seven occupations"
        );
        assert_eq!(
            minors1.len(),
            14,
            "seat 1 must hold seven minors + seven occupations"
        );
        vec![minors0, minors1]
    }

    #[test]
    fn c006_production_path_places_stones_and_passes_left() {
        use crate::engine::{Action, Detail};
        use crate::model::{Config, Game, Space};

        // band_c pool + a played occupation to satisfy the prerequisite.
        let set = CardSet {
            definitions: production_definitions(),
        };
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            production_hands("C006"),
        )
        .unwrap();
        let p = &mut game.public.players[0];
        // Prerequisite: one played occupation (from the dealt hand).
        let occ = game.cards.hands[0]
            .iter()
            .find(|id| {
                game.card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == CardKind::Occupation)
            })
            .cloned()
            .expect("seat 0 holds occupations");
        game.cards.hands[0].retain(|id| id != &occ);
        p.played_cards.push(occ.clone());
        p.supply.food = 1;
        // Contract case C006-normal setup: 2 empty fields + 1 occupied.
        p.farm[0].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[1].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "C006".into(),
            },
        };
        game.validate_action(&action).unwrap();
        game.apply(action).unwrap();

        let owner = &game.public.players[0];
        // Contract: stones_on_fields == 2, occupied field got none.
        assert_eq!(owner.farm[0].field_stones, 1);
        assert_eq!(owner.farm[1].field_stones, 1);
        assert_eq!(owner.farm[2].field_stones, 0);
        // Cost paid: food 1 -> 0.
        assert_eq!(owner.supply.food, 0);
        // Contract: post_play destination = left_neighbor_hand. Two players:
        // the left neighbor of seat 0 is seat 1.
        assert!(
            !owner.played_cards.iter().any(|c| c == "C006"),
            "C006 must not stay in front of the owner"
        );
        assert!(
            game.cards.hands[1].iter().any(|c| c == "C006"),
            "C006 must be in the left neighbor's hand"
        );
        // The occupation stays in front of the owner.
        assert!(owner.played_cards.iter().any(|c| c == &occ));
    }

    #[test]
    fn c006_production_path_harvests_stones_at_field_phase() {
        use crate::model::{Config, Game};

        let set = CardSet {
            definitions: production_definitions(),
        };
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            production_hands("C006"),
        )
        .unwrap();
        let p = &mut game.public.players[0];
        // Contract case C006-harvest setup: 3 stones standing on fields.
        p.farm[0].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[0].field_stones = 3;
        let stone_before = p.supply.stone;
        game.public.round = 4; // a field-phase (harvest) round
        game.harvest_fields().unwrap();
        let p = &game.public.players[0];
        // Contract: owner_stone_delta == 3 and the fields are cleared.
        assert_eq!(p.supply.stone, stone_before + 3);
        assert_eq!(p.farm[0].field_stones, 0);
    }

    #[test]
    fn c007_production_path_choice_flat_and_per_sheep() {
        use crate::engine::{Action, Detail};
        use crate::model::{Config, Game, Pasture, Space};

        let set = CardSet {
            definitions: production_definitions(),
        };
        let hands = production_hands("C007");
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            hands,
        )
        .unwrap();
        {
            let p = &mut game.public.players[0];
            // Contract prerequisites: pasture_count == 1. Build legal fenced
            // pastures on non-adjacent cells 10 and 12.
            p.farm[10].tile = Tile::Pasture { id: 0 };
            p.farm[12].tile = Tile::Pasture { id: 1 };
            p.pastures.push(Pasture {
                id: 0,
                cells: vec![10],
                animal: None,
                quantity: 0,
            });
            p.supply.wood = 1;
            // Contract case C007-flat setup: 1 sheep.
            p.pastures.push(Pasture {
                id: 1,
                cells: vec![12],
                animal: Some(Animal::Sheep),
                quantity: 1,
            });
            let mut fences = crate::model::pasture_border(&[10]);
            fences.extend(crate::model::pasture_border(&[12]));
            p.fences = fences;
        }
        // Plain PlayMinor must fail closed for the choice card.
        let plain = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "C007".into(),
            },
        };
        assert!(game.apply(plain).is_err());

        // Choice index 0 = flat 3 food.
        let food_before = game.public.players[0].supply.food;
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinorChoice {
                card_id: "C007".into(),
                option: 0,
            },
        };
        game.apply(action).unwrap();
        let owner = &game.public.players[0];
        // Contract C007-flat: food_delta == 3, sheep kept.
        assert_eq!(owner.supply.food, food_before + 3);
        assert_eq!(owner.animal_count(Animal::Sheep), 1);
        assert_eq!(owner.supply.wood, 0);
        // Post-play pass.
        assert!(!owner.played_cards.iter().any(|c| c == "C007"));
        assert!(game.cards.hands[1].iter().any(|c| c == "C007"));
    }

    #[test]
    fn c007_production_path_per_sheep_and_zero_sheep() {
        use crate::engine::{Action, Detail};
        use crate::model::{Config, Game, Pasture, Space};

        let set = CardSet {
            definitions: production_definitions(),
        };
        // 4 sheep case: per_sheep option (index 1) grants 4 food.
        let hands = production_hands("C007");
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            hands,
        )
        .unwrap();
        {
            let p = &mut game.public.players[0];
            // One two-cell pasture: capacity 2 cells * 2 = 4 slots for the
            // 4 sheep, and pasture_count == 1 satisfies the prerequisite.
            p.farm[10].tile = Tile::Pasture { id: 0 };
            p.farm[11].tile = Tile::Pasture { id: 0 };
            p.supply.wood = 1;
            p.pastures.push(Pasture {
                id: 0,
                cells: vec![10, 11],
                animal: Some(Animal::Sheep),
                quantity: 4,
            });
            p.fences = crate::model::pasture_border(&[10, 11]);
        }
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinorChoice {
                card_id: "C007".into(),
                option: 1,
            },
        };
        let food_before = game.public.players[0].supply.food;
        game.apply(action).unwrap();
        // Contract C007-per-sheep: food_delta == 4, sheep kept.
        assert_eq!(game.public.players[0].supply.food, food_before + 4);
        assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 4);

        // Out-of-range option fails closed.
        let set = CardSet {
            definitions: production_definitions(),
        };
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            production_hands("C007"),
        )
        .unwrap();
        {
            let p = &mut game.public.players[0];
            p.farm[10].tile = Tile::Pasture { id: 0 };
            p.pastures.push(Pasture {
                id: 0,
                cells: vec![10],
                animal: None,
                quantity: 0,
            });
            p.fences = crate::model::pasture_border(&[10]);
            p.supply.wood = 1;
        }
        let bad = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinorChoice {
                card_id: "C007".into(),
                option: 2,
            },
        };
        assert!(game.apply(bad).is_err());

        // Contract C007-zero-sheep: per_sheep with 0 sheep -> 0 food (playable).
        let set = CardSet {
            definitions: production_definitions(),
        };
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            production_hands("C007"),
        )
        .unwrap();
        {
            let p = &mut game.public.players[0];
            p.farm[10].tile = Tile::Pasture { id: 0 };
            p.pastures.push(Pasture {
                id: 0,
                cells: vec![10],
                animal: None,
                quantity: 0,
            });
            p.fences = crate::model::pasture_border(&[10]);
            p.supply.wood = 1;
        }
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinorChoice {
                card_id: "C007".into(),
                option: 1,
            },
        };
        let food_before = game.public.players[0].supply.food;
        game.apply(action).unwrap();
        assert_eq!(game.public.players[0].supply.food, food_before);
    }

    #[test]
    fn c006_zero_empty_fields_production_path_still_playable() {
        use crate::engine::{Action, Detail};
        use crate::model::{Config, Game, Space};

        let set = CardSet {
            definitions: production_definitions(),
        };
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            production_hands("C006"),
        )
        .unwrap();
        {
            let p = &mut game.public.players[0];
            let occ = game.cards.hands[0]
                .iter()
                .find(|id| {
                    game.card_set
                        .definition(id)
                        .is_some_and(|c| c.kind == CardKind::Occupation)
                })
                .cloned()
                .expect("seat 0 holds occupations");
            game.cards.hands[0].retain(|id| id != &occ);
            p.played_cards.push(occ);
            p.supply.food = 1;
            // Contract case C006-zero-empty: no empty fields (3 occupied).
            p.farm[0].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
            p.farm[1].tile = Tile::Field {
                crop: Some(Crop::Vegetable),
                remaining: 1,
            };
            p.farm[2].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
        }
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "C006".into(),
            },
        };
        // Contract: play_allowed with zero empty fields, stones_on_fields == 0.
        game.apply(action).unwrap();
        let owner = &game.public.players[0];
        assert_eq!(owner.farm[0].field_stones, 0);
        assert_eq!(owner.farm[1].field_stones, 0);
        assert_eq!(owner.farm[2].field_stones, 0);
        // Still consumed cost + passed left.
        assert_eq!(owner.supply.food, 0);
        assert!(!owner.played_cards.iter().any(|c| c == "C006"));
        assert!(game.cards.hands[1].iter().any(|c| c == "C006"));
    }

    #[test]
    fn c006_prerequisite_enforced_on_production_path() {
        use crate::engine::{Action, Detail};
        use crate::model::{Config, Game, Space};

        let set = CardSet {
            definitions: production_definitions(),
        };
        let mut game = Game::with_cards_and_hands(
            Config {
                seed: 7, // round 1 reveals the Major Improvement space
                ..Config::default()
            },
            set,
            production_hands("C006"),
        )
        .unwrap();
        game.public.players[0].supply.food = 1;
        // No occupation played: the contract's prerequisite must reject.
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "C006".into(),
            },
        };
        assert!(game.apply(action).is_err());
        // Also fails without the food cost.
        let mut game = Game::with_cards_and_hands(
            Config::default(),
            CardSet {
                definitions: production_definitions(),
            },
            production_hands("C006"),
        )
        .unwrap();
        game.public.players[0].supply.wood = 1; // wrong good
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "C006".into(),
            },
        };
        assert!(game.apply(action).is_err());
    }
}

//! A-deck band card definitions.
use super::{
    CardDefinition, CardEffect, CardKind, CardRequirement, FeedingExchangeOption, HarvestFieldRule,
    ScoreBonusRule,
};
use crate::model::{Animal, Good, Space};

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
        // A112 大鎌使い (Scythe Worker): gain 1 grain when playing this card;
        // in each field phase gain 1 extra grain per growing grain field (the
        // dispatcher runs before crops are removed, so the count equals the
        // grain the fields yield this phase). Spec: data/bga-card-specs/
        // A112.json (on_play grain 1 + harvest_field_phase grain per grain
        // field), cost/prereq none, 0 points.
        CardDefinition {
            use_limit: None,
            id: "A112".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::OnPlayGain {
                    good: Good::Grain,
                    amount: 1,
                },
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::GrainFieldsGrowing,
                    good: Good::Grain,
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
        // A109 小商人 (Small Trader): when an improvement is played from hand
        // via a Major or Minor Improvement action, gain 3 food.
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
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
        // A140 ショベル運搬人 (Shovel Bearer): after using ClayPit, food equal
        // to the clay left on Hollow; after using Hollow, food equal to the
        // clay left on ClayPit (the printed card pays the OTHER accumulation's
        // clay count; agricola.cloudfree.jp ?p=3285). The amount is dynamic,
        // read from the public accumulations at resolution time. Only
        // available at 3+ players.
        CardDefinition {
            use_limit: None,
            id: "A140".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                CardEffect::AfterWorkerGainOtherAccumulationFood {
                    space: Space::ClayPit,
                    good: Good::Food,
                },
                CardEffect::AfterWorkerGainOtherAccumulationFood {
                    space: Space::Hollow,
                    good: Good::Food,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A066 餌皿 (Feeding Dish): cost 1 wood, no prerequisites, 0 points.
        // After using a sheep/pig/cattle market while already keeping that
        // animal type, gain 1 grain. Snapshot is after_event: animals taken
        // at that same market count. One variant per market fixes the
        // space→animal mapping; the effect targets the card owner only.
        CardDefinition {
            use_limit: None,
            id: "A066".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::AfterWorkerGainIfOwnsAnimal {
                    space: Space::SheepMarket,
                    animal: Animal::Sheep,
                    good: Good::Grain,
                    amount: 1,
                },
                CardEffect::AfterWorkerGainIfOwnsAnimal {
                    space: Space::PigMarket,
                    animal: Animal::Boar,
                    good: Good::Grain,
                    amount: 1,
                },
                CardEffect::AfterWorkerGainIfOwnsAnimal {
                    space: Space::CattleMarket,
                    animal: Animal::Cattle,
                    good: Good::Grain,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A052 手投げ斧 (Throwing Axe): cost 1 wood, no prerequisites, 0 points.
        // After using a wood accumulation space (Forest/Copse/Grove) while the
        // Pig Market holds at least 1 boar, gain 2 food. One effect per space
        // fixes the space gate; the handler reads the animal-market ledger
        // (pig_market) threaded through DispatchContext by the after-worker
        // dispatcher. Available at all player counts.
        CardDefinition {
            use_limit: None,
            id: "A052".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::AfterWorkerGainIfAnimalAccumulationAtLeast {
                    space: Space::Forest,
                    accumulation: Space::PigMarket,
                    min: 1,
                    good: Good::Food,
                    amount: 2,
                },
                CardEffect::AfterWorkerGainIfAnimalAccumulationAtLeast {
                    space: Space::Copse,
                    accumulation: Space::PigMarket,
                    min: 1,
                    good: Good::Food,
                    amount: 2,
                },
                CardEffect::AfterWorkerGainIfAnimalAccumulationAtLeast {
                    space: Space::Grove,
                    accumulation: Space::PigMarket,
                    min: 1,
                    good: Good::Food,
                    amount: 2,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A068 アスパラガスの贈り物 (Asparagus Gift): no cost, no bonus points.
        // Play precondition: at least one empty (plowed, unplanted, stoneless)
        // field. Each time the owner builds a number of fences equal to or
        // greater than the current round, gain 1 vegetable. The printed
        // number is the count built by that fence-building action (not the
        // standing fence total), and the trigger is the fence build itself:
        // Fencing and FarmRedevelopment fence building both fire
        // (spec data/bga-card-specs/A068.json).
        CardDefinition {
            use_limit: None,
            id: "A068".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumEmptyFields(1),
                },
                CardEffect::AfterWorkerGainIfFencesAtLeastRound {
                    space: Space::Fencing,
                    good: Good::Vegetable,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A062 樽詰ビール (Beer Keg): cost 1 wood, 0 printed points; in each
        // feeding phase the owner may exchange 1/2/3 grain for 3 food and
        // 0/1/2 bonus points (BGA spec A062, harvest_feeding optional exchange).
        CardDefinition {
            use_limit: None,
            id: "A062".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::HarvestFeedingOptionalExchange {
                options: vec![
                    FeedingExchangeOption {
                        give: vec![(Good::Grain, 1)],
                        take_field_grain: false,
                        receive_food: 3,
                        receive_points: 0,
                        receive_animal: false,
                    },
                    FeedingExchangeOption {
                        give: vec![(Good::Grain, 2)],
                        take_field_grain: false,
                        receive_food: 3,
                        receive_points: 1,
                        receive_animal: false,
                    },
                    FeedingExchangeOption {
                        give: vec![(Good::Grain, 3)],
                        take_field_grain: false,
                        receive_food: 3,
                        receive_points: 2,
                        receive_animal: false,
                    },
                ],
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

#[cfg(test)]
mod verified_band_a_tests {
    use super::super::*;
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
        for id in ["A099", "A101", "A106", "A117", "A140", "A155"] {
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

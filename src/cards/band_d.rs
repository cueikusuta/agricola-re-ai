//! D-deck band card definitions.
use super::{
    CardDefinition, CardEffect, CardKind, CardRequirement, CountKind, FeedingExchangeOption,
    ScoreBonusRule,
};
use crate::model::{Animal, Good, Space};

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
        // D065 麦ふるい (Grain Sieve): cost 1 wood; during each harvest's field
        // phase, if the grain gained this field phase is at least 2, gain 1
        // more grain from the general supply (BGA spec D065, harvest_field_phase
        // with grain_harvested_at_least n=2 -> gain_resource grain 1).
        card(
            "D065",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 1)],
            vec![CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast {
                threshold: 2,
                good: Good::Grain,
                amount: 1,
            }],
        ),
        // D030 職人地区 (Artisan District): cost 1 stone, prerequisites 3
        // occupations, printed points 1; at scoring, 2/5/8 bonus points when
        // the bottom row of the supply board holds 3/4/5+ major improvements
        // (2 or fewer -> no bonus).
        CardDefinition {
            use_limit: None,
            id: "D030".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Stone, 1)],
            points: 1,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(3),
                },
                CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::MajorImprovementsBottomRowThresholds,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // D035 飼料部屋 (Fodder Chamber): cost 3 stone + 3 grain, printed
        // 2 points; at scoring, 1 bonus point per threshold number of own
        // livestock (2p: 7 animals/point, 3p: 5, 4p: 4). Each player-count
        // run is guarded by Minimum/MaximumPlayers so the game-entry
        // projection keeps exactly one end-game bonus per player count.
        CardDefinition {
            use_limit: None,
            id: "D035".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Stone, 3), (Good::Grain, 3)],
            points: 2,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MaximumPlayers(2),
                },
                CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::LivestockThresholds(7),
                },
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                CardEffect::Prerequisite {
                    rule: CardRequirement::MaximumPlayers(3),
                },
                CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::LivestockThresholds(5),
                },
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::LivestockThresholds(4),
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // D012 搾乳所 (Milking Place): cost 1 grain, 1 printed point;
        // +1 food in the feeding phase of each harvest (BGA spec D012,
        // harvest_feeding gain). The printed passive "no animals in the
        // house" is not modeled (the engine has no house-animal state) and
        // is tracked as follow-up work.
        CardDefinition {
            use_limit: None,
            id: "D012".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Grain, 1)],
            points: 1,
            effects: vec![CardEffect::HarvestFeedingGain {
                good: Some(Good::Food),
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // D062 ビールタップ (Beer Tap): cost 1 wood; +2 food on play; in
        // each feeding phase the owner may exchange exactly 2/3/4 grain for
        // 3/6/9 food (one shared choice, BGA spec D062).
        card(
            "D062",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 1)],
            vec![
                CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 2,
                },
                CardEffect::HarvestFeedingOptionalExchange {
                    options: vec![
                        FeedingExchangeOption {
                            give: vec![(Good::Grain, 2)],
                            take_field_grain: false,
                            receive_food: 3,
                            receive_points: 0,
                            receive_animal: false,
                        },
                        FeedingExchangeOption {
                            give: vec![(Good::Grain, 3)],
                            take_field_grain: false,
                            receive_food: 6,
                            receive_points: 0,
                            receive_animal: false,
                        },
                        FeedingExchangeOption {
                            give: vec![(Good::Grain, 4)],
                            take_field_grain: false,
                            receive_food: 9,
                            receive_points: 0,
                            receive_animal: false,
                        },
                    ],
                },
            ],
        ),
        // D084 飼料ペレット (Feed Pellets): cost 0; +1 sheep on play
        // (placed into an existing pasture/stable); in each feeding phase
        // the owner may convert 1 vegetable into 1 animal of a type already
        // owned (BGA spec D084).
        card(
            "D084",
            CardKind::MinorImprovement,
            vec![],
            vec![
                CardEffect::OnPlayGainAnimal {
                    animal: Animal::Sheep,
                    amount: 1,
                },
                CardEffect::HarvestFeedingOptionalExchange {
                    options: vec![FeedingExchangeOption {
                        give: vec![(Good::Vegetable, 1)],
                        take_field_grain: false,
                        receive_food: 0,
                        receive_points: 0,
                        receive_animal: true,
                    }],
                },
            ],
        ),
    ]
}

#[cfg(test)]
mod verified_band_d_tests {
    use super::super::*;

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
    // Index loops into `p.farm` mirror the farm-grid layout the test asserts on.
    #[allow(clippy::needless_range_loop)]
    fn d100_bonus_point_per_category_at_max_four() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D100".into());
        assert_eq!(end_game_bonus(&set, &p), 0);
        // fields 5 (=4 pts), pastures 4 (=4 pts), fenced stable x4 (=4 pts),
        // grain 8 (=4 pts) -> 4 categories.
        for cell in p.farm.iter_mut().skip(1).take(5) {
            cell.tile = Tile::Field {
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
        for (i, cell) in p.farm.iter_mut().enumerate().skip(10).take(4) {
            cell.stable = true;
            cell.tile = Tile::Pasture { id: (10 + i) as u8 };
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
    // Index loops into `p.farm` mirror the farm-grid layout the test asserts on.
    #[allow(clippy::needless_range_loop)]
    fn d100_three_point_categories_do_not_count() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D100".into());

        // Test fields boundary: 4 fields = 3 pts, 5 fields = 4 pts
        for cell in p.farm.iter_mut().skip(1).take(4) {
            cell.tile = Tile::Field {
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
        for (i, cell) in p4.farm.iter_mut().enumerate().skip(10).take(3) {
            cell.stable = true;
            cell.tile = Tile::Pasture { id: (10 + i) as u8 };
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

    /// Bottom row = Joinery / Pottery / Basketry (same canonical set as
    /// `CountKind::MajorImprovementsBottomRow` / B007), each contributing one
    /// to the count; 3/4/5+ -> 2/5/8, 0-2 -> 0.
    #[test]
    fn d030_thresholds_two_five_eight_for_three_four_five_bottom_row_majors() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D030".into());
        // 0 and 2 bottom-row majors -> no bonus (2 is the exact boundary).
        assert_eq!(end_game_bonus(&set, &p), 0);
        for imp in [Improvement::Joinery, Improvement::Pottery] {
            p.improvements.push(imp);
        }
        assert_eq!(end_game_bonus(&set, &p), 0);
        // 3 -> +2 (spec contract D030-three-majors).
        p.improvements.push(Improvement::Basketry);
        assert_eq!(end_game_bonus(&set, &p), 2);
        // 4 -> +5.
        p.improvements.push(Improvement::Joinery);
        assert_eq!(end_game_bonus(&set, &p), 5);
        // 5 -> +8.
        p.improvements.push(Improvement::Pottery);
        assert_eq!(end_game_bonus(&set, &p), 8);
        // 6 (above the top threshold) still caps at +8.
        p.improvements.push(Improvement::Basketry);
        assert_eq!(end_game_bonus(&set, &p), 8);
    }

    #[test]
    fn d030_less_than_three_bottom_row_majors_no_bonus() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D030".into());
        // spec contract D030-less-than-three.
        p.improvements
            .extend([Improvement::Joinery, Improvement::Pottery]);
        assert_eq!(end_game_bonus(&set, &p), 0);
    }

    #[test]
    fn d030_bonus_counts_only_bottom_row_majors() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D030".into());
        // 3 bottom-row majors -> +2; adding five non-bottom-row majors
        // must not raise the count or the bonus.
        p.improvements.extend([
            Improvement::Joinery,
            Improvement::Pottery,
            Improvement::Basketry,
            Improvement::Fireplace2,
            Improvement::Fireplace3,
            Improvement::Hearth4,
            Improvement::ClayOven,
            Improvement::StoneOven,
        ]);
        assert_eq!(end_game_bonus(&set, &p), 2);
        // Two bottom-row majors mixed with others stays below the threshold.
        let mut q = Player::new(0, 0);
        q.played_cards.push("D030".into());
        q.improvements.extend([
            Improvement::Joinery,
            Improvement::Basketry,
            Improvement::Fireplace2,
            Improvement::ClayOven,
        ]);
        assert_eq!(end_game_bonus(&set, &q), 0);
    }

    #[test]
    fn d030_cost_stone_one_three_occupations_and_printed_points_one() {
        let set = set();
        let card = set.definition("D030").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Stone, 1)]);
        // Printed points 1 live on the definition, separate from the
        // end-game bonus effect.
        assert_eq!(card.points, 1);
        assert!(card.effects.iter().any(|effect| matches!(
            effect,
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumOccupations(3)
            }
        )));
        let mut p = Player::new(0, 0);
        p.supply.stone = 1;
        // Two played occupations (D100/D105) cannot satisfy the 3-occupation
        // prerequisite even though the stone cost is affordable.
        p.played_cards.extend(["D100".into(), "D105".into()]);
        assert!(!can_play(&set, card, &p, 0));
        // A third occupation (D109) opens the play.
        p.played_cards.push("D109".into());
        assert!(can_play(&set, card, &p, 0));
        // Without the stone the cost alone blocks the play.
        p.supply.stone = 0;
        assert!(!can_play(&set, card, &p, 0));
    }

    #[test]
    fn d030_printed_points_and_end_game_bonus_are_separate() {
        let set = set();
        let mut hand = vec!["D030".to_string()];
        let mut p = Player::new(0, 0);
        p.supply.stone = 1;
        // Prerequisite unmet (2 occupations) -> play refuses.
        p.played_cards.extend(["D100".into(), "D105".into()]);
        assert!(play(
            &set,
            &mut hand,
            &mut p,
            "D030",
            CardKind::MinorImprovement,
            0
        )
        .is_err());
        // With the third occupation the play succeeds and adds the printed
        // point immediately; the end-game bonus is still computed separately.
        p.played_cards.push("D109".into());
        play(
            &set,
            &mut hand,
            &mut p,
            "D030",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        assert_eq!(p.card_points, 1);
        assert_eq!(p.supply.stone, 0, "cost stone 1 was paid");
        // No bottom-row major -> no bonus on top of the printed point.
        assert_eq!(end_game_bonus(&set, &p), 0);
        // Three bottom-row majors -> +2 on top of the printed point.
        p.improvements.extend([
            Improvement::Joinery,
            Improvement::Pottery,
            Improvement::Basketry,
        ]);
        assert_eq!(end_game_bonus(&set, &p), 2);
        assert_eq!(
            p.card_points, 1,
            "bonus never joins the card-points counter"
        );
    }

    #[test]
    fn band_d_cards_are_absent_from_the_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        for id in ["D100", "D105", "D109", "D032", "D034", "D065", "D030"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }

    /// D065 spec parity (data/bga-card-specs/D065.json): cost wood 1, printed
    /// points 0, no prerequisites; the single harvest effect carries the
    /// printed threshold 2 and gain grain 1.
    #[test]
    fn d065_cost_and_effect_params_match_the_spec() {
        let card = set()
            .definition("D065")
            .expect("D065 in the band D pool")
            .clone();
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
        assert!(
            !card
                .effects
                .iter()
                .any(|e| matches!(e, CardEffect::Prerequisite { .. })),
            "D065 prints no prerequisites"
        );
        assert!(matches!(
            card.effects.as_slice(),
            [CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast {
                threshold: 2,
                good: Good::Grain,
                amount: 1,
            }]
        ));
        assert_eq!(
            card.effects[0].public_tag(),
            "harvest_field_gain_if_grain_harvested_at_least"
        );
    }

    /// D065 麦ふるい dispatcher contract (D065-normal / D065-boundary): the
    /// pre-harvest count is growing grain FIELDS (remaining >= 1), one grain
    /// per field — a single field with two remaining crops is still one field
    /// and yields one grain, so the printed "at least 2 grain" is NOT met.
    #[test]
    fn d065_grants_extra_grain_only_when_two_fields_are_sown_with_grain() {
        use crate::card_events::dispatch_harvest_field;
        use std::collections::BTreeMap;

        let set = set();
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("D065".into());
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;

        // D065-boundary: one growing grain field with 2 remaining crops yields
        // exactly one grain this phase, so the threshold of 2 is not met.
        players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
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
        assert_eq!(
            players[0].supply.grain, 0,
            "one growing grain field must not reach the printed threshold of 2"
        );

        // D065-normal: the second field makes this phase yield 2 grain, so the
        // card adds exactly 1 more (the dispatcher grants only the card bonus;
        // the field yield itself happens in harvest_fields afterwards).
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
        assert_eq!(players[0].supply.grain, 1, "two fields -> exactly +1 grain");
    }

    /// The legacy `harvest_field_rewards` differential path keeps the same
    /// pre-harvest equivalence: it also runs before crops are taken.
    #[test]
    fn d065_legacy_harvest_field_rewards_match_the_dispatcher() {
        let set = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("D065".into());
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.grain, 0, "one field is below the threshold");
        p.farm[4].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.grain, 1, "two fields -> exactly +1 grain");
    }
}

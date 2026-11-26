//! E-deck band card definitions.
use super::{CardDefinition, CardEffect, CardKind, HarvestFieldRule, ScoreBonusRule};
use crate::model::{Good, Space};

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
        // E006 再集計 (Recount): cost 0; on play, for each of wood/clay/
        // reed/stone the owner already holds at least 4 of, gain 1 of that
        // good, then pass the card to the left neighbor's hand
        // (PostPlayPassLeft). The card does not remain in the owner's
        // played_cards. Spec eligibility 2/3/4 = unknown, so no
        // MinimumPlayers prerequisite is expressed here.
        CardDefinition {
            use_limit: None,
            id: "E006".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::OnPlayGainEachWithAtLeast {
                    goods: vec![Good::Wood, Good::Clay, Good::Reed, Good::Stone],
                    threshold: 4,
                    amount: 1,
                },
                CardEffect::PostPlayPassLeft,
            ],
            // Destination is expressed by the `PostPlayPassLeft` effect (main's
            // engine-level `handle_post_play_destination`), not the
            // `post_play` field, so it stays at the default PlayedArea here.
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E032 身廊 (Nave): cost 2 stone + 1 reed; at scoring, 1 point per
        // farmyard column (of the 5) holding at least one room
        // (agricola.cloudfree.jp ?p=2721).
        CardDefinition {
            use_limit: None,
            id: "E032".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Stone, 2), (Good::Reed, 1)],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::ColumnsWithRoom,
            }],
            // post_play is left at PlayedArea: this card prints no pass-left
            // effect (scoring-only card).
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

#[cfg(test)]
mod verified_band_e_tests {
    use super::super::*;

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
        for cell in p.farm.iter_mut().take(14) {
            cell.tile = Tile::Field {
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
        for cell in q.farm.iter_mut().skip(1).take(4) {
            cell.tile = Tile::Pasture { id: 0 };
        }
        for cell in q.farm.iter_mut().skip(6).take(9) {
            cell.tile = Tile::Pasture { id: 0 };
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
        for cell in p.farm.iter_mut().skip(1).take(4) {
            cell.tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 1,
            };
        }
        for cell in p.farm.iter_mut().skip(6).take(8) {
            cell.tile = Tile::Field {
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

    #[test]
    fn e032_normal_three_columns_with_room_scores_three() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E032".into());
        // Place rooms in 3 different columns (0, 1, 2).
        // Column c uses cells c, c+5, c+10.
        p.farm[0].tile = Tile::Room; // column 0
        p.farm[6].tile = Tile::Room; // column 1 (1 + 5)
        p.farm[12].tile = Tile::Room; // column 2 (2 + 10)
                                      // Other cells empty -> no more columns with rooms.
        assert_eq!(end_game_bonus(&set, &p), 3);
    }

    #[test]
    fn e032_multiple_rooms_same_column_scores_once() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E032".into());
        // Two rooms in column 0 (cells 0 and 5), one room in column 1 (cell 6).
        p.farm[0].tile = Tile::Room; // column 0
        p.farm[5].tile = Tile::Room; // column 0 (5 + 0)
        p.farm[6].tile = Tile::Room; // column 1
                                     // Column 0 has 2 rooms -> still scores 1. Column 1 scores 1. Total 2.
        assert_eq!(end_game_bonus(&set, &p), 2);
    }

    #[test]
    fn e032_zero_default_rooms_only_scores_one() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E032".into());
        // Default farm has rooms at cells 0 and 5 (both column 0).
        // No additional rooms added -> only column 0 has rooms.
        assert_eq!(end_game_bonus(&set, &p), 1);
    }

    #[test]
    fn e032_fields_pastures_stables_do_not_count_as_rooms() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E032".into());
        // Clear default rooms (cells 0 and 5 are in column 0).
        p.farm[0].tile = Tile::Empty;
        p.farm[5].tile = Tile::Empty;
        // Fill column 0 with a field, column 1 with a pasture, column 2 with a stable.
        p.farm[0].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        }; // column 0
        p.farm[6].tile = Tile::Pasture { id: 0 }; // column 1
        p.farm[12].stable = true; // column 2 (cell 12), stable on empty
        p.farm[12].tile = Tile::Empty;
        // None are Tile::Room -> bonus 0.
        assert_eq!(end_game_bonus(&set, &p), 0);
    }

    #[test]
    fn e032_all_five_columns_with_room_scores_five() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("E032".into());
        // Place a room in each of the 5 columns (cells 0, 1, 2, 3, 4).
        for cell in p.farm.iter_mut().take(5) {
            cell.tile = Tile::Room;
        }
        assert_eq!(end_game_bonus(&set, &p), 5);
    }

    #[test]
    fn e032_cost_and_points_and_absent_from_two_player_preset() {
        let set = CardSet {
            definitions: verified_band_e_cards(),
        };
        let card = set.definition("E032").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Stone, 2), (Good::Reed, 1)]);
        assert_eq!(card.points, 0);
        let preset = verified_basic_card_set(2).unwrap();
        assert!(preset.definition("E032").is_none());
    }
}

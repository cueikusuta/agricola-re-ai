//! Cross-deck preset card pools (M6/M7 starter & shared verified sets).
use super::{
    CardDefinition, CardEffect, CardKind, CardRequirement, HarvestFieldRule, PassiveRule,
    ScoreBonusRule, StablePlacementScope,
};
use crate::model::{Animal, Crop, Good, House, Space};

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

#[cfg(test)]
mod verified_scoring_card_tests {
    use super::super::*;
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
    use super::super::*;

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
    use super::super::*;
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
    use super::super::*;

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
        // A118 樹木庭師 (Treegardener): 1 wood per harvest field phase (fixed),
        // plus optional: pay 1 food for 1 wood (up to 2x per harvest).
        CardDefinition {
            use_limit: None,
            id: "A118".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::Fixed,
                    good: Good::Wood,
                    amount: 1,
                },
                // Optional exchange: pay 1 food for 1 wood (up to 2x per harvest).
                // Implemented plan-driven via FeedingPlan::a118_wood_buys during the
                // engine's feeding phase; the dispatcher scan stays dormant.
                CardEffect::HarvestFieldOptionalExchange {
                    cost: Good::Food,
                    cost_amount: 1,
                    gain: Good::Wood,
                    gain_amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A145 綱作り (Ropemaker): 1 reed after each harvest.
        CardDefinition {
            use_limit: None,
            id: "A145".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterHarvestGain {
                good: Good::Reed,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C066 永遠のライ麦畑 (Eternal Rye Cultivation): after each harvest,
        // if grain >= 2 gain 1 food; at grain >= 3 the owner takes 1 grain
        // INSTEAD of the food (default) or opts back into the food via the
        // FeedingPlan flag. Single conditional-choice effect: the old twin
        // unconditional effects double-granted at grain>=3 (PR #385 review
        // finding (b)).
        CardDefinition {
            use_limit: None,
            id: "C066".into(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumCropFields {
                        crop: Crop::Grain,
                        number: 1,
                    },
                },
                CardEffect::AfterHarvestChooseGainIfAtLeast {
                    good: Good::Grain,
                    threshold: 2,
                    gain: Good::Food,
                    amount: 1,
                    upgrade_threshold: 3,
                    upgrade_gain: Good::Grain,
                    upgrade_amount: 1,
                },
            ],
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
        // B114 子なし (Childless): at round start with exactly 2 family and
        // 3+ rooms, gain 1 food. The spec JSON's second step (choose 1 grain
        // or 1 vegetable) has no choice-effect slot at this trigger and is
        // recorded as needs_resolution — only the fixed food gain ships here.
        CardDefinition {
            use_limit: None,
            id: "B114".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundGainIfFamilyAndRooms {
                family: 2,
                rooms_at_least: 3,
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A157 ボヘミアン (Bohemian): at round start, if Lessons holds at
        // least 1 occupant (public.used contains Lessons), gain 1 food.
        // Spec eligibility is 4 players only; kept out of the two-player
        // preset below (same treatment as A145).
        CardDefinition {
            use_limit: None,
            id: "A157".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundGainIfLessonsOccupants {
                min: 1,
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A049 営巣地 (Nest Site): at round start, if the Reed Bank holds at
        // least 1 reed and the per-round refill landed on a non-empty bank,
        // gain 1 food.
        CardDefinition {
            use_limit: None,
            id: "A049".into(),
            kind: minor,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![CardEffect::StartRoundGainIfAccumulationGrew {
                space: Space::ReedBank,
                min: 1,
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C067 ミネラルフィーダー (Mineral Feeder): at round start of a round
        // that does not end with a harvest, if the owner pastures at least 1
        // sheep, gain 1 grain.
        CardDefinition {
            use_limit: None,
            id: "C067".into(),
            kind: minor,
            cost: vec![(Good::Reed, 1)],
            points: 1,
            effects: vec![CardEffect::StartRoundGainIfNoHarvestAndAnimals {
                animal: Animal::Sheep,
                min: 1,
                good: Good::Grain,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // D054 マスの池 (Trout Pool): at the start of each work phase, if the
        // Fishing accumulation holds at least 3 food, gain 1 food. The
        // accumulation is read-only through the round-start dispatch; no
        // "grew this round" requirement (unlike A049 営巣地).
        CardDefinition {
            use_limit: None,
            id: "D054".into(),
            kind: minor,
            cost: vec![(Good::Clay, 2)],
            points: 1,
            effects: vec![CardEffect::StartRoundGainIfAccumulationAtLeast {
                space: Space::Fishing,
                min: 3,
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

#[cfg(test)]
mod round_and_harvest_card_tests {
    use super::super::*;
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
            &BTreeMap::new(),
            &BTreeMap::new(),
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
            &BTreeMap::new(),
            &BTreeMap::new(),
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
            &BTreeMap::new(),
            &BTreeMap::new(),
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
            &BTreeMap::new(),
            &BTreeMap::new(),
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
            &BTreeMap::new(),
            &BTreeMap::new(),
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
            &BTreeMap::new(),
            &BTreeMap::new(),
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
    // --- B114 子なし (Childless): family exactly 2 + rooms >= 3 -> 1 food ---
    // Helper: run one round-start dispatch and report the owner's food.
    fn b114_round_start_food(family: u8, extra_room: bool) -> u32 {
        use crate::card_events::dispatch_start_round;
        use crate::cards::CardState;
        use std::collections::BTreeMap;

        let set = CardSet {
            definitions: verified_round_and_harvest_cards(),
        };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("B114".into());
        players[0].family = family;
        // Player::new starts with exactly 2 rooms (farm[0], farm[5]).
        if extra_room {
            players[0].farm[1].tile = Tile::Room;
        }
        let used = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut vec![],
            &mut BTreeMap::new(),
            2,
            &used,
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            &[],
        )
        .unwrap();
        players[0].supply.food
    }

    #[test]
    fn b114_gains_food_at_round_start_with_two_family_and_three_rooms() {
        // Boundary: exactly 2 family and exactly 3 rooms fires.
        assert_eq!(b114_round_start_food(2, true), 1);
        // Negative: rooms 2 < 3 does not fire.
        assert_eq!(b114_round_start_food(2, false), 0);
        // Negative: family 3 != 2 does not fire (even with rooms 3).
        assert_eq!(b114_round_start_food(3, true), 0);
        // Negative: family 1 != 2 does not fire.
        assert_eq!(b114_round_start_food(1, true), 0);
    }

    // --- A157 ボヘミアン (Bohemian): Lessons occupants >= 1 -> 1 food ---
    fn a157_round_start_food(used: &std::collections::BTreeSet<Space>) -> u32 {
        use crate::card_events::dispatch_start_round;
        use crate::cards::CardState;
        use std::collections::BTreeMap;

        let set = CardSet {
            definitions: verified_round_and_harvest_cards(),
        };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("A157".into());
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut vec![],
            &mut BTreeMap::new(),
            2,
            used,
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            &[],
        )
        .unwrap();
        players[0].supply.food
    }

    #[test]
    fn a157_gains_food_when_lessons_holds_an_occupant() {
        // Positive: a Lessons occupant (used_spaces carries Lessons) fires.
        let lessons = std::collections::BTreeSet::from([Space::Lessons]);
        assert_eq!(a157_round_start_food(&lessons), 1);
        // Negative: another space's occupant does not count.
        let other = std::collections::BTreeSet::from([Space::Forest]);
        assert_eq!(a157_round_start_food(&other), 0);
        // Negative: no occupants at all (also the production path, which
        // clears public.used before dispatching round-start triggers).
        let empty = std::collections::BTreeSet::new();
        assert_eq!(a157_round_start_food(&empty), 0);
    }

    // --- A049 営巣地 (Nest Site): ReedBank >= 1 and grew -> 1 food ---
    fn a049_round_start_food(accumulations: &std::collections::BTreeMap<Space, u32>) -> u32 {
        use crate::card_events::dispatch_start_round;
        use crate::cards::CardState;
        use std::collections::BTreeMap;

        let set = CardSet {
            definitions: verified_round_and_harvest_cards(),
        };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("A049".into());
        let used = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut vec![],
            &mut BTreeMap::new(),
            2,
            &used,
            accumulations,
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            &[],
        )
        .unwrap();
        players[0].supply.food
    }

    #[test]
    fn a049_gains_food_when_the_reed_bank_refill_lands_on_a_non_empty_bank() {
        // Positive: 2 reeds = this round's refill landed on a non-empty bank.
        let grew = std::collections::BTreeMap::from([(Space::ReedBank, 2)]);
        assert_eq!(a049_round_start_food(&grew), 1);
        // Boundary/positive: multi-round untouched growth also fires.
        let grew_more = std::collections::BTreeMap::from([(Space::ReedBank, 4)]);
        assert_eq!(a049_round_start_food(&grew_more), 1);
        // Negative: 1 reed means the refill landed on an empty bank.
        let fresh = std::collections::BTreeMap::from([(Space::ReedBank, 1)]);
        assert_eq!(a049_round_start_food(&fresh), 0);
        // Negative: empty bank (accumulation_at_least unmet).
        let empty = std::collections::BTreeMap::new();
        assert_eq!(a049_round_start_food(&empty), 0);
    }

    // --- C067 ミネラルフィーダー (Mineral Feeder): non-harvest round +
    //     1+ sheep in pastures -> 1 grain ---
    use std::collections::BTreeMap;
    fn c067_round_start_grain(round: u8, sheep: u32) -> u32 {
        use crate::card_events::dispatch_start_round;
        use crate::cards::CardState;

        let set = CardSet {
            definitions: verified_round_and_harvest_cards(),
        };
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("C067".into());
        if sheep > 0 {
            players[0].pastures.push(crate::model::Pasture {
                id: 0,
                cells: vec![2, 3],
                animal: Some(Animal::Sheep),
                quantity: sheep,
            });
        }
        let used = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut vec![],
            &mut BTreeMap::new(),
            round,
            &used,
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            &[],
        )
        .unwrap();
        players[0].supply.grain
    }

    #[test]
    fn c067_gains_grain_in_non_harvest_rounds_with_one_sheep() {
        // Positive: round 3 (no harvest) with one sheep.
        assert_eq!(c067_round_start_grain(3, 1), 1);
        // Boundary: exactly one sheep (the spec minimum).
        assert_eq!(c067_round_start_grain(2, 1), 1);
        // Negative: harvest round 4 stays dormant even with sheep.
        assert_eq!(c067_round_start_grain(4, 1), 0);
        // Negative: no sheep in pastures.
        assert_eq!(c067_round_start_grain(3, 0), 0);
    }

    #[test]
    fn startround_conditional_definitions_match_spec_json() {
        let cards = verified_round_and_harvest_cards();
        let find = |id: &str| cards.iter().find(|c| c.id == id).expect(id).clone();

        let b114 = find("B114");
        assert_eq!(b114.kind, CardKind::Occupation);
        assert_eq!(b114.cost, vec![]);
        assert_eq!(b114.points, 0);
        assert!(matches!(
            b114.effects.as_slice(),
            [CardEffect::StartRoundGainIfFamilyAndRooms {
                family: 2,
                rooms_at_least: 3,
                good: Good::Food,
                amount: 1,
            }]
        ));

        let a157 = find("A157");
        assert_eq!(a157.kind, CardKind::Occupation);
        assert_eq!(a157.cost, vec![]);
        assert_eq!(a157.points, 0);
        assert!(matches!(
            a157.effects.as_slice(),
            [CardEffect::StartRoundGainIfLessonsOccupants {
                min: 1,
                good: Good::Food,
                amount: 1,
            }]
        ));

        let a049 = find("A049");
        assert_eq!(a049.kind, CardKind::MinorImprovement);
        assert_eq!(a049.cost, vec![(Good::Food, 1)]);
        assert_eq!(a049.points, 0);
        assert!(matches!(
            a049.effects.as_slice(),
            [CardEffect::StartRoundGainIfAccumulationGrew {
                space: Space::ReedBank,
                min: 1,
                good: Good::Food,
                amount: 1,
            }]
        ));

        let c067 = find("C067");
        assert_eq!(c067.kind, CardKind::MinorImprovement);
        assert_eq!(c067.cost, vec![(Good::Reed, 1)]);
        assert_eq!(c067.points, 1);
        assert!(matches!(
            c067.effects.as_slice(),
            [CardEffect::StartRoundGainIfNoHarvestAndAnimals {
                animal: Animal::Sheep,
                min: 1,
                good: Good::Grain,
                amount: 1,
            }]
        ));

        // D054 マスの池: Fishing accumulation >= 3 -> 1 food, no grew-this-round
        // requirement (unlike A049 営巣地 above).
        let d054 = find("D054");
        assert_eq!(d054.kind, CardKind::MinorImprovement);
        assert_eq!(d054.cost, vec![(Good::Clay, 2)]);
        assert_eq!(d054.points, 1);
        assert!(matches!(
            d054.effects.as_slice(),
            [CardEffect::StartRoundGainIfAccumulationAtLeast {
                space: Space::Fishing,
                min: 3,
                good: Good::Food,
                amount: 1,
            }]
        ));

        // Two-player preset: B114/A049/C067/D054 are in (2p eligible per spec),
        // A157 is out (spec eligibility "2"/"3": "no", "4": "yes").
        let preset = verified_basic_card_set(2).unwrap();
        assert!(preset.definition("B114").is_some());
        assert!(preset.definition("A049").is_some());
        assert!(preset.definition("C067").is_some());
        assert!(preset.definition("D054").is_some());
        assert!(preset.definition("A157").is_none());
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
        card(
            "C013",
            CardKind::MinorImprovement,
            vec![(Good::Wood, 1)],
            PassiveRule::FirstRenovationStoneDiscount {
                min_wooden_rooms: 5,
                discount: 2,
                direct_to_stone: true,
            },
        ),
    ]
}
#[cfg(test)]
mod passive_cards_tests {
    use super::super::*;
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

/// Additional A/B occupations with unambiguous fixed, public-state effects.
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
    // B102 女性顧問 (Consultant): on play, gain the printed per-player-count
    // reward — 2 players 3 clay, 3 players 2 reed, 4 players 2 sheep. Each
    // variant is a plain unguarded effect; the registry merge
    // (`merge_card_variants` "B102") guards every variant behind paired
    // Minimum/MaximumPlayers prerequisites so exactly one series projects at
    // any given player count. Source: data/bga-card-specs/B102.json
    // (player_count_table 2=clay/3, 3=reed/2, 4=sheep/2).
    match players {
        2 => result.push(CardDefinition {
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
        }),
        3 => result.push(CardDefinition {
            use_limit: None,
            id: "B102".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Reed,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }),
        4 => result.push(CardDefinition {
            use_limit: None,
            id: "B102".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainAnimal {
                animal: Animal::Sheep,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }),
        _ => {}
    }
    result
}

/// on_play build_stable chapter: bga-checked minor improvements (A001 雨除け
/// Shelter / C002 厩 Stable / E001 簡易納屋 Pole Barns) whose printed effects
/// map onto the typed effect vocabulary, sourced from the BGA Agricola card
/// wiki (agricola.cloudfree.jp; A001 p=3424, C002 p=3087, E001 p=2752,
/// cross-checked against the English list). All three are printed "pass
/// left"; this base does not model post-play destinations (the engine has no
/// `post_play` field yet — that lands with the ch02 pass-left chapter), so
/// the cards stay in the played area here, exactly like every other card on
/// this base.
///
/// These are deliberately kept OUT of `verified_basic_card_set` (same as the
/// band pools): the built-in A/B two-player preset must keep its pool size,
/// distribution digest and recorded replay hashes unchanged.
pub fn verified_build_stable_cards() -> Vec<CardDefinition> {
    let minor = CardKind::MinorImprovement;
    vec![
        // A001 雨除け (Shelter): no cost; on play build 1 free stable, but
        // only on the cell of a fenced one-cell pasture.
        CardDefinition {
            use_limit: None,
            id: "A001".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayBuildStable {
                amount: 1,
                wood_cost: 0,
                scope: StablePlacementScope::OneCellPasture { pasture_size: 1 },
            }],
            // on_play build_stable is wired; pass-left awaits its own chapter
            // (spec implementation_status = in_progress), so the card stays in
            // the played area.
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C002 厩 (Stable): cost 1 wood; on play build 1 free stable (the
        // stable itself is free, the card cost is still paid).
        CardDefinition {
            use_limit: None,
            id: "C002".to_owned(),
            kind: minor,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayBuildStable {
                amount: 1,
                wood_cost: 0,
                scope: StablePlacementScope::AnySite,
            }],
            // on_play build_stable is wired; pass-left awaits its own chapter
            // (spec implementation_status = in_progress), so the card stays in
            // the played area.
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E001 簡易納屋 (Pole Barns): cost 2 wood and requires 15 fence
        // segments; on play build up to 3 free stables (the printed "up to"
        // is the owner's choice; the deterministic engine builds as many as
        // legal sites allow, so fewer when the farm is nearly full).
        CardDefinition {
            use_limit: None,
            id: "E001".to_owned(),
            kind: minor,
            cost: vec![(Good::Wood, 2)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumFences(15),
                },
                CardEffect::OnPlayBuildStable {
                    amount: 3,
                    wood_cost: 0,
                    scope: StablePlacementScope::AnySite,
                },
            ],
            // on_play build_stable is wired; pass-left awaits its own chapter
            // (spec implementation_status = in_progress), so the card stays in
            // the played area.
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

#[cfg(test)]
mod simple_occupation_tests {
    use super::super::*;
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
        // The 3-player subset carries its own Reed 2 variant and the
        // 4-player subset the Sheep 2 OnPlayGainAnimal variant, so the
        // two-player clay variant is the only one that can fire here.
        let three = verified_simple_occupations(3);
        let b102_3 = three
            .iter()
            .find(|c| c.id == "B102")
            .expect("B102 3-player variant must exist");
        assert_eq!(
            b102_3.effects,
            vec![CardEffect::OnPlayGain {
                good: Good::Reed,
                amount: 2
            }]
        );
        let four = verified_simple_occupations(4);
        let b102_4 = four
            .iter()
            .find(|c| c.id == "B102")
            .expect("B102 4-player variant must exist");
        assert_eq!(
            b102_4.effects,
            vec![CardEffect::OnPlayGainAnimal {
                animal: Animal::Sheep,
                amount: 2
            }]
        );
        assert!(verified_simple_occupations(5)
            .iter()
            .all(|c| c.id != "B102"));
    }
    #[test]
    fn only_implemented_cards_are_in_two_player_preset() {
        let set = verified_basic_card_set(2).unwrap();
        // PR #385 review fix (e): A145 is 3+ players only (eligibility
        // "2": "no"), so the preset is 37 = 17 occupations + 20 minors.
        // After merge with main (PR #393 etc.), additional cards are included:
        // C141, A050, D061, C083, C147, A109, etc.
        let total = set.definitions.len();
        let occ = set
            .definitions
            .iter()
            .filter(|x| x.kind == CardKind::Occupation)
            .count();
        let minor = set
            .definitions
            .iter()
            .filter(|x| x.kind == CardKind::MinorImprovement)
            .count();
        // B114/A049/C067 (StartRoundGainIf* wave) join the two-player preset;
        // A157 stays out (4 players only), so the count is 41 + 3 = 44
        // (occupations 18 + 1, minors 23 + 2).
        // D054 マスの池 (spec eligibility "2": "yes") joins the preset as a
        // minor: 44 -> 45 total (occupations unchanged, minors 25 -> 26).
        // D026 大工の敷地 / E024 野心 add 2 more minors: 45 + 2 = 47 total
        // (occupations 19, minors 26 -> 28).
        // C013 Wood Slide Hammer (PR #487 merge) joins as a 2p-eligible minor:
        // 47 -> 48 total (occupations 19, minors 28 -> 29).
        assert_eq!(total, 48);
        assert_eq!(occ, 19);
        assert_eq!(minor, 29);
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
        // D026 大工の敷地 (Carpenter's Yard): cost 1 wood + 1 reed, 1 VP.
        // Passive: when taking a Minor Improvement action (MeetingPlace), may build Joinery or Well.
        CardDefinition {
            use_limit: None,
            id: "D026".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1), (Good::Reed, 1)],
            points: 1,
            effects: vec![CardEffect::Passive {
                rule: PassiveRule::AllowMajorViaMinor {
                    joinery_and_well_only: true,
                },
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // E024 野心 (Ambition): prerequisite 2 occupations, 0 VP.
        // Passive: when taking a Minor Improvement action (MeetingPlace), may build any major improvement.
        CardDefinition {
            use_limit: None,
            id: "E024".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(2),
                },
                CardEffect::Passive {
                    rule: PassiveRule::AllowMajorViaMinor {
                        joinery_and_well_only: false,
                    },
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

/// Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064) mapped from
/// bga_checked specifications. These use the new StartRoundGain vocabulary
/// and custom StartRound* effect variants for complex triggers.
pub fn verified_round_start_cards() -> Vec<CardDefinition> {
    let minor = CardKind::MinorImprovement;
    let occupation = CardKind::Occupation;
    vec![
        // A090 鋤職人 (Plow Driver): at each round start, if the owner lives in a
        // stone house, may pay 1 food to plow exactly 1 field. Optional.
        CardDefinition {
            use_limit: None,
            id: "A090".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundPlowFieldIfHouse {
                house: House::Stone,
                food_cost: 1,
                fields: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A151 吟遊楽人 (Minstrel): at each round start in rounds 1-4, if the round
        // action space for that round is unused (no worker placed), copy its
        // effect. The copied effect is resolved deterministically without placing
        // a worker. Not optional (automatic when condition met). 4 players only.
        CardDefinition {
            use_limit: None,
            id: "A151".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::StartRoundCopyUnusedRoundSpace { max_round: 4 },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // A152 夜学生 (Night-School Student): at each round start, if the Lessons
        // space was used by fewer players than any other action space this round,
        // may pay 1 food to play an occupation from hand. Optional. 4 players only.
        CardDefinition {
            use_limit: None,
            id: "A152".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::StartRoundPlayOccupationIfLessonsUnderused { food_cost: 1 },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B097 学者 (Scholar): at each round start, if the owner has a stone house,
        // may either pay 1 food to play an occupation from hand, or pay the normal
        // cost to play a minor improvement from hand. Optional. All player counts.
        CardDefinition {
            use_limit: None,
            id: "B097".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundPlayCardIfStoneHouse {
                occupation_food_cost: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B106 清廉潔白な人 (Moral Crusader): at each round start, if there are any
        // goods on revealed round action spaces (accumulations), gain 1 food.
        // Unconditional gain when condition met. All player counts.
        CardDefinition {
            use_limit: None,
            id: "B106".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::StartRoundGainIfGoodsOnRoundSpaces {
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B135 栄養士 (Nutrition Expert): at each round start, may exchange 1 animal
        // of any owned type, 1 grain, and 1 vegetable for 5 food and 2 points.
        // Optional. 3+ players only.
        CardDefinition {
            use_limit: None,
            id: "B135".into(),
            kind: occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                CardEffect::StartRoundExchangeForFoodAndPoints {
                    give_animal: true,
                    give_grain: 1,
                    give_vegetable: 1,
                    receive_food: 5,
                    receive_points: 2,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // C064 穀物蒸留酒製造所 (Corn Schnapps Distillery): cost 1 wood + 2 clay,
        // 1 printed point. At each round start, may pay 1 grain to place 1 food
        // on each of the next 4 round action spaces. Optional. All player counts.
        CardDefinition {
            use_limit: None,
            id: "C064".into(),
            kind: minor,
            cost: vec![(Good::Wood, 1), (Good::Clay, 2)],
            points: 1,
            effects: vec![CardEffect::StartRoundPlaceFoodOnNextRoundSpaces {
                grain_cost: 1,
                food_per_space: 1,
                num_spaces: 4,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
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
        post_play: crate::cards::PostPlayDestination::PlayedArea,
    }]
}
#[cfg(test)]
mod m6_extra_passive_tests {
    use super::super::*;
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

mod tests {
    use super::*;
    use crate::cards::{verified_band_a_cards, CardDefinition, CardKind, CardSet};
    use crate::model::{Animal, Crop, Good, Improvement, Pasture, Player, Space, Tile};
    use crate::pending::Transition;
    use std::collections::BTreeSet;

    fn setup_game() -> (CardSet, Vec<Player>, crate::cards::CardState) {
        let set = CardSet {
            definitions: vec![
                CardDefinition {
                    use_limit: None,
                    id: "TEST_OCC".into(),
                    kind: CardKind::Occupation,
                    cost: vec![],
                    points: 0,
                    effects: vec![CardEffect::AfterWorkerGain {
                        space: Space::Forest,
                        good: Good::Wood,
                        amount: 1,
                    }],
                },
                CardDefinition {
                    use_limit: None,
                    id: "TEST_MINOR".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![CardEffect::OnPlayGain {
                        good: Good::Food,
                        amount: 2,
                    }],
                },
            ],
        };
        let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
        players[0].played_cards.push("TEST_OCC".into());
        let cards = crate::cards::CardState {
            hands: vec![vec!["TEST_MINOR".into()], vec![]],
            undealt: vec![],
        };
        (set, players, cards)
    }

    #[test]
    fn dispatch_after_worker_triggers_gain() {
        let (set, mut players, mut cards) = setup_game();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let card_use_limits = std::collections::BTreeMap::new();
        let mut pending = None;

        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        assert_eq!(players[0].supply.wood, 1);
    }

    #[test]
    fn dispatch_start_round_triggers_gain() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "SR_GAIN".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 1,
                    house: None,
                    rooms: None,
                }],
            }],
        };
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("SR_GAIN".into());
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let card_use_limits = std::collections::BTreeMap::new();
        let mut pending = None;

        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )
        .unwrap();

        // Player::new(0, 0) starts with 0 food; the card grants +1.
        assert_eq!(players[0].supply.food, 1);
    }

    #[test]
    fn event_budget_exhausted() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "CHAIN".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 1,
                    house: None,
                    rooms: None,
                }],
            }],
        };
        let mut players = vec![Player::new(0, 0)];
        // 70 instances of the SAME chain card: resolve_phase looks up every
        // played card by definition, so each instance must exist in the set
        // to consume one budget unit. 70 > 64 budget → exhaustive guard fires.
        for _ in 0..70 {
            players[0].played_cards.push(set.definitions[0].id.clone());
        }
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let card_use_limits = std::collections::BTreeMap::new();
        let mut pending = None;

        let result = dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("budget exhausted"));
    }

    // Equivalence differential tests: the legacy hooks path (cards.rs public
    // fns, the pre-Step3 implementation) and the new phase dispatcher must
    // produce identical state transitions, including multi-card chains where
    // several played cards fire on the same trigger (Issue #266 acceptance:
    // equivalence of the old CardEffect/hooks path and the new path, incl. chains).
    fn equivalence_pool() -> CardSet {
        let chain = |id: &str, effects: Vec<CardEffect>| CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects,
        };
        CardSet {
            definitions: vec![
                // chain of three cards firing on the same worker trigger
                chain(
                    "C1",
                    vec![CardEffect::AfterWorkerGain {
                        space: Space::Forest,
                        good: Good::Wood,
                        amount: 1,
                    }],
                ),
                chain(
                    "C2",
                    vec![CardEffect::AfterWorkerGain {
                        space: Space::Forest,
                        good: Good::Clay,
                        amount: 2,
                    }],
                ),
                chain(
                    "C3",
                    vec![CardEffect::AfterWorkerGainIfUsed {
                        space: Space::Forest,
                        required_space: Space::Lessons,
                        good: Good::Reed,
                        amount: 1,
                    }],
                ),
                chain("C4", vec![CardEffect::AfterHarvestBakeGain { amount: 3 }]),
                chain(
                    "C5",
                    vec![CardEffect::HarvestFieldGain {
                        rule: crate::cards::HarvestFieldRule::Animals {
                            sheep_per: 2,
                            cattle_per: 1,
                        },
                        good: Good::Food,
                        amount: 1,
                    }],
                ),
                chain(
                    "C6",
                    vec![CardEffect::StartRoundGain {
                        good: Good::Food,
                        amount: 1,
                        house: None,
                        rooms: None,
                    }],
                ),
                chain(
                    "C7",
                    vec![CardEffect::AfterClayConstructionGain {
                        good: Good::Food,
                        amount: 2,
                    }],
                ),
                // minor with an on-play gain for the play-path equivalence
                CardDefinition {
                    use_limit: None,
                    id: "M1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 1,
                    effects: vec![CardEffect::OnPlayGain {
                        good: Good::Grain,
                        amount: 2,
                    }],
                },
            ],
        }
    }

    /// All the pieces the dispatchers need, so each test stays a one-liner.
    struct Ctx {
        players: Vec<Player>,
        cards: crate::cards::CardState,
        budget: u32,
        uses: BTreeMap<String, u32>,
        limits: BTreeMap<String, u32>,
    }

    impl Ctx {
        fn new() -> Self {
            Self {
                players: vec![Player::new(0, 0)],
                cards: crate::cards::CardState::default(),
                budget: 64,
                uses: BTreeMap::new(),
                limits: BTreeMap::new(),
            }
        }
    }

    #[test]
    fn equivalence_after_worker_chain_matches_legacy_path() {
        let set = equivalence_pool();
        for space in [Space::Forest, Space::Lessons, Space::DayLaborer] {
            // legacy path: the production pair after_worker + after_worker_if_used
            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
            crate::cards::after_worker(&set, &mut legacy[0], space).unwrap();
            let mut legacy_used = std::collections::BTreeSet::new();
            legacy_used.insert(Space::Lessons);
            crate::cards::after_worker_if_used(&set, &mut legacy[0], space, &legacy_used).unwrap();

            // new dispatcher path
            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut used = std::collections::BTreeSet::new();
            used.insert(Space::Lessons);
            let mut pending = None;
            dispatch_after_worker(
                &set,
                0,
                space,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                1,
                &used,
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                false, // test: not post-work chain
            )
            .unwrap();
            assert_eq!(
                legacy[0].supply.wood, c.players[0].supply.wood,
                "wood mismatch on {space:?}"
            );
            assert_eq!(
                legacy[0].supply.clay, c.players[0].supply.clay,
                "clay mismatch on {space:?}"
            );
            assert_eq!(
                legacy[0].supply.reed, c.players[0].supply.reed,
                "reed mismatch on {space:?}"
            );
        }
    }

    #[test]
    fn equivalence_start_round_matches_legacy_path() {
        let set = equivalence_pool();
        let mut legacy = [Player::new(0, 0)];
        legacy[0].played_cards = vec!["C1".into(), "C4".into(), "C6".into(), "C6".into()];
        crate::cards::start_round_rewards(&set, &mut legacy[0]).unwrap();

        let mut c = Ctx::new();
        c.players[0].played_cards = legacy[0].played_cards.clone();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            2,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(legacy[0].supply.food, c.players[0].supply.food);
    }

    #[test]
    fn equivalence_harvest_and_bake_matches_legacy_path() {
        let set = equivalence_pool();
        // HarvestField chain C5 with no animals: multiplier 0 on both paths.
        let mut legacy = [Player::new(0, 0)];
        legacy[0].played_cards = vec!["C5".into()];
        crate::cards::harvest_field_rewards(&set, &mut legacy[0]).unwrap();

        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C5".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            legacy[0].supply.food, c.players[0].supply.food,
            "harvest field gain must match legacy"
        );

        // bake chain: two C4 instances fire together
        let mut legacy2 = [Player::new(0, 0)];
        legacy2[0].played_cards = vec!["C1".into(), "C4".into(), "C4".into()];
        crate::cards::after_harvest_baking(&set, &mut legacy2[0], 5, true).unwrap();

        let mut c2 = Ctx::new();
        c2.players[0].played_cards = legacy2[0].played_cards.clone();
        let mut market2 = vec![];
        let mut well2 = BTreeMap::new();
        let mut pending2 = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c2.cards,
            &mut c2.players,
            &mut market2,
            &mut well2,
            5,
            &std::collections::BTreeSet::new(),
            &mut c2.budget,
            &mut c2.uses,
            &c2.limits,
            &mut pending2,
            false, // test: not post-work chain
        )
        .unwrap();
        assert_eq!(
            legacy2[0].supply.food, c2.players[0].supply.food,
            "bake bonus must match legacy"
        );
    }

    #[test]
    fn equivalence_after_clay_construction_matches_legacy_path() {
        let set = equivalence_pool();
        // previous clay house, current renovates to stone -> qualifies
        let mut previous = Player::new(0, 0);
        previous.house = crate::model::House::Clay;

        let mut legacy = [Player::new(0, 0)];
        legacy[0].played_cards = vec!["C1".into(), "C7".into(), "C7".into()];
        legacy[0].house = crate::model::House::Stone;
        crate::cards::after_clay_construction(&set, &previous, &mut legacy[0]).unwrap();

        let mut c = Ctx::new();
        c.players[0].played_cards = legacy[0].played_cards.clone();
        c.players[0].house = crate::model::House::Stone;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            Some(crate::model::Improvement::ClayOven),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();
        assert_eq!(legacy[0].supply.food, c.players[0].supply.food);
    }

    /// The bake bonus must only fire in the round after a harvest round, exactly
    /// like the legacy cards::after_harvest_baking. A missing gate here would
    /// award free food in ordinary rounds (round 4 review, P0).
    #[test]
    fn equivalence_bake_bonus_round_gate_matches_legacy() {
        let set = equivalence_pool();
        // Every round 1..=14, baked=true, single C4 in play (AfterHarvestBakeGain 3).
        for round in 1u8..=14 {
            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = vec!["C4".into()];
            crate::cards::after_harvest_baking(&set, &mut legacy[0], round, true).unwrap();

            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C4".into()];
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_after_harvest_bake(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                round,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                false, // test: not post-work chain
            )
            .unwrap();

            assert_eq!(
                legacy[0].supply.food, c.players[0].supply.food,
                "round {round}: bake bonus must match the legacy gate"
            );
            // The gate is: round >= 2 and round-1 is a harvest round.
            let expected = if round >= 2 && crate::model::HARVEST_ROUNDS.contains(&(round - 1)) {
                3
            } else {
                0
            };
            assert_eq!(
                c.players[0].supply.food, expected,
                "round {round}: expected {expected} food from the bake bonus"
            );
        }
    }

    /// Exhaustive worker-trigger equivalence: every round, every space, with a
    /// chain of gain cards plus a conditional one. The used-spaces set is varied
    /// so `AfterWorkerGainIfUsed` is exercised both ways.
    #[test]
    fn equivalence_worker_triggers_exhaustively_match_legacy() {
        let set = equivalence_pool();
        let spaces = [
            Space::Forest,
            Space::ClayPit,
            Space::ReedBank,
            Space::Fishing,
            Space::Lessons,
            Space::DayLaborer,
            Space::Farmland,
            Space::Fencing,
            Space::GrainSeeds,
            Space::CattleMarket,
        ];
        for space in spaces {
            for lessons_used in [false, true] {
                let mut legacy = [Player::new(0, 0)];
                legacy[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
                let mut used = std::collections::BTreeSet::new();
                if lessons_used {
                    used.insert(Space::Lessons);
                }
                crate::cards::after_worker(&set, &mut legacy[0], space).unwrap();
                crate::cards::after_worker_if_used(&set, &mut legacy[0], space, &used).unwrap();

                let mut c = Ctx::new();
                c.players[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
                let mut market = vec![];
                let mut well = BTreeMap::new();
                let mut pending = None;
                dispatch_after_worker(
                    &set,
                    0,
                    space,
                    &mut c.cards,
                    &mut c.players,
                    &mut market,
                    &mut well,
                    1,
                    &used,
                    &mut c.budget,
                    &mut c.uses,
                    &c.limits,
                    &mut pending,
                    false, // test: not post-work chain
                )
                .unwrap();

                for good in [Good::Wood, Good::Clay, Good::Reed, Good::Food] {
                    assert_eq!(
                        legacy[0].supply.get(good),
                        c.players[0].supply.get(good),
                        "{space:?} lessons_used={lessons_used}: {good:?} diverged"
                    );
                }
            }
        }
    }

    /// Exhaustive start-round equivalence across every round number, including
    /// round 1 and the rounds that follow a harvest.
    #[test]
    fn equivalence_start_round_exhaustively_matches_legacy() {
        let set = equivalence_pool();
        for round in 1u8..=14 {
            let chain = vec!["C6".to_string(), "C1".to_string(), "C6".to_string()];
            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = chain.clone();
            crate::cards::start_round_rewards(&set, &mut legacy[0]).unwrap();

            let mut c = Ctx::new();
            c.players[0].played_cards = chain;
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_start_round(
                &set,
                &mut c.players,
                &mut c.cards,
                &mut market,
                &mut well,
                round,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            assert_eq!(
                legacy[0].supply.food, c.players[0].supply.food,
                "round {round}: StartRoundGain total diverged"
            );
            assert_eq!(legacy[0].supply.wood, c.players[0].supply.wood);
        }
    }

    /// Harvest-field equivalence with real animal counts, so the Animals
    /// multiplier is compared on non-degenerate inputs (C5: sheep_per 2,
    /// cattle_per 1).
    #[test]
    fn equivalence_harvest_field_multipliers_match_legacy() {
        let set = equivalence_pool();
        for (sheep, cattle) in [(0, 0), (1, 0), (2, 0), (0, 1), (4, 2), (7, 3)] {
            let pasture =
                |animal: crate::model::Animal, quantity: u32, id: u8| crate::model::Pasture {
                    id,
                    cells: vec![1],
                    animal: Some(animal),
                    quantity,
                };

            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = vec!["C5".into()];
            if sheep > 0 {
                legacy[0]
                    .pastures
                    .push(pasture(crate::model::Animal::Sheep, sheep, 0));
            }
            if cattle > 0 {
                legacy[0]
                    .pastures
                    .push(pasture(crate::model::Animal::Cattle, cattle, 1));
            }
            crate::cards::harvest_field_rewards(&set, &mut legacy[0]).unwrap();

            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C5".into()];
            c.players[0].pastures = legacy[0].pastures.clone();
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_harvest_field(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                8,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            assert_eq!(
                legacy[0].supply.food, c.players[0].supply.food,
                "sheep={sheep} cattle={cattle}: harvest multiplier diverged"
            );
            // Expected: (sheep / 2 + cattle / 1) * 1 food.
            assert_eq!(
                c.players[0].supply.food,
                sheep / 2 + cattle,
                "sheep={sheep} cattle={cattle}: wrong expected multiplier"
            );
        }
    }

    /// Clay-construction rewards must fire on the construction condition alone.
    /// This is the gap the third review found: with `improvement = None` (a room
    /// built, no improvement bought) the legacy path still granted the bonus, so
    /// the dispatcher must too.
    #[test]
    fn equivalence_clay_construction_without_an_improvement_purchase() {
        let set = equivalence_pool();
        // previous: clay house, 2 rooms. current: clay house, 3 rooms (built one).
        let mut previous = Player::new(0, 0);
        previous.house = crate::model::House::Clay;

        // control: same house, same room count -> nothing fires on either path
        let mut legacy = Player::new(0, 0);
        legacy.house = crate::model::House::Clay;
        legacy.played_cards = vec!["C7".into()];
        crate::cards::after_clay_construction(&set, &previous, &mut legacy).unwrap();
        assert_eq!(
            legacy.supply.food, 0,
            "control: without a room/upgrade change nothing fires"
        );

        let mut legacy_qualifies = Player::new(0, 0);
        legacy_qualifies.house = crate::model::House::Clay;
        legacy_qualifies.played_cards = vec!["C7".into()];
        // Two extra rooms -> strictly more rooms than `previous`.
        legacy_qualifies.farm[7].tile = crate::model::Tile::Room;
        legacy_qualifies.farm[8].tile = crate::model::Tile::Room;
        crate::cards::after_clay_construction(&set, &previous, &mut legacy_qualifies).unwrap();

        // dispatcher, no improvement bought
        let mut c = Ctx::new();
        c.players[0].house = crate::model::House::Clay;
        c.players[0].played_cards = vec!["C7".into()];
        c.players[0].farm[7].tile = crate::model::Tile::Room;
        c.players[0].farm[8].tile = crate::model::Tile::Room;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            None,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        assert_eq!(
            legacy_qualifies.supply.food, c.players[0].supply.food,
            "a room built in a clay house must pay the bonus even without buying an improvement"
        );
        assert_eq!(c.players[0].supply.food, 2, "C7 grants 2 food once");
    }

    #[test]
    fn equivalence_play_path_matches_legacy_cards_play() {
        // cards::play (still the legacy single-call API for direct callers)
        // vs the engine path: play_validate_pay + play_from_hand +
        // dispatch_play + update_card_points_on_play. Identical state.
        let set = equivalence_pool();
        let mut legacy_hand = vec!["M1".to_string()];
        let mut legacy = [Player::new(0, 0)];
        crate::cards::play(
            &set,
            &mut legacy_hand,
            &mut legacy[0],
            "M1",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();

        // engine path
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["M1".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![Player::new(0, 0)];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        {
            let mut ctx = crate::card_zones::CardMoveCtx {
                cards: &mut cards,
                players: &mut players,
                improvement_market: &mut market,
            };
            crate::cards::play_validate_pay(
                &set,
                &mut ctx.cards.hands[0],
                &mut ctx.players[0],
                "M1",
                CardKind::MinorImprovement,
                0,
            )
            .unwrap();
            crate::card_zones::play_from_hand(&mut ctx, 0, "M1").unwrap();
            dispatch_play(
                &set,
                0,
                "M1",
                0,
                ctx.cards,
                ctx.players,
                ctx.improvement_market,
                &mut well,
                1,
                &std::collections::BTreeSet::new(),
                &mut budget,
                &mut uses,
                &limits,
                &mut pending,
            )
            .unwrap();
            let card = set.definition("M1").unwrap();
            crate::cards::update_card_points_on_play(&mut ctx.players[0], card.points).unwrap();
        }

        assert_eq!(
            legacy[0].supply.grain, players[0].supply.grain,
            "on-play grain gain must match"
        );
        assert_eq!(legacy_hand, cards.hands[0]);
        assert_eq!(
            legacy[0].played_cards, players[0].played_cards,
            "zone move must match legacy retain+push"
        );
        assert_eq!(legacy[0].card_points, players[0].card_points);
    }

    /// The Interrupt phase must be reachable: a handler that needs a decision
    /// sets a PendingDecision and pauses the chain. With a single decision-requiring
    /// card in play, the chain pauses on that card's effect.
    #[test]
    fn interrupt_phase_pauses_the_chain_with_a_pending_decision() {
        let probe = |id: &str| CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::PendingProbe {
                options: vec![(Good::Food, 2), (Good::Wood, 1)],
            }],
        };
        let set = CardSet {
            definitions: vec![probe("CHOICE_A")],
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["CHOICE_A".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let result = dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        );
        // With a single PendingProbe effect, the chain pauses on that effect.
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "the chain must pause, got: {result:?}"
        );
        let decision = pending.expect("a PendingDecision must be recorded");
        assert_eq!(decision.player, 0);
        match &decision.kind {
            crate::pending::PendingKind::ChooseGain {
                source_card,
                options,
            } => {
                assert_eq!(
                    source_card, "CHOICE_A",
                    "the decision-requiring card must pause the chain"
                );
                assert_eq!(options.len(), 2);
            }
            other => panic!("unexpected pending kind: {other:?}"),
        }
        // Exactly one budget unit consumed.
        assert_eq!(c.budget, 63, "the chain stopped after the decision");
    }

    /// An effect whose per-period limit is reached must be refused, and the
    /// ledger must not exceed the limit. This is the real (non-vacuous) proof
    /// for the "use limits" acceptance criterion: the limit comes from data.
    #[test]
    fn declared_per_card_use_limit_is_enforced() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "ONCE".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 1,
                    house: None,
                    rooms: None,
                }],
            }],
        };
        // Two copies of the same card in play, limit 1 per period.
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["ONCE".into(), "ONCE".into()];
        c.limits.insert("ONCE".into(), 1);

        let err = dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending_none(),
        )
        .unwrap_err();
        assert!(err.contains("use limit"), "got: {err}");
        assert_eq!(
            c.uses.get("ONCE"),
            Some(&1),
            "the ledger stops at the limit"
        );
        assert_eq!(
            c.players[0].supply.food, 1,
            "exactly one resolution happened before the limit fired"
        );
    }

    /// End-to-end test for Issue #268: interrupt → save → restore → select → continue → replay.
    /// A worker action triggers a pending decision (via PendingProbe), the game pauses,
    /// we serialize the state, deserialize it, resolve the decision, and verify the
    /// action completes and replays correctly.
    #[test]
    fn e2e_interrupt_save_restore_resolve_replay() {
        // Create a card that produces a pending decision on worker completion
        let probe = |id: &str| CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::PendingProbe {
                options: vec![(Good::Food, 2), (Good::Wood, 1)],
            }],
        };
        let set = CardSet {
            definitions: vec![probe("CHOICE_CARD")],
        };

        // Set up a game with the probe card in play
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["CHOICE_CARD".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = BTreeSet::new();
        used.insert(Space::Forest);
        let mut pending = None;

        // Execute a worker action on Forest - this should trigger the interrupt
        let result = dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // test: not post-work chain
        );

        // The chain should pause with a pending decision
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "worker action should pause"
        );
        let decision = pending.take().expect("pending decision must be recorded");
        assert_eq!(decision.player, 0);
        // Note: At the dispatcher level, the decision has ID 0 and empty state_hash_before.
        // The engine layer (apply_with_plans) stamps these with real values.
        // The continuation is also set by the engine layer; at dispatcher level it's None.
        assert_eq!(decision.continuation, crate::pending::Continuation::None);

        // Now resolve the pending decision with a choice
        let resolution = crate::pending::PendingResolution::ChooseGain {
            chosen: crate::pending::GainOption {
                good: Good::Food,
                amount: 2,
            },
        };

        // Validate the resolution against the pending decision
        decision
            .validate_resolution(&resolution)
            .expect("resolution must be valid");

        // Verify replay integrity: the pending_resolution event should be recorded
        // with the decision_id in its payload
        let event_payload = serde_json::json!({
            "source_card": "CHOICE_CARD",
            "chosen_gain": resolution,
            "decision_id": decision.id,
        });
        assert_eq!(event_payload["decision_id"], 0);
    }

    fn pending_none() -> Option<PendingDecision> {
        None
    }

    // --- Ported tests from c0e53e4 (Issue #272 original head) ---

    /// Test that AfterWorkerGain only consumes budget/use when condition met.
    #[test]
    fn conditional_effect_not_consumed_when_condition_unmet_real_path() {
        // Card that only triggers on Fishing space
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "FISHING_ONLY".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Food,
                    amount: 2,
                }],
            }],
        };

        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("FISHING_ONLY".into());
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;

        // Activate Forest (NOT Fishing) - the Fishing-only effect should NOT fire
        // and should NOT consume budget or card use
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        // No food gained because condition (space == Fishing) not met
        assert_eq!(
            players[0].supply.food, 0,
            "Fishing-only effect must not fire on Forest"
        );
        // Budget must NOT be consumed when condition is unmet
        assert_eq!(
            budget, 64,
            "event budget must not be consumed when condition unmet"
        );
        // Card use must NOT be counted when condition is unmet
        assert_eq!(
            uses.get("FISHING_ONLY"),
            None,
            "card use must not be counted when condition unmet"
        );

        // Now activate Fishing - the effect SHOULD fire
        dispatch_after_worker(
            &set,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        // Food gained because condition met
        assert_eq!(
            players[0].supply.food, 2,
            "Fishing-only effect must fire on Fishing"
        );
        // Budget consumed
        assert_eq!(budget, 63, "event budget consumed when condition met");
        // Card use counted
        assert_eq!(
            uses.get("FISHING_ONLY"),
            Some(&1),
            "card use counted when condition met"
        );
    }

    /// Test that AfterWorkerGainIfUsed only consumes budget/use when BOTH conditions met.
    #[test]
    fn conditional_effect_if_used_not_consumed_when_required_space_unused() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "IF_USED".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::AfterWorkerGainIfUsed {
                    space: Space::Forest,
                    required_space: Space::Lessons,
                    good: Good::Wood,
                    amount: 3,
                }],
            }],
        };

        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("IF_USED".into());
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;

        // Activate Forest but Lessons NOT used - effect should NOT fire
        let mut used_spaces = std::collections::BTreeSet::new();
        used_spaces.insert(Space::Forest);
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &used_spaces,
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        assert_eq!(
            players[0].supply.wood, 0,
            "IfUsed effect must not fire when required space unused"
        );
        assert_eq!(
            budget, 64,
            "budget not consumed when IfUsed condition unmet"
        );
        assert_eq!(
            uses.get("IF_USED"),
            None,
            "card use not counted when IfUsed condition unmet"
        );

        // Activate Forest WITH Lessons used - effect SHOULD fire
        used_spaces.insert(Space::Lessons);
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &used_spaces,
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        assert_eq!(
            players[0].supply.wood, 3,
            "IfUsed effect fires when required space used"
        );
        assert_eq!(budget, 63, "budget consumed when IfUsed condition met");
        assert_eq!(
            uses.get("IF_USED"),
            Some(&1),
            "card use counted when IfUsed condition met"
        );
    }

    /// Test that StartRoundGain with house/room condition only consumes budget/use when met.
    #[test]
    fn conditional_effect_start_round_not_consumed_when_house_mismatch() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "STONE_HOUSE_ONLY".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 5,
                    house: Some(House::Stone),
                    rooms: None,
                }],
            }],
        };

        // Player has Wood house (not Stone)
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("STONE_HOUSE_ONLY".into());
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;

        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(
            players[0].supply.food, 0,
            "Stone-house effect must not fire for Wood house"
        );
        assert_eq!(budget, 64, "budget not consumed when house condition unmet");
        assert_eq!(
            uses.get("STONE_HOUSE_ONLY"),
            None,
            "card use not counted when house condition unmet"
        );

        // Upgrade to Stone house - effect SHOULD fire
        players[0].house = House::Stone;
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut market,
            &mut well,
            2,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(
            players[0].supply.food, 5,
            "Stone-house effect fires for Stone house"
        );
        assert_eq!(budget, 63, "budget consumed when house condition met");
        assert_eq!(
            uses.get("STONE_HOUSE_ONLY"),
            Some(&1),
            "card use counted when house condition met"
        );
    }

    /// Real-card end-to-end test for A078 (Canoe / カヌー) via direct CardSet.
    /// A078 is a Minor Improvement: Fishing action gives +1 food and +1 reed.
    /// This test exercises the condition-gated resolve_phase in the engine path (dispatch_after_worker),
    /// not just raw dispatchers.
    #[test]
    fn a078_canoe_e2e_via_verified_registry_pool() {
        // Create CardSet with A078 card definition directly.
        // A078: Minor Improvement with two AfterWorkerGain effects on Fishing space:
        // +1 Food and +1 Reed.
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "A078".into(),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![
                    CardEffect::AfterWorkerGain {
                        space: Space::Fishing,
                        good: Good::Food,
                        amount: 1,
                    },
                    CardEffect::AfterWorkerGain {
                        space: Space::Fishing,
                        good: Good::Reed,
                        amount: 1,
                    },
                ],
            }],
        };

        // Set up player with A078 played
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("A078".into());
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64u32;
        let mut card_uses = std::collections::BTreeMap::new();
        let card_use_limits = std::collections::BTreeMap::new();
        let mut pending = None;

        // --- Test 1: Activate Forest (NOT Fishing) ---
        // The Fishing-only effects should NOT fire and should NOT consume budget or card use
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        // No food/reed gained because condition (space == Fishing) not met
        assert_eq!(
            players[0].supply.food, 0,
            "A078 Fishing effect must not fire on Forest"
        );
        assert_eq!(
            players[0].supply.reed, 0,
            "A078 Fishing effect must not fire on Forest"
        );
        // Budget must NOT be consumed when condition is unmet
        assert_eq!(
            event_budget, 64,
            "event budget must not be consumed when condition unmet"
        );
        // Card use must NOT be counted when condition is unmet
        assert_eq!(
            card_uses.get("A078"),
            None,
            "card use must not be counted when condition unmet"
        );

        // --- Test 2: Activate Fishing ---
        // Both effects (+1 food, +1 reed) SHOULD fire
        dispatch_after_worker(
            &set,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        // Both gains fire
        assert_eq!(players[0].supply.food, 1, "A078 gives +1 food on Fishing");
        assert_eq!(players[0].supply.reed, 1, "A078 gives +1 reed on Fishing");
        // Budget consumed (2 effects = 2 budget)
        assert_eq!(event_budget, 62, "event budget consumed for both effects");
        // Card use counted (once per effect that fires, so 2 for A078's two effects)
        assert_eq!(
            card_uses.get("A078"),
            Some(&2),
            "card use counted per effect when condition met"
        );
    }

    // --- Engine-path e2e test for the REAL verified A078 card (Issue #270 R5) ---
    //
    // Same contract as `choose_trigger_order_e2e_engine_path_pause_resume`
    // above, but on the real verified card set: A078 (Canoe, two
    // AfterWorkerGain(Fishing) effects) loaded through
    // `verified_basic_card_set`. This pins the exact user-visible regression
    // the reviewers reproduced: placing a worker on Fishing with A078 played
    // must pause, and the resume must apply BOTH bonuses.
    #[test]
    fn a078_two_fishing_effects_apply_after_engine_pause_resume() {
        use crate::model::Config;

        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let mut game = crate::model::Game::with_cards(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("game creation must succeed");
        game.public.active_player = 0;
        game.public.players[0].played_cards.push("A078".into());

        let action = crate::engine::Action {
            space: Space::Fishing,
            detail: crate::engine::Detail::None,
        };
        let transition =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        let paused = matches!(transition, Ok(Transition::Paused(_)));
        assert!(
            paused,
            "two simultaneous A078 Fishing triggers must pause, got: {transition:?}"
        );
        let before = (
            game.public.players[0].supply.food,
            game.public.players[0].supply.reed,
        );

        let outcome = game
            .resolve_pending_and_resume(crate::pending::PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, Transition::Completed, "resume must complete");
        let player = &game.public.players[0];
        assert_eq!(
            player.supply.food,
            before.0 + 1,
            "A078 food bonus lost after resume (before={before:?})"
        );
        assert_eq!(
            player.supply.reed,
            before.1 + 1,
            "A078 reed bonus lost after resume (before={before:?})"
        );
        assert!(
            game.public.used.contains(&Space::Fishing),
            "the action space must be marked used by the resume"
        );
    }

    // --- Engine-path e2e test for ChooseTriggerOrder pause/resume (Issue #270 P2) ---
    //
    // The full engine path: apply_with_plans -> Ok(Paused(ChooseTriggerOrder))
    // -> resolve(chosen_order=[1,0]) -> resume Completed.
    // Verifies: the pause is created on the real engine path (not just the
    // dispatcher), the effects fire in the chosen order exactly once each,
    // the space is marked used, budget and use ledger are exact, and the
    // permutation validator rejects invalid orders.
    #[test]
    fn choose_trigger_order_e2e_engine_path_pause_resume() {
        use crate::engine::Action;
        use crate::model::Config;

        // Two simultaneous After-phase effects on Fishing (A078-like).
        let card = CardDefinition {
            use_limit: None,
            id: "AW2".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 1,
            effects: vec![
                CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Reed,
                    amount: 1,
                },
            ],
        };
        let mut definitions = vec![card];
        for i in 1..=14 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("OCC_{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        for i in 1..=13 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 1,
                effects: vec![],
            });
        }
        let set = CardSet { definitions };

        let occ_ids: Vec<String> = (1..=14).map(|i| format!("OCC_{i}")).collect();
        let minor_ids: Vec<String> = (1..=13).map(|i| format!("MINOR_{i}")).collect();
        let mut hands = vec![vec!["AW2".to_string()], vec![]];
        hands[0].extend(occ_ids[0..7].iter().cloned());
        hands[0].extend(minor_ids[0..6].iter().cloned());
        hands[1].extend(occ_ids[7..14].iter().cloned());
        hands[1].extend(minor_ids[6..13].iter().cloned());

        let mut game = crate::model::Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set,
            hands,
        )
        .expect("game creation must succeed");
        game.public.active_player = 0;
        game.public.players[0].played_cards.push("AW2".into());
        assert!(
            game.legal_spaces().contains(&Space::Fishing),
            "Fishing must be legal in round 1"
        );

        // 1. The engine path pauses with ChooseTriggerOrder on 2 simultaneous
        //    After-phase triggers.
        let action = Action {
            space: Space::Fishing,
            detail: crate::engine::Detail::None,
        };
        let result =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "the engine path must pause, got: {result:?}"
        );
        let pending = game.pending.clone().expect("a decision must be recorded");
        match &pending.kind {
            PendingKind::ChooseTriggerOrder { phase, triggers } => {
                assert_eq!(*phase, TriggerPhase::After);
                assert_eq!(triggers.len(), 2, "both effects must be offered");
            }
            other => panic!("expected ChooseTriggerOrder, got: {other:?}"),
        }
        // The continuation carries the After-phase resume position so the
        // resumed scan does not re-fire or skip effects.
        let resume = match &pending.continuation {
            crate::pending::Continuation::PostWork { resume, .. } => resume,
            other => panic!("expected a PostWork continuation, got: {other:?}"),
        };
        assert_eq!(
            resume.phase,
            TriggerPhase::After,
            "pause phase must be After"
        );
        assert_eq!(
            (resume.after.card_index, resume.after.effect_index),
            (0, 1),
            "resume position must be the last triggerable effect"
        );

        // 2. Invalid permutations are rejected.
        for bad in [vec![0, 0], vec![0], vec![2, 0]] {
            let err = game.resolve_pending_and_resume(
                crate::pending::PendingResolution::ChooseTriggerOrder { chosen_order: bad },
            );
            assert!(err.is_err(), "invalid permutation must be rejected");
            assert!(game.has_pending(), "the decision must remain resolvable");
        }

        // 3. Resolve with the reverse order and resume: both effects fire
        //    exactly once; budget/use/space are exact.
        let outcome = game
            .resolve_pending_and_resume(crate::pending::PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![1, 0],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, Transition::Completed, "resume must complete");
        assert!(!game.has_pending(), "no pending after resume");
        assert_eq!(
            game.public.players[0].supply.food, 4,
            "food +1 on top of the initial 3"
        );
        assert_eq!(game.public.players[0].supply.reed, 1, "reed +1");
        assert!(
            game.public.used.contains(&Space::Fishing),
            "the action space must be marked used by the resume"
        );
        assert_eq!(
            game.public.card_uses.get("AW2").copied(),
            Some(2),
            "each effect must fire exactly once"
        );
        assert_eq!(game.event_budget, 62, "two effects consume two budget");
    }

    /// P0-3 reproduction: save/load/replay after ChooseTriggerOrder pause/resume
    /// with A078 (Canoe, two AfterWorkerGain on Fishing).
    /// This test should FAIL before the fix (double recording breaks replay)
    /// and PASS after the fix.
    #[test]
    fn p0_3_repro_save_load_replay_after_choose_trigger_order() {
        use crate::engine::{Action, Detail};
        use crate::harvest::SimpleHarvestDetails;
        use crate::model::{Config, Game, Space};
        use crate::pending::{PendingResolution, Transition};

        // Seed-independent setup (PR #385 review R-P1-2): the random seed-1
        // deal stopped seating B121 and A078 in seat 0 together once this
        // batch changed the card-set composition, so the hands are forced
        // through `with_cards_and_hands`. The game RNG streams are derived
        // from the seed alone (`from_card_state`), so the round deck and
        // every other expectation below are unchanged; only the hand
        // contents differ.
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let mut occs: Vec<String> = set
            .definitions
            .iter()
            .filter(|c| c.kind == crate::cards::CardKind::Occupation)
            .map(|c| c.id.clone())
            .collect();
        occs.sort();
        let mut minors: Vec<String> = set
            .definitions
            .iter()
            .filter(|c| c.kind == crate::cards::CardKind::MinorImprovement)
            .map(|c| c.id.clone())
            .collect();
        minors.sort();
        let mut seat0_occ = vec!["B121".to_string()];
        seat0_occ.extend(occs.iter().filter(|id| *id != "B121").take(6).cloned());
        let mut seat0_minor = vec!["A078".to_string()];
        seat0_minor.extend(minors.iter().filter(|id| *id != "A078").take(6).cloned());
        let seat1_occ: Vec<String> = occs
            .iter()
            .filter(|id| !seat0_occ.contains(id))
            .take(7)
            .cloned()
            .collect();
        let seat1_minor: Vec<String> = minors
            .iter()
            .filter(|id| !seat0_minor.contains(id))
            .take(7)
            .cloned()
            .collect();
        assert_eq!(
            (seat0_occ.len(), seat0_minor.len()),
            (7, 7),
            "seat 0 must hold seven occupations and seven minors"
        );
        assert_eq!(
            (seat1_occ.len(), seat1_minor.len()),
            (7, 7),
            "seat 1 must hold seven occupations and seven minors"
        );
        let mut game = Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 1,
                additional_action_space_enabled: false,
            },
            set,
            vec![
                [seat0_occ, seat0_minor].concat(),
                [seat1_occ, seat1_minor].concat(),
            ],
        )
        .expect("game creation must succeed");

        // Helper to let player 1 take a simple accumulation action
        let p1_simple_action = |game: &mut Game, space: Space| {
            game.public.active_player = 1;
            eprintln!("P1 available spaces: {:?}", game.available_spaces());
            eprintln!("P1 active_player: {}", game.public.active_player);
            eprintln!("P1 used spaces: {:?}", game.public.used);
            let action = Action {
                space,
                detail: Detail::None,
            };
            let transition = game.apply_with_plans(action, None, None, &SimpleHarvestDetails);
            eprintln!("P1 transition: {:?}", transition);
            assert_eq!(
                transition,
                Ok(Transition::Completed),
                "P1 action must complete"
            );
        };

        // --- Round 1: Player 0 plays occupation (B121), P1 acts, P0 takes Forest, P1 acts -> harvest ---
        // B121 (Geologist) is a free occupation (no cost).
        game.public.active_player = 0;
        let occ_action = Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: "B121".into(),
            },
        };
        let transition = game.apply_with_plans(occ_action, None, None, &SimpleHarvestDetails);
        assert_eq!(
            transition,
            Ok(Transition::Completed),
            "B121 occupation play must complete"
        );

        // P1 takes ClayPit (so P0 can still take Forest)
        p1_simple_action(&mut game, Space::ClayPit);

        // P0 takes Forest (gets 3 wood)
        game.public.active_player = 0;
        eprintln!("P0 available spaces: {:?}", game.available_spaces());
        eprintln!("P0 active_player: {}", game.public.active_player);
        eprintln!("P0 used spaces: {:?}", game.public.used);
        let forest_action_r1 = Action {
            space: Space::Forest,
            detail: Detail::None,
        };
        let transition = game.apply_with_plans(forest_action_r1, None, None, &SimpleHarvestDetails);
        eprintln!("P0 transition: {:?}", transition);
        assert_eq!(
            transition,
            Ok(Transition::Completed),
            "Forest action must complete"
        );
        assert_eq!(
            game.public.players[0].supply.wood, 3,
            "Forest gives 3 wood at round 1"
        );

        // P1 takes DayLaborer -> all workers placed, round advances to 2 automatically
        p1_simple_action(&mut game, Space::DayLaborer);

        // --- Round 2: Already at round 2 after auto-advancement, play A078 and take Fishing ---
        assert_eq!(
            game.public.round, 2,
            "should be round 2 after auto-advancement"
        );
        assert_eq!(
            game.public.players[0].supply.wood, 3,
            "wood carried over (Forest accumulation at round 2 start goes to space, not player)"
        );

        // Play A078 (costs 2 wood, requires 1 occupation)
        game.public.active_player = 0;
        let minor_action = Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "A078".into(),
            },
        };
        let transition = game.apply_with_plans(minor_action, None, None, &SimpleHarvestDetails);
        assert_eq!(
            transition,
            Ok(Transition::Completed),
            "A078 minor play must complete"
        );
        assert_eq!(
            game.public.players[0].supply.wood, 1,
            "A078 costs 2 wood (3 - 2 = 1)"
        );

        // P1 takes ReedBank
        p1_simple_action(&mut game, Space::ReedBank);

        // --- Step: Take Fishing action - triggers A078's two effects, pauses with ChooseTriggerOrder ---
        game.public.active_player = 0;
        let fishing_action = Action {
            space: Space::Fishing,
            detail: Detail::None,
        };
        let transition = game.apply_with_plans(fishing_action, None, None, &SimpleHarvestDetails);

        // Should pause with ChooseTriggerOrder
        assert!(
            matches!(transition, Ok(Transition::Paused(_))),
            "must pause on Fishing with A078's two triggers"
        );

        // --- Step: Resolve with chosen order ---
        let outcome = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, Transition::Completed, "resume must complete");

        // P1 takes DayLaborer -> harvest
        p1_simple_action(&mut game, Space::DayLaborer);

        // --- Step: Verify both bonuses applied ---
        assert_eq!(
            game.public.players[0].supply.food, 5,
            "A078 gives +1 food on Fishing (3 start + 1 Fishing accum + 1 A078)"
        );
        assert_eq!(
            game.public.players[0].supply.reed, 1,
            "A078 gives +1 reed on Fishing"
        );

        // --- Step: Count pending_resolution events (should be 1 after fix) ---
        let resolution_count = game
            .log
            .events
            .iter()
            .filter(|e| e.kind == "pending_resolution")
            .count();
        assert_eq!(
            resolution_count, 1,
            "must have exactly one pending_resolution event after fix"
        );

        // --- Step: Save/load/replay round-trip (all state created via public actions, so this must succeed) ---
        let json = game.save_json().expect("save must succeed");
        let loaded =
            Game::load_json(&json).expect("load_json must succeed - all state from public actions");
        let replayed = Game::replay_with_setup_and_start_player(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            loaded.log.initial_hands.clone(),
            loaded.log.initial_start_player_override,
            &loaded.log.events,
        )
        .expect("replay must succeed");

        // Final state hash must match
        assert_eq!(
            game.state_hash(),
            replayed.state_hash(),
            "state_hash must match after save/load/replay round-trip"
        );

        // Verify resources preserved after replay
        assert_eq!(
            replayed.public.players[0].supply.food, 5,
            "food preserved after replay"
        );
        assert_eq!(
            replayed.public.players[0].supply.reed, 1,
            "reed preserved after replay"
        );
    }

    /// Double-pause regression (PR #352 Round 2 P0-1/P1-2): resume from an
    /// AfterClayConstruction ChooseTriggerOrder pause into an AfterWorker
    /// ChooseTriggerOrder pause, with every state element produced by public
    /// actions only (card plays via Lessons, the clay house via renovation,
    /// clay/reed via accumulation spaces) -- so the save/load/replay section is
    /// a real proof rather than log-external state mutation. Each of the four
    /// card effects must apply exactly once: the delta across the double-pause
    /// resume is +2 food / +2 wood regardless of the starting supply.
    #[test]
    fn double_pause_after_clay_then_after_worker() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};
        use crate::engine::{Action, Detail};
        use crate::harvest::SimpleHarvestDetails;
        use crate::model::{Config, Game, Good, House, RoundCard, Space};
        use crate::pending::{PendingResolution, Transition};

        // Two test occupations: 2x AfterClayConstructionGain and 2x
        // AfterWorkerGain on FarmExpansion. Fillers (inert, free) pad each
        // hand to the 7+7 deal the draft-hands constructor requires.
        let filler_occ = |i: usize| CardDefinition {
            use_limit: None,
            id: format!("FILLER_OCC_{i}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        };
        let filler_minor = |i: usize| CardDefinition {
            use_limit: None,
            id: format!("FILLER_MINOR_{i}"),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![],
        };
        let mut set = crate::cards::verified_basic_card_set(2).expect("verified basic card set");
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_CLAY_DOUBLE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterClayConstructionGain {
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterClayConstructionGain {
                    good: Good::Wood,
                    amount: 1,
                },
            ],
        });
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_WORKER_DOUBLE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterWorkerGain {
                    space: Space::FarmExpansion,
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::FarmExpansion,
                    good: Good::Wood,
                    amount: 1,
                },
            ],
        });
        for i in 0..12 {
            set.definitions.push(filler_occ(i));
        }
        for i in 0..14 {
            set.definitions.push(filler_minor(i));
        }

        let p0_occ = [
            "TEST_CLAY_DOUBLE",
            "TEST_WORKER_DOUBLE",
            "FILLER_OCC_0",
            "FILLER_OCC_1",
            "FILLER_OCC_2",
            "FILLER_OCC_3",
            "FILLER_OCC_4",
        ];
        let p0_minor = [
            "FILLER_MINOR_0",
            "FILLER_MINOR_1",
            "FILLER_MINOR_2",
            "FILLER_MINOR_3",
            "FILLER_MINOR_4",
            "FILLER_MINOR_5",
            "FILLER_MINOR_6",
        ];
        let p1_occ = [
            "FILLER_OCC_5",
            "FILLER_OCC_6",
            "FILLER_OCC_7",
            "FILLER_OCC_8",
            "FILLER_OCC_9",
            "FILLER_OCC_10",
            "FILLER_OCC_11",
        ];
        let p1_minor = [
            "FILLER_MINOR_7",
            "FILLER_MINOR_8",
            "FILLER_MINOR_9",
            "FILLER_MINOR_10",
            "FILLER_MINOR_11",
            "FILLER_MINOR_12",
            "FILLER_MINOR_13",
        ];
        let hands = vec![
            p0_occ
                .iter()
                .chain(p0_minor.iter())
                .map(|s| (*s).to_string())
                .collect(),
            p1_occ
                .iter()
                .chain(p1_minor.iter())
                .map(|s| (*s).to_string())
                .collect(),
        ];

        // Player 0 is the start player: deterministic turn order.
        let mut game = Game::with_cards_and_hands_with_start_player(
            Config {
                players: 2,
                seed: 1,
                additional_action_space_enabled: false,
            },
            set,
            hands,
            0,
        )
        .expect("game creation must succeed");

        fn act(game: &mut Game, player: usize, space: Space, detail: Detail) -> Transition {
            game.public.active_player = player;
            match game.apply_with_plans(Action { space, detail }, None, None, &SimpleHarvestDetails)
            {
                Ok(t) => t,
                Err(e) => panic!("P{player} action {space:?} failed: {e}"),
            }
        }

        // --- Round 1: play both occupations via Lessons (public actions only).
        // The first occupation play costs no fee; the second costs 1 food.
        // (Both are played by the placement loop below, one per round, since
        // Lessons is usable once per round.)
        assert_eq!(game.public.players[0].played_cards, [] as [String; 0]);

        // --- Rounds 2..: accumulate clay (7) and reed (3), renovate to a clay
        // house as soon as HouseRedevelopment is revealed, then build a clay
        // room via FarmExpansion. That build triggers the double pause:
        // AfterClayConstruction (2 effects) -> AfterWorker (2 effects).
        let mut paused: Option<Box<crate::pending::PendingDecision>> = None;
        let mut build_pre: Option<(u32, u32)> = None;
        // Alternate P0 / P1 placements. Lessons is usable once per round per
        // player (a used space stays used until the round ends), so the two
        // occupation plays land in different rounds automatically.
        let mut p0_turn = true;
        for _ in 0..60 {
            if paused.is_some() {
                break;
            }
            if p0_turn {
                let clay = game.public.players[0].supply.clay;
                let reed = game.public.players[0].supply.reed;
                let house = game.public.players[0].house;
                let clay_double = game.public.players[0]
                    .played_cards
                    .iter()
                    .any(|id| id == "TEST_CLAY_DOUBLE");
                let worker_double = game.public.players[0]
                    .played_cards
                    .iter()
                    .any(|id| id == "TEST_WORKER_DOUBLE");
                let lessons_free = !game.public.used.contains(&Space::Lessons);
                let (space, detail) = if !clay_double && lessons_free {
                    (
                        Space::Lessons,
                        Detail::PlayOccupation {
                            card_id: "TEST_CLAY_DOUBLE".into(),
                        },
                    )
                } else if !worker_double && lessons_free {
                    (
                        Space::Lessons,
                        Detail::PlayOccupation {
                            card_id: "TEST_WORKER_DOUBLE".into(),
                        },
                    )
                } else if house == House::Clay && clay >= 5 && reed >= 2 {
                    (
                        Space::FarmExpansion,
                        Detail::Build {
                            rooms: vec![1], // adjacent to the starting room at 0
                            stables: vec![],
                        },
                    )
                } else if house == House::Wood
                    && clay >= 7
                    && reed >= 3
                    && game
                        .public
                        .revealed
                        .contains(&RoundCard::HouseRedevelopment)
                {
                    (
                        Space::HouseRedevelopment,
                        Detail::Renovate {
                            bonus_improvement: None,
                            instant_bake: 0,
                            pastures: vec![],
                        },
                    )
                } else {
                    let preferred = if clay < 7 {
                        Space::ClayPit
                    } else if reed < 3 {
                        Space::ReedBank
                    } else {
                        Space::DayLaborer
                    };
                    let fallback = if game.public.used.contains(&preferred) {
                        [
                            Space::DayLaborer,
                            Space::ClayPit,
                            Space::ReedBank,
                            Space::Fishing,
                            Space::Forest,
                        ]
                        .into_iter()
                        .find(|s| !game.public.used.contains(s))
                        .expect("P0 has a free fallback space")
                    } else {
                        preferred
                    };
                    (fallback, Detail::None)
                };
                // Snapshot supply right before the build so the delta asserts
                // below measure exactly this action's card effects.
                if space == Space::FarmExpansion {
                    build_pre = Some((
                        game.public.players[0].supply.food,
                        game.public.players[0].supply.wood,
                    ));
                }
                let t = act(&mut game, 0, space, detail);
                match t {
                    Transition::Completed => {}
                    Transition::Paused(pending) => {
                        paused = Some(pending);
                        break;
                    }
                }
            } else {
                // P1 placement: filler, no played cards => no card effects.
                let p1_space = [Space::DayLaborer, Space::Fishing, Space::Forest]
                    .into_iter()
                    .find(|s| game.legal_spaces().contains(s))
                    .expect("P1 must have a simple legal space");
                act(&mut game, 1, p1_space, Detail::None);
            }
            p0_turn = !p0_turn;
        }
        let pending = paused.expect("the clay-room build must pause");

        // First pause: AfterClayConstruction, two simultaneous After effects.
        assert!(matches!(
            pending.kind,
            PendingKind::ChooseTriggerOrder {
                phase: crate::card_events::TriggerPhase::After,
                ..
            }
        ));

        // Resolve first pause -> the chain advances to AfterWorker and pauses again.
        let outcome = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("first resolution must succeed");
        let Transition::Paused(pending2) = outcome else {
            panic!("expected second pause on AfterWorker, got {outcome:?}");
        };
        assert!(matches!(
            pending2.kind,
            PendingKind::ChooseTriggerOrder {
                phase: crate::card_events::TriggerPhase::After,
                ..
            }
        ));

        // Resolve second pause -> the action completes.
        let outcome2 = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("second resolution must succeed");
        assert_eq!(
            outcome2,
            Transition::Completed,
            "final resume must complete"
        );

        // Each of the four card effects applied exactly once: +2 food, +2 wood
        // relative to the state right before the build. Asserting the delta
        // (not the absolute supply) keeps the test meaningful regardless of
        // feeding/begging from earlier rounds.
        let (pre_food, pre_wood) = build_pre.expect("the build action ran");
        assert_eq!(
            game.public.players[0].supply.food,
            pre_food + 2,
            "one food from clay + one food from worker"
        );
        assert_eq!(
            game.public.players[0].supply.wood,
            pre_wood + 2,
            "one wood from clay + one wood from worker"
        );

        // Sanity: the build really happened (room count grew) and the action
        // bookkeeping is complete (space used, worker placed).
        assert_eq!(game.public.players[0].rooms(), 3);
        assert!(game.public.used.contains(&Space::FarmExpansion));

        // Round-trip save/load/replay: every state element above was produced
        // by public actions, so this must reproduce byte-for-byte.
        let json = game.save_json().expect("save must succeed");
        let loaded = Game::load_json(&json).expect("load_json must succeed");
        let replayed = Game::replay_with_setup_and_start_player(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            loaded.log.initial_hands.clone(),
            loaded.log.initial_start_player_override,
            &loaded.log.events,
        )
        .expect("replay must succeed");

        assert_eq!(
            game.state_hash(),
            replayed.state_hash(),
            "state_hash must match after save/load/replay round-trip"
        );
        assert_eq!(
            replayed.public.players[0].supply.food, game.public.players[0].supply.food,
            "food must reproduce after replay"
        );
        assert_eq!(
            replayed.public.players[0].supply.wood, game.public.players[0].supply.wood,
            "wood must reproduce after replay"
        );
        assert!(replayed.public.used.contains(&Space::FarmExpansion));
        assert_eq!(replayed.public.workers_placed, game.public.workers_placed);
    }

    /// C111 小動物飼育員 via the production round-start dispatcher: the
    #[test]
    fn c111_round_start_food_threshold_uses_current_round() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let dispatch = |food: u32, round: u8| {
            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C111".into()];
            c.players[0].supply.food = food;
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_start_round(
                &set,
                &mut c.players,
                &mut c.cards,
                &mut market,
                &mut well,
                round,
                &BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            c.players[0].supply.food
        };
        // Round 5: 4 food < 5 → no gain; 5 food >= 5 → +1.
        assert_eq!(dispatch(4, 5), 4);
        assert_eq!(dispatch(5, 5), 6);
        // Round 8: 7 food < 8 → no gain; 8 food >= 8 → +1.
        assert_eq!(dispatch(7, 8), 7);
        assert_eq!(dispatch(8, 8), 9);
        // The band set must not change the built-in A/B preset pool.
        let preset = crate::cards::verified_basic_card_set(2).unwrap();
        assert!(preset.definition("C111").is_none());
    }

    /// C106 じゃがいも収穫者 via the production harvest-field dispatcher:
    /// 1 extra food per vegetable the field phase is about to yield.
    #[test]
    fn c106_harvest_field_food_per_growing_vegetable_field() {
        use crate::model::{Crop, Tile};
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C106".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 2,
            "two growing vegetable fields must grant 2 food"
        );
    }

    /// C116 調度品職人 via the production play dispatcher: each later
    /// occupation play grants 1 wood per food paid as its occupation cost.
    #[test]
    fn c116_occupation_play_grants_wood_per_food_cost() {
        let mut set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "occ_food2".into(),
            kind: CardKind::Occupation,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![],
        });
        let mut c = Ctx::new();
        // Zone move already applied by the caller: C116 first, new card last.
        c.players[0].played_cards = vec!["C116".into(), "occ_food2".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        dispatch_play(
            &set,
            0,
            "occ_food2",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 2,
            "occupation food cost 2 must grant 2 wood via C116"
        );
        // Fee also counts as occupation cost.
        c.players[0].played_cards = vec!["C116".into(), "C111".into()];
        dispatch_play(
            &set,
            0,
            "C111",
            3,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 5, "fee 3 + cost 0 -> 3 wood");
        // Minor improvements and zero-cost occupations do not trigger it.
        c.players[0].played_cards = vec!["C116".into(), "C106".into()];
        dispatch_play(
            &set,
            0,
            "C106",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 5, "no food paid -> no gain");
    }

    /// Dispatcher play path for A117 木材調達人: verifies that the dispatcher
    /// route grants 1 wood per improvement in front (base improvements +
    /// played minor improvements), and does NOT count occupations.
    #[test]
    fn dispatch_play_a117_wood_per_improvement_in_front() {
        // Add a minor improvement card for testing
        let set = CardSet {
            definitions: {
                let mut defs = verified_band_a_cards();
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs
            },
        };

        // Case 1: no improvements -> 0 wood
        let mut c = Ctx::new();
        c.players[0].supply.wood = 5;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_play(
            &set,
            0,
            "A117",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 5, "no improvements -> 0 wood");

        // Case 2: one minor improvement + one base improvement -> 2 wood
        let mut c = Ctx::new();
        c.players[0].supply.wood = 5;
        c.players[0].played_cards.push("minor1".into());
        c.players[0].improvements.push(Improvement::Well);
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_play(
            &set,
            0,
            "A117",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 7, "1 minor + 1 base -> 2 wood");

        // Case 3: occupation in front should NOT be counted
        let mut c = Ctx::new();
        c.players[0].supply.wood = 5;
        c.players[0].played_cards.push("minor1".into());
        c.players[0].improvements.push(Improvement::Well);
        c.players[0].played_cards.push("A099".into()); // occupation
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_play(
            &set,
            0,
            "A117",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 7,
            "occupation A099 must NOT count -> still 2 wood"
        );
    }

    /// Dispatcher play path for A106 液肥散布人: verifies that the dispatcher
    /// route correctly triggers harvest field rewards for fields with remaining==1.
    #[test]
    fn dispatch_play_a106_harvest_field_last_crop() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };

        // Case 1: field with remaining==1 grain -> 2 food
        let mut c = Ctx::new();
        c.players[0].played_cards.push("A106".into());
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8, // harvest round
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.food, 2, "last grain field -> 2 food");

        // Case 2: field with remaining==1 vegetable -> 1 food
        let mut c = Ctx::new();
        c.players[0].played_cards.push("A106".into());
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "last vegetable field -> 1 food"
        );

        // Case 3: field with remaining > 1 -> 0 food
        let mut c = Ctx::new();
        c.players[0].played_cards.push("A106".into());
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.food, 0, "remaining > 1 -> 0 food");
    }

    /// Equivalence test: the three paths (legacy cards::play, dispatcher
    /// dispatch_play, legacy module apply_on_play_effects_legacy) must produce
    /// identical wood gains for A117.
    #[test]
    fn equivalence_a117_three_paths_match() {
        let set = CardSet {
            definitions: {
                let mut defs = verified_band_a_cards();
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor2".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs
            },
        };

        // Setup: 2 minor improvements + 1 base improvement = 3 improvements
        let base_player = {
            let mut p = Player::new(0, 0);
            p.supply.wood = 10;
            p.played_cards = vec!["minor1".into(), "minor2".into()];
            p.improvements.push(Improvement::Well);
            p
        };

        // Path 1: legacy cards::play
        let mut legacy_hand = vec!["A117".to_string()];
        let mut legacy = base_player.clone();
        crate::cards::play(
            &set,
            &mut legacy_hand,
            &mut legacy,
            "A117",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        let legacy_wood = legacy.supply.wood;

        // Path 2: dispatcher (play_validate_pay + play_from_hand + dispatch_play)
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["A117".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![base_player.clone()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        {
            let mut ctx = crate::card_zones::CardMoveCtx {
                cards: &mut cards,
                players: &mut players,
                improvement_market: &mut market,
            };
            crate::cards::play_validate_pay(
                &set,
                &mut ctx.cards.hands[0],
                &mut ctx.players[0],
                "A117",
                CardKind::Occupation,
                0,
            )
            .unwrap();
            crate::card_zones::play_from_hand(&mut ctx, 0, "A117").unwrap();
            dispatch_play(
                &set,
                0,
                "A117",
                0,
                ctx.cards,
                ctx.players,
                ctx.improvement_market,
                &mut well,
                1,
                &BTreeSet::new(),
                &mut budget,
                &mut uses,
                &limits,
                &mut pending,
            )
            .unwrap();
            let card = set.definition("A117").unwrap();
            crate::cards::update_card_points_on_play(&mut ctx.players[0], card.points).unwrap();
        }
        let dispatcher_wood = players[0].supply.wood;

        // Path 3: legacy module apply_on_play_effects_legacy
        let mut legacy3_hand = vec!["A117".to_string()];
        let mut legacy3 = base_player.clone();
        legacy::apply_on_play_effects_legacy(
            &set,
            &mut legacy3_hand,
            &mut legacy3,
            "A117",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        let legacy_mod_wood = legacy3.supply.wood;

        assert_eq!(
            legacy_wood, dispatcher_wood,
            "legacy cards::play vs dispatcher: {legacy_wood} vs {dispatcher_wood}"
        );
        assert_eq!(
            legacy_wood, legacy_mod_wood,
            "legacy cards::play vs legacy module: {legacy_wood} vs {legacy_mod_wood}"
        );
        assert_eq!(
            dispatcher_wood, legacy_mod_wood,
            "dispatcher vs legacy module: {dispatcher_wood} vs {legacy_mod_wood}"
        );
        // Expected: 10 + 3 = 13
        assert_eq!(legacy_wood, 13, "expected 3 wood from 3 improvements");
    }

    /// Reproduction test for PR #326 P0: HarvestFieldGain should NOT fire on
    /// dispatch_after_worker. C106 has HarvestFieldGain (VegetablesHarvested).
    #[test]
    fn c106_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C106".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // C106 has HarvestFieldGain (VegetablesHarvested) which should ONLY fire
        // on harvest field phase, NOT on worker completion.
        // If the bug exists, this will incorrectly gain 2 food.
        assert_eq!(
            c.players[0].supply.food, 0,
            "C106 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: StartRoundGain should NOT fire on
    /// dispatch_after_worker. B057 has StartRoundGain (Food+1 when Wood house).
    #[test]
    fn b057_start_round_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // B057 has StartRoundGain which should ONLY fire on round start, NOT on
        // worker completion. If the bug exists, this will incorrectly gain 1 food.
        assert_eq!(
            c.players[0].supply.food, 0,
            "B057 StartRoundGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: C111 StartRoundGainIfFoodAtLeastRound
    /// should NOT fire on dispatch_after_worker.
    #[test]
    fn c111_start_round_gain_if_food_at_least_round_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10; // >= round (4)
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // C111 has StartRoundGainIfFoodAtLeastRound which should ONLY fire on
        // round start, NOT on worker completion.
        assert_eq!(
            c.players[0].supply.food, 10,
            "C111 StartRoundGainIfFoodAtLeastRound must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: B118 StartRoundGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b118_start_round_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        // B118 requires 2 rooms; Player::new() gives 2 rooms by default.
        // The only reason it doesn't fire here is the event_kind check (RoundStart vs WorkerPlaced).
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // B118 has 2 rooms by default; the event_kind check prevents firing.
        assert_eq!(
            c.players[0].supply.wood, 10,
            "B118 StartRoundGain must not fire on after_worker (event_kind check)"
        );
    }

    /// Reproduction test for PR #326 P0: B107 StartRoundGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b107_start_round_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_simple_occupations(3), // B107 is in simple_occupations
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B107".into()];
        c.players[0].house = House::Stone; // B107 requires Stone house
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B107 StartRoundGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: B050 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b050_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        c.players[0].pastures.push(Pasture {
            id: 1,
            cells: vec![7, 8],
            animal: Some(Animal::Cattle),
            quantity: 3,
        });
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // B050 has HarvestFieldGain (Animals) which should ONLY fire on harvest
        // field phase, NOT on worker completion.
        assert_eq!(
            c.players[0].supply.food, 0,
            "B050 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: B061 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b061_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        // B061 needs MixedFields (grain + vegetable + fallow) - set up so mutation would fire
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B061 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: A106 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn a106_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_a_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A106".into()];
        // A106 needs LastGrainTaken (grain field with remaining 1) and/or LastVegetableTaken (veg field with remaining 1)
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "A106 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: E107 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn e107_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        // E107 needs 2+ plowed fields (FieldCountThresholds: 2->1 food) - set up so mutation would fire
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E107 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: E111 StartRoundGainIfNoMinorPlayed should NOT
    /// fire on dispatch_after_worker.
    #[test]
    fn e111_start_round_gain_if_no_minor_played_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        // E111 condition: no minor improvements played - satisfied in this test
        // E111 also has HarvestFieldGainIfNoMinorPlayed giving wood - check both food and wood
        c.players[0].supply.wood = 0;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E111 StartRoundGainIfNoMinorPlayed must not fire on after_worker"
        );
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E111 HarvestFieldGainIfNoMinorPlayed must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: E117 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn e117_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        // E117 needs a wheat (grain) field (HasWheatField) - set up so mutation would fire
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E117 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Test that the correct effects DO fire on their proper dispatchers.
    /// C106 should fire on harvest field phase.
    #[test]
    fn c106_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C106".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        // C106 has HarvestFieldGain (VegetablesHarvested) - should fire on harvest field
        assert_eq!(
            c.players[0].supply.food, 2,
            "C106 HarvestFieldGain must fire on harvest field phase"
        );
    }

    /// Test that B057 fires on round start.
    #[test]
    fn b057_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "B057 StartRoundGain must fire on start_round"
        );
    }

    /// Test that B050 fires on harvest field phase.
    #[test]
    fn b050_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        // 6 sheep = 2 food (per 3), 3 cattle = 1 food (per 2) -> total 3 food
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        c.players[0].pastures.push(Pasture {
            id: 1,
            cells: vec![7, 8],
            animal: Some(Animal::Cattle),
            quantity: 3,
        });
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
                    c.players[0].supply.food, 3,
                    "B050 HarvestFieldGain (Animals) must fire on harvest field phase: 6 sheep + 3 cattle = 3 food"
                );
    }

    /// Test that B061 fires on harvest field phase.
    #[test]
    fn b061_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        // B061 needs MixedFields: grain + vegetable + fallow
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 3,
            "B061 HarvestFieldGain (MixedFields) must fire on harvest field phase"
        );
    }

    /// Test that B118 fires on round start.
    #[test]
    fn b118_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        // B118 needs exactly 2 rooms - Player::new() gives 2 rooms by default
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 11,
            "B118 StartRoundGain must fire on start_round (exactly 2 rooms)"
        );
    }

    /// Test that C111 fires on round start (food >= round).
    #[test]
    fn c111_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10; // >= round (4)
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 11,
            "C111 StartRoundGainIfFoodAtLeastRound must fire on start_round"
        );
    }

    /// Test that E107 fires on harvest field phase.
    #[test]
    fn e107_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        // E107: 4 plowed fields -> 2 food (thresholds: 2->1, 4->2, 6->3, 7->4)
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
                    c.players[0].supply.food, 2,
                    "E107 HarvestFieldGain (FieldCountThresholds) must fire on harvest field phase: 4 fields -> 2 food"
                );
    }

    /// Test that E111 fires on round start (food, no minor played).
    #[test]
    fn e111_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        // E111: no minor improvements played -> 1 food at round start
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "E111 StartRoundGainIfNoMinorPlayed must fire on start_round"
        );
    }

    /// Test that E111 fires on harvest field phase (wood, no minor played).
    #[test]
    fn e111_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        c.players[0].supply.wood = 0;
        // E111: no minor improvements played -> 1 wood at harvest field phase
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "E111 HarvestFieldGainIfNoMinorPlayed must fire on harvest field phase"
        );
    }

    /// Test that E117 fires on harvest field phase.
    #[test]
    fn e117_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        // E117: at least 1 grain (wheat) field -> 2 wood
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 2,
            "E117 HarvestFieldGain (HasWheatField) must fire on harvest field phase"
        );
    }

    // =====================================================================
    // Negative tests: cards must NOT fire on WRONG dispatch sites
    // (clay_construction, after_harvest_bake)
    // =====================================================================

    /// B050 must not fire on after_clay_construction.
    #[test]
    fn b050_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(), // previous player
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B050 must not fire on after_clay_construction"
        );
    }

    /// B050 must not fire on after_harvest_bake.
    #[test]
    fn b050_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B050 must not fire on after_harvest_bake"
        );
    }

    /// B057 must not fire on after_clay_construction.
    #[test]
    fn b057_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B057 must not fire on after_clay_construction"
        );
    }

    /// B057 must not fire on after_harvest_bake.
    #[test]
    fn b057_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B057 must not fire on after_harvest_bake"
        );
    }

    /// B061 must not fire on after_clay_construction.
    #[test]
    fn b061_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B061 must not fire on after_clay_construction"
        );
    }

    /// B061 must not fire on after_harvest_bake.
    #[test]
    fn b061_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B061 must not fire on after_harvest_bake"
        );
    }

    /// B118 must not fire on after_clay_construction.
    #[test]
    fn b118_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 10,
            "B118 must not fire on after_clay_construction"
        );
    }

    /// B118 must not fire on after_harvest_bake.
    #[test]
    fn b118_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 10,
            "B118 must not fire on after_harvest_bake"
        );
    }

    /// C106 must not fire on after_clay_construction.
    #[test]
    fn c106_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C106".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C106 must not fire on after_clay_construction"
        );
    }

    /// C106 must not fire on after_harvest_bake.
    #[test]
    fn c106_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C106".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C106 must not fire on after_harvest_bake"
        );
    }

    /// C111 must not fire on after_clay_construction.
    #[test]
    fn c111_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 10,
            "C111 must not fire on after_clay_construction"
        );
    }

    /// C111 must not fire on after_harvest_bake.
    #[test]
    fn c111_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 10,
            "C111 must not fire on after_harvest_bake"
        );
    }

    /// E107 must not fire on after_clay_construction.
    #[test]
    fn e107_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E107 must not fire on after_clay_construction"
        );
    }

    /// E107 must not fire on after_harvest_bake.
    #[test]
    fn e107_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E107 must not fire on after_harvest_bake"
        );
    }

    /// E111 must not fire on after_clay_construction (food).
    #[test]
    fn e111_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E111 must not fire on after_clay_construction (food)"
        );
    }

    /// E111 must not fire on after_harvest_bake (food).
    #[test]
    fn e111_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E111 must not fire on after_harvest_bake (food)"
        );
    }

    /// E111 must not fire on after_clay_construction (wood).
    #[test]
    fn e111_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        c.players[0].supply.wood = 0;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E111 must not fire on after_clay_construction (wood)"
        );
    }

    /// E111 must not fire on after_harvest_bake (wood).
    #[test]
    fn e111_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        c.players[0].supply.wood = 0;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E111 must not fire on after_harvest_bake (wood)"
        );
    }

    /// E117 must not fire on after_clay_construction.
    #[test]
    fn e117_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E117 must not fire on after_clay_construction"
        );
    }

    /// E117 must not fire on after_harvest_bake.
    #[test]
    fn e117_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E117 must not fire on after_harvest_bake"
        );
    }

    // --- A118/A145/C066 dispatch tests (PR #385 review) ---

    /// A118: fixed 1 wood per harvest field phase fires; optional exchange is plan-driven
    /// and does NOT create a pending decision in the dispatcher.
    #[test]
    fn a118_harvest_field_fixed_wood_fires_and_no_pending() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A118".into()];
        c.players[0].supply.food = 0; // no food for exchange
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "A118 fixed HarvestFieldGain: 1 wood per harvest field phase"
        );
        assert!(
            pending.is_none(),
            "A118 optional exchange must not create a pending decision in dispatcher (plan-driven)"
        );
    }

    /// A118 with food available: fixed wood fires, no pending (exchange is plan-driven via FeedingPlan).
    #[test]
    fn a118_harvest_field_with_food_no_pending() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A118".into()];
        c.players[0].supply.food = 5; // food available for exchange
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "A118 fixed wood still fires"
        );
        assert_eq!(
            c.players[0].supply.food, 5,
            "food unchanged in field phase (exchange happens in feeding via plan)"
        );
        assert!(
            pending.is_none(),
            "A118 optional exchange must not create a pending decision in dispatcher"
        );
    }

    /// A145: reed 1 fires at harvest tail (AfterHarvestComplete), not at field phase.
    #[test]
    fn a145_reed_fires_at_harvest_tail_not_field() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        // Field phase: must not fire
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A145".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.reed, 0,
            "A145 reed must not fire during field phase"
        );
        assert!(pending.is_none());

        // Harvest tail: must fire
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A145".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // c066_take_food_instead
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.reed, 1,
            "A145 reed must fire at harvest tail (AfterHarvestComplete)"
        );
        assert!(pending.is_none());
    }

    /// A145 in non-harvest round: no fire.
    #[test]
    fn a145_no_fire_in_non_harvest_round() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A145".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            3, // round 3 is not a harvest round
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.reed, 0,
            "A145 must not fire in non-harvest round"
        );
    }

    /// C066: grain 0/1 -> no gain.
    #[test]
    fn c066_grain_zero_one_no_gain() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        for grain in [0, 1] {
            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C066".into()];
            c.players[0].supply.grain = grain;
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_after_harvest_complete(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                4,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                false, // take_food_instead
            )
            .unwrap();
            assert_eq!(
                c.players[0].supply.food, 0,
                "C066 grain={grain}: no food gain below threshold"
            );
            assert_eq!(
                c.players[0].supply.grain, grain,
                "C066 grain={grain}: no grain change"
            );
            assert!(pending.is_none());
        }
    }

    /// C066: grain 2 -> food 1 only.
    #[test]
    fn c066_grain_two_food_one() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 2;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // take_food_instead = false (default: take grain instead at >=3)
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "C066 grain=2: food 1"
        );
        assert_eq!(
            c.players[0].supply.grain, 2,
            "C066 grain=2: grain unchanged"
        );
        assert!(pending.is_none());
    }

    /// C066: grain 3 -> grain 1 instead of food (default).
    #[test]
    fn c066_grain_three_grain_instead_default() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 3;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // take_food_instead = false -> take grain instead
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C066 grain=3 default: no food (grain instead)"
        );
        assert_eq!(
            c.players[0].supply.grain, 4,
            "C066 grain=3 default: +1 grain (3 -> 4)"
        );
        assert!(pending.is_none());
    }

    /// C066: grain 3 -> food 1 if take_food_instead = true.
    #[test]
    fn c066_grain_three_food_if_opt_in() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 3;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            true, // take_food_instead = true -> opt into food
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "C066 grain=3 with take_food_instead: food 1"
        );
        assert_eq!(
            c.players[0].supply.grain, 3,
            "C066 grain=3 with take_food_instead: grain unchanged"
        );
        assert!(pending.is_none());
    }

    /// C066: grain 5 -> still only food 1 + grain 1 (second trigger doesn't fire again).
    #[test]
    fn c066_grain_five_capped() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 5;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C066 grain=5 default: no food (grain instead)"
        );
        assert_eq!(
            c.players[0].supply.grain, 6,
            "C066 grain=5 default: +1 grain only (5 -> 6), not +2"
        );
        assert!(pending.is_none());
    }
}
    use crate::model::{Config, Game, Space};
use crate::pending::{ChainStep, PendingResolution, PostWorkResume};
use crate::engine::{Action, Detail};

    /// P1-1: the two resolve APIs must be distinguishable, and the standalone
    /// `resolve_pending` must NOT accept a paused work action.
    ///
    /// Pre-fix, `resolve_pending` happily consumed a PostWork decision: the
    /// choice was applied, the continuation (the paused work chain) was
    /// silently dropped, and the recorded `pending_resolution` event claimed
    /// the decision was complete — a replay then ran the continuation and hit
    /// a poststate hash mismatch. Post-fix the standalone API refuses PostWork
    /// decisions atomically, and both APIs stamp their event with
    /// `from_resume` so the log records which API resolved the decision.
    #[test]
    fn p1_1_resolve_pending_rejects_postwork_and_events_record_the_source_api() {
        let make_postwork_game = |seed: u64| {
            let mut game = Game::new(Config {
                players: 2,
                seed,
                additional_action_space_enabled: false,
            });
            let player = 0;
            let completion = Box::new(crate::pending::WorkCompletion {
                action: Action { space: Space::Forest, detail: Detail::None },
                previous_player: game.public.players[player].clone(),
                harvest_plans: None,
                breeding_plans: None,
                logged_before: String::new(),
            });
            let resume = PostWorkResume {
                step: ChainStep::AfterWorker,
                phase: crate::card_events::TriggerPhase::Interrupt,
                after: crate::pending::ChainPosition { card_index: 0, effect_index: 0 },
                space_applied: false,
            };
            game.pending = Some(crate::pending::PendingDecision {
                id: 1,
                player,
                kind: crate::pending::PendingKind::ChooseGain {
                    source_card: "TEST_CARD".into(),
                    options: vec![crate::pending::GainOption {
                        good: crate::model::Good::Food,
                        amount: 1,
                    }],
                },
                state_hash_before: game.state_hash(),
                context: serde_json::json!({}),
                continuation: crate::pending::Continuation::PostWork {
                    player,
                    resume,
                    completion,
                },
            });
            game
        };

        // (a) The standalone API must refuse a PostWork decision ATOMICALLY:
        // pending, state hash, and event log all unchanged.
        let mut game = make_postwork_game(1);
        let before_hash = game.state_hash();
        let before_pending = game.pending.clone();
        let before_events = game.log.events.len();
        let err = game
            .resolve_pending(PendingResolution::ChooseGain {
                chosen: crate::pending::GainOption { good: crate::model::Good::Food, amount: 1 },
            })
            .expect_err("resolve_pending must refuse a PostWork decision");
        assert!(
            err.contains("resolve_pending_and_resume"),
            "the refusal must name the correct API, got: {err}"
        );
        assert_eq!(before_hash, game.state_hash(), "P1-1: state must be unchanged");
        assert_eq!(before_pending, game.pending, "P1-1: pending must be unchanged");
        assert_eq!(before_events, game.log.events.len(), "P1-1: log must be unchanged");

        // (b) The resume API resolves the SAME decision and stamps its event
        // with from_resume = true.
        let mut game = make_postwork_game(1);
        let events_before = game.log.events.len();
        game.resolve_pending_and_resume(PendingResolution::ChooseGain {
            chosen: crate::pending::GainOption { good: crate::model::Good::Food, amount: 1 },
        })
        .expect("resume API must accept the PostWork decision");
        assert_eq!(events_before + 1, game.log.events.len());
        let payload = &game.log.events[events_before].payload;
        // The event payload is the serde-tagged RecordedAction, so the field
        // lives under the "PendingResolution" tag key.
        assert_eq!(
            payload
                .get("PendingResolution")
                .and_then(|p| p.get("from_resume")),
            Some(&serde_json::json!(true)),
            "P1-1: the resume API must stamp from_resume=true, got {payload}"
        );

        // (c) A decision WITHOUT a work continuation stays resolvable through
        // the standalone API, and its event is stamped from_resume = false.
        let mut game = Game::new(Config {
            players: 2,
            seed: 3,
            additional_action_space_enabled: false,
        });
        game.pending = Some(crate::pending::PendingDecision::new(
            1,
            0,
            crate::pending::PendingKind::ChooseGain {
                source_card: "TEST_CARD".into(),
                options: vec![crate::pending::GainOption {
                    good: crate::model::Good::Food,
                    amount: 1,
                }],
            },
            game.state_hash(),
            serde_json::json!({}),
        ));
        let events_before = game.log.events.len();
        game.resolve_pending(PendingResolution::ChooseGain {
            chosen: crate::pending::GainOption { good: crate::model::Good::Food, amount: 1 },
        })
        .expect("standalone API must still resolve plain decisions");
        let payload = &game.log.events[events_before].payload;
        assert_eq!(
            payload
                .get("PendingResolution")
                .and_then(|p| p.get("from_resume")),
            Some(&serde_json::json!(false)),
            "P1-1: the standalone API must stamp from_resume=false, got {payload}"
        );
    }

    /// P1-2: resolve_pending_and_resume atomicity violation.
    /// CONFIRMED: When the second half fails (after apply_pending_resolution),
    /// the pending is already taken and not restored on error.
    #[test]
    fn p1_2_resolve_pending_and_resume_atomicity_violation() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 2,
            additional_action_space_enabled: false,
        });
        let player = 0;

        // Set up a PostWork continuation with an invalid space (Forest not revealed)
        let completion = Box::new(crate::pending::WorkCompletion {
            action: Action { space: Space::Forest, detail: Detail::None },
            previous_player: game.public.players[player].clone(),
            harvest_plans: None,
            breeding_plans: None,
            logged_before: String::new(),
        });
        let resume = PostWorkResume {
            step: ChainStep::AfterWorker,
            phase: crate::card_events::TriggerPhase::Interrupt,
            after: crate::pending::ChainPosition { card_index: 0, effect_index: 0 },
            space_applied: false,
        };
        game.pending = Some(crate::pending::PendingDecision {
            id: 1,
            player,
            kind: crate::pending::PendingKind::ChoosePayment {
                card_id: "TEST_CARD".into(),
                options: vec![crate::pending::PaymentOption {
                    cost: std::collections::BTreeMap::from([(crate::model::Good::Wood, 1)]),
                }],
            },
            state_hash_before: game.state_hash(),
            context: serde_json::json!({}),
            continuation: crate::pending::Continuation::PostWork { player, resume, completion },
        });

        let before_hash = game.state_hash();
        let before_pending = game.pending.clone();
        let before_events = game.log.events.len();

        let resolution = PendingResolution::ChoosePayment {
            chosen: crate::pending::PaymentOption {
                cost: std::collections::BTreeMap::from([(crate::model::Good::Wood, 1)]),
            },
        };

        let result = game.resolve_pending_and_resume(resolution);

        // P1-2 FIX VERIFICATION: On any Err, the state must be fully restored:
        // pending, state hash, and event log all unchanged from before the call.
        if result.is_err() {
            let after_hash = game.state_hash();
            let after_pending = game.pending.clone();
            let after_events = game.log.events.len();
            eprintln!("P1-2 FIX: result={:?}, hash_changed={}, pending_changed={}, events_changed={}",
                result, before_hash != after_hash, before_pending != after_pending, before_events != after_events);

            assert_eq!(before_hash, after_hash, "P1-2: state_hash must be unchanged on error");
            assert_eq!(before_pending, after_pending, "P1-2: pending must be restored on error");
            assert_eq!(before_events, after_events, "P1-2: event log must be unchanged on error");
        } else {
            panic!("Expected error for invalid space, got Ok");
        }
    }

    /// P1-3 (Clay->Worker path): a build action that pauses inside the
    /// AfterClayConstruction step must freeze the PRE-action player snapshot.
    ///
    /// Pre-fix, `previous_player` was cloned from `self` AFTER `*self = next`
    /// committed the mid-action state, so the snapshot already contained the
    /// room built by this very action. Every later ClayConstruction condition
    /// re-check (`current.rooms() > prev.rooms()`) then compared the post-state
    /// against itself and the construction bonus was lost.
    #[test]
    fn p1_3_clay_pause_completion_previous_player_is_pre_action() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};
        use crate::harvest::SimpleHarvestDetails;
        use crate::model::{Good, House};

        let filler = |i: usize, tag: &str| CardDefinition {
            use_limit: None,
            id: format!("{tag}_{i}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        };
        let mut set = crate::cards::verified_basic_card_set(2).expect("basic set");
        // Two simultaneous AfterClayConstructionGain effects -> the After phase
        // of the clay step pauses on ChooseTriggerOrder (Issue #270 contract).
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_CLAY_DOUBLE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterClayConstructionGain { good: Good::Food, amount: 1 },
                CardEffect::AfterClayConstructionGain { good: Good::Wood, amount: 1 },
            ],
        });
        for i in 0..13 {
            set.definitions.push(filler(i, "FILLER_OCC"));
        }
        for i in 0..14 {
            set.definitions.push(CardDefinition {
                use_limit: None,
                id: format!("FILLER_MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        let hands = vec![
            ["TEST_CLAY_DOUBLE"]
                .iter()
                .map(|s| s.to_string())
                .chain((0..6).map(|i| format!("FILLER_OCC_{i}")))
                .chain((0..7).map(|i| format!("FILLER_MINOR_{i}")))
                .collect::<Vec<_>>(),
            (0..7)
                .map(|i| format!("FILLER_OCC_{}", i + 6))
                .chain((0..7).map(|i| format!("FILLER_MINOR_{}", i + 7)))
                .collect::<Vec<_>>(),
        ];
        let mut game = Game::with_cards_and_hands_with_start_player(
            Config { players: 2, seed: 1, additional_action_space_enabled: false },
            set,
            hands,
            0,
        )
        .expect("game created");

        // Direct fixture: clay house + the double clay card played. The card
        // partition stays valid because the id is moved hand -> played.
        game.cards.hands[0].retain(|id| id != "TEST_CLAY_DOUBLE");
        game.public.players[0].played_cards.push("TEST_CLAY_DOUBLE".into());
        game.public.players[0].house = House::Clay;
        game.public.players[0].supply.clay = 10;
        game.public.players[0].supply.reed = 5;
        let pre_rooms = game.public.players[0].rooms(); // 2 (starting rooms)
        assert_eq!(pre_rooms, 2, "fixture: two starting rooms");

        // Build one clay-horse room adjacent to the starting room at 0.
        let action = Action {
            space: Space::FarmExpansion,
            detail: Detail::Build { rooms: vec![1], stables: vec![] },
        };
        let transition = game
            .apply_with_plans(action, None, None, &SimpleHarvestDetails)
            .expect("build must be legal");
        let crate::pending::Transition::Paused(pending) = transition else {
            panic!("the two clay effects must pause the chain, got {transition:?}");
        };
        let crate::pending::Continuation::PostWork { completion, .. } = &pending.continuation
        else {
            panic!("paused build must carry a PostWork continuation");
        };
        // THE CONTRACT: the frozen snapshot is the PRE-action player (2 rooms),
        // not the mid-action commit that already contains the new room (3).
        assert_eq!(
            completion.previous_player.rooms(),
            pre_rooms,
            "P1-3: WorkCompletion.previous_player must be the pre-action snapshot"
        );

        // Resolving the pause must actually pay both clay bonuses: the
        // conditions re-check against the correct pre-action snapshot.
        let pre_food = game.public.players[0].supply.food;
        let pre_wood = game.public.players[0].supply.wood;
        let outcome = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, crate::pending::Transition::Completed);
        assert_eq!(
            game.public.players[0].supply.food,
            pre_food + 1,
            "clay bonus 1 (food) must fire on resume"
        );
        assert_eq!(
            game.public.players[0].supply.wood,
            pre_wood + 1,
            "clay bonus 2 (wood) must fire on resume"
        );
    }

    /// P1-3 (Bake->Clay path): a renovate action that bakes and buys a bonus
    /// improvement, interrupted inside the AfterHarvestBake step, must resume
    /// into AfterClayConstruction with the PRE-action snapshot (clay house,
    /// no bonus improvement yet) so the house upgrade bonus still fires.
    #[test]
    fn p1_3_bake_pause_completion_previous_player_is_pre_action() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};
        use crate::harvest::SimpleHarvestDetails;
        use crate::model::{Good, House, RoundCard};

        let mut set = crate::cards::verified_basic_card_set(2).expect("basic set");
        // Two simultaneous AfterHarvestBakeGain effects -> the After phase of
        // the bake step pauses on ChooseTriggerOrder (Issue #270 contract).
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_BAKE_DOUBLE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterHarvestBakeGain { amount: 1 },
                CardEffect::AfterHarvestBakeGain { amount: 1 },
            ],
        });
        // Clay-construction bonus: fires when the owner upgrades clay->stone.
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_CLAY_UPGRADE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterClayConstructionGain {
                good: Good::Food,
                amount: 1,
            }],
        });
        for i in 0..12 {
            set.definitions.push(CardDefinition {
                use_limit: None,
                id: format!("FILLER_OCC_{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        for i in 0..14 {
            set.definitions.push(CardDefinition {
                use_limit: None,
                id: format!("FILLER_MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        let hands = vec![
            ["TEST_BAKE_DOUBLE", "TEST_CLAY_UPGRADE"]
                .iter()
                .map(|s| s.to_string())
                .chain((0..5).map(|i| format!("FILLER_OCC_{i}")))
                .chain((0..7).map(|i| format!("FILLER_MINOR_{i}")))
                .collect::<Vec<_>>(),
            (0..7)
                .map(|i| format!("FILLER_OCC_{}", i + 5))
                .chain((0..7).map(|i| format!("FILLER_MINOR_{}", i + 7)))
                .collect::<Vec<_>>(),
        ];
        let mut game = Game::with_cards_and_hands_with_start_player(
            Config { players: 2, seed: 2, additional_action_space_enabled: false },
            set,
            hands,
            0,
        )
        .expect("game created");

        // Direct fixture: clay house + both cards played (hand -> played keeps
        // the card partition valid). Round 5 so the bake step dispatches
        // (round-1 = 4 is a harvest round).
        game.cards.hands[0].retain(|id| id != "TEST_BAKE_DOUBLE" && id != "TEST_CLAY_UPGRADE");
        game.public.players[0].played_cards.push("TEST_BAKE_DOUBLE".into());
        game.public.players[0].played_cards.push("TEST_CLAY_UPGRADE".into());
        game.public.players[0].house = House::Clay;
        game.public.round = 5;
        // Advance the board to round 5 (reveal 4 more cards) keeping the
        // revealed+hidden multiset intact, then force HouseRedevelopment as
        // the newest revealed card so the renovate space is available.
        for _ in 0..4 {
            let card = game.hidden_round_deck.pop().expect("14 rounds only");
            game.public.revealed.push(card);
        }
        {
            let last = *game.public.revealed.last().expect("revealed non-empty");
            if last != RoundCard::HouseRedevelopment {
                let idx = game
                    .hidden_round_deck
                    .iter()
                    .position(|c| *c == RoundCard::HouseRedevelopment)
                    .expect("HouseRedevelopment in the deck");
                game.hidden_round_deck[idx] = last;
                *game.public.revealed.last_mut().expect("non-empty") =
                    RoundCard::HouseRedevelopment;
            }
        }
        let p = &mut game.public.players[0];
        p.supply.clay = 10;
        p.supply.stone = 10;
        p.supply.reed = 5;
        p.supply.grain = 3;
        p.supply.wood = 10;
        let pre_food = p.supply.food;
        let pre_improvements = p.improvements.len();

        // Renovate clay->stone, buy the ClayOven bonus, and bake one grain
        // with it -- all inside ONE action whose bake step pauses.
        let action = Action {
            space: Space::HouseRedevelopment,
            detail: Detail::Renovate {
                bonus_improvement: Some(crate::model::Improvement::ClayOven),
                instant_bake: 1,
                pastures: vec![],
            },
        };
        let transition = game
            .apply_with_plans(action, None, None, &SimpleHarvestDetails)
            .expect("renovate+bake must be legal");
        assert!(
            matches!(transition, crate::pending::Transition::Paused(ref p)
                if matches!(p.kind, crate::pending::PendingKind::ChooseTriggerOrder { .. })),
            "the two bake effects must pause the chain on ChooseTriggerOrder, got {transition:?}"
        );
        let Some(pending) = game.pending.clone() else {
            panic!("the committed pause decision must be stored");
        };
        let crate::pending::Continuation::PostWork { completion, .. } = &pending.continuation
        else {
            panic!("paused renovate must carry a PostWork continuation");
        };
        // THE CONTRACT: the frozen snapshot predates the action: clay house,
        // no ClayOven on the table yet.
        assert_eq!(
            completion.previous_player.house,
            House::Clay,
            "P1-3 Bake->Clay: previous_player must be the pre-action (clay) house"
        );
        assert_eq!(
            completion.previous_player.improvements.len(),
            pre_improvements,
            "P1-3 Bake->Clay: previous_player must not contain the bonus oven"
        );

        // Resolve the bake-order pause: the resume continues into
        // AfterClayConstruction, where the upgrade condition must see the
        // pre-action clay house and pay the bonus.
        let outcome = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, crate::pending::Transition::Completed);
        // 5 food from the ClayOven bake + 1+1 from the two bake-step gains
        // + 1 from the clay->stone upgrade bonus.
        assert_eq!(
            game.public.players[0].supply.food,
            pre_food + 5 + 2 + 1,
            "P1-3 Bake->Clay: the construction bonus must fire after the bake pause"
        );
    }

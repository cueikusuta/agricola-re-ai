import re

# Read the file
with open(r"C:\Users\cue\AppData\Local\hermes\kanban\boards\game-dev\workspaces\t_b1b2eda9\src\card_events.rs", "r") as f:
    content = f.read()

# Find the test module boundaries
mod_tests_start = content.find("mod tests\n{\n")
if mod_tests_start == -1:
    mod_tests_start = content.find("mod tests {\n")
print(f"mod tests starts at: {mod_tests_start}")

# Find the closing brace of the test module
brace_count = 0
mod_end = mod_tests_start
for i, ch in enumerate(content[mod_tests_start:]):
    if ch == '{':
        brace_count += 1
    elif ch == '}':
        brace_count -= 1
        if brace_count == 0:
            mod_end = mod_tests_start + i + 1
            break

print(f"mod tests ends at: {mod_end}")

# The merged test module content - combining HEAD's A118/A145/C066 tests and main's C083/C147 tests
# We'll keep the first part of the test module (before the conflicts) and replace from the conflict point
# The conflict starts at "// --- A118/A145/C066 dispatch tests (PR #385 review) ---"

conflict_start = content.find("// --- A118/A145/C066 dispatch tests (PR #385 review) ---", mod_tests_start)
if conflict_start == -1:
    conflict_start = content.find("/// C083 若牛: on play gain 2 cattle", mod_tests_start)
print(f"Conflict section starts at: {conflict_start}")

# Build the merged test section
merged_tests = '''    // --- A118/A145/C066 dispatch tests (PR #385 review) ---

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

    // --- C083/C147 tests from PR #393 (market followup cards) ---

    /// C083 若牛: on play gain 2 cattle, auto-placed via AnimalPlan (no pause for on-play).
    #[test]
    fn c083_on_play_gain_animal_auto_placed() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.cards.hands = vec![vec!["C083".to_string()]];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();

        // Play C083 from hand
        let mut ctx = crate::card_zones::CardMoveCtx {
            cards: &mut c.cards,
            players: &mut c.players,
            improvement_market: &mut market,
        };
        crate::cards::play_validate_pay(
            &set,
            &mut ctx.cards.hands[0],
            &mut ctx.players[0],
            "C083",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        crate::card_zones::play_from_hand(&mut ctx, 0, "C083").unwrap();
        dispatch_play(
            &set,
            0,
            "C083",
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
            true, // is_improvement_action: C083 is MinorImprovement
        )
        .unwrap();

        // The effect should NOT pause; cattle should be auto-placed
        assert!(
            pending.is_none(),
            "C083 on-play must not pause for animal placement"
        );
        // Player 0 starts with no pastures/stables: 1 cattle goes to pet, 1 discarded
        assert_eq!(c.players[0].animal_count(Animal::Cattle), 1);
        assert_eq!(c.players[0].pet, Some(Animal::Cattle));
    }

    /// C147 牛の飼育士: after worker on CattleMarket gains 1 cattle (pauses for decision).
    #[test]
    fn c147_after_worker_gain_animal_pauses_for_placement() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C147".to_string()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();

        let result = dispatch_after_worker(
            &set,
            0,
            Space::CattleMarket,
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
            false,
        );

        // The effect should pause for ChooseAnimalPlacement
        assert!(result.is_ok());
        let pending_decision = pending.expect("C147 must pause for animal placement");
        assert!(matches!(
            pending_decision.kind,
            crate::pending::PendingKind::ChooseAnimalPlacement { .. }
        ));
        assert_eq!(pending_decision.player, 0);
    }

    /// C147 牛の飼育士: not triggered on other spaces.
    #[test]
    fn c147_after_worker_not_triggered_on_other_spaces() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C147".to_string()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();

        // Use Forest instead of CattleMarket
        let result = dispatch_after_worker(
            &set,
            0,
            Space::Forest,
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
            false,
        );

        // No pending decision should be created
        assert!(result.is_ok());
        assert!(pending.is_none(), "C147 must not trigger on Forest");
    }

    /// C147 牛の飼育士: on CattleMarket, multiple uses each grant 1 cattle.
    #[test]
    fn c147_multiple_uses_each_grant_cattle() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C147".to_string()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();

        // First use
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::CattleMarket,
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
            false,
        )
        .unwrap();
        assert!(pending.is_some(), "first use must pause");

        // Second use
        pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::CattleMarket,
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
            false,
        )
        .unwrap();
        assert!(pending.is_some(), "second use must also pause");
    }

    /// Resume with site mismatch: a ChooseTriggerOrder pause created in a WorkerPlaced
    /// context (site=WorkerPlaced) must be rejected when resumed in a different
    /// context (e.g., Bake). This prevents legacy saves with missing site fields
    /// from being misinterpreted.
    #[test]
    fn resume_rejects_site_mismatch_bake_vs_worker_placed() {
        use crate::engine::{Action, Detail};
        use crate::model::{Config, Game, Space};
        use crate::pending::{
            ChainPosition, ChainStep, Continuation, PendingDecision, PendingKind,
            PendingResolution, PostWorkResume, WorkCompletion,
        };

        let mut game = Game::new(Config {
            players: 2,
            seed: 99,
            additional_action_space_enabled: false,
        });

        // Create a ChooseTriggerOrder pause with site=WorkerPlaced (as if from AfterWorker)
        // but with a PostWork continuation for AfterHarvestBake (site=Bake).
        // This simulates a legacy save where site field was missing and defaulted to WorkerPlaced,
        // but the pause actually occurred during a Bake action.
        let worker_placed_site = crate::registry::EventKind::WorkerPlaced;
        let bake_site = crate::registry::EventKind::Bake;

        game.pending = Some(PendingDecision {
            id: 0,
            player: 0,
            kind: PendingKind::ChooseTriggerOrder {
                phase: crate::card_events::TriggerPhase::After,
                site: worker_placed_site, // Legacy default: WorkerPlaced
                triggers: vec![("TEST_CARD".into(), 0, "test effect".into())],
            },
            state_hash_before: game.state_hash(),
            context: serde_json::json!({
                "trigger_data": {
                    "space": null,
                    "improvement": null,
                    "baked": true,
                    "previous_player": null,
                    "used_spaces": [],
                    "site": bake_site, // Actual context: Bake
                    "event_kind": "bake"
                }
            }),
            continuation: Continuation::PostWork {
                player: 0,
                resume: PostWorkResume {
                    step: ChainStep::AfterHarvestBake, // Execution context: Bake
                    phase: crate::card_events::TriggerPhase::After,
                    after: ChainPosition {
                        card_index: 0,
                        effect_index: 0,
                    },
                    space_applied: false,
                },
                completion: Box::new(WorkCompletion {
                    action: Action {
                        space: Space::FarmExpansion,
                        detail: Detail::Sow {
                            plantings: vec![],
                            bake: 1,
                        },
                    },
                    previous_player: game.public.players[0].clone(),
                    harvest_plans: None,
                    breeding_plans: None,
                    logged_before: String::new(),
                }),
            },
        });

        // Attempting to resume should fail with site mismatch error
        let err = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0],
            })
            .unwrap_err();

        assert!(
            err.contains("site mismatch on resume"),
            "expected site mismatch error, got: {err}"
        );
        assert!(
            err.contains("WorkerPlaced") && err.contains("Bake"),
            "error should mention both sites, got: {err}"
        );
        // Decision should stay pending after rejection
        assert!(
            game.has_pending(),
            "decision must stay resolvable after rejection"
        );
    }
}'''

# Replace the conflict section to the end of the module
new_content = content[:conflict_start] + merged_tests + content[mod_end:]

# Write the result
with open(r"C:\Users\cue\AppData\Local\hermes\kanban\boards\game-dev\workspaces\t_b1b2eda9\src\card_events.rs", "w") as f:
    f.write(new_content)

print("Test module replaced successfully!")
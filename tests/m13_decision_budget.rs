//! #167 A: one-decision wall-clock budget covering search **and** details.
//!
//! These tests fix the contract, not the strength of any policy:
//!
//! 1. With no budget the play path is unchanged and still reproducible.
//! 2. A budget that is already spent still returns a legal action, and it says
//!    so (`budget_exhausted_before_details`, `detail_fallback`).
//! 3. Detail generation is inside the budget, so a slow detail policy cannot
//!    silently append its cost after the search (the #164 gap).
//! 4. Overshoot is measured, never hidden, and a whole game under a budget keeps
//!    the reproducibility contract only where the contract allows it.
use agricola_re_ai::decision_budget::{DecisionBudgetSpec, DecisionTimings};
use agricola_re_ai::engine::Action;
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::mcts_puct::{PuctConfig, PuctPolicy};
use agricola_re_ai::model::{Config, Game, Observation, Space};
use agricola_re_ai::policy::{
    play_to_end_with_budget, resolve_decision, BudgetScope, BudgetedDecisions, DetailPolicy,
    RandomPolicy, SimpleDetails,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::cell::Cell;
use std::time::{Duration, Instant};

fn game(players: usize, seed: u64) -> Game {
    Game::new(Config {
        players,
        seed,
        additional_action_space_enabled: false,
    })
}

/// A detail policy that always declines. Used to prove the legal fallback.
struct DecliningDetails;
impl DetailPolicy for DecliningDetails {
    fn produce(&self, _: &Observation, _: Space) -> Option<Action> {
        None
    }
}

/// A detail policy that burns a fixed amount of wall time before delegating.
struct SlowDetails {
    millis: u64,
    calls: Cell<usize>,
}
impl DetailPolicy for SlowDetails {
    fn produce(&self, obs: &Observation, space: Space) -> Option<Action> {
        self.calls.set(self.calls.get() + 1);
        std::thread::sleep(Duration::from_millis(self.millis));
        SimpleDetails.produce(obs, space)
    }
}

fn opening(players: usize) -> (Game, Observation, Vec<Space>) {
    let game = game(players, 4242);
    let obs = game.observation(game.public.active_player).unwrap();
    let legal = game.legal_spaces();
    (game, obs, legal)
}

#[test]
fn an_unbudgeted_decision_is_unchanged_and_unmeasured() {
    for players in [2, 4] {
        let (_, obs, legal) = opening(players);
        let policy = MctsPolicy {
            simulations: 4,
            tree_depth: 2,
        };
        let mut first_rng = ChaCha8Rng::seed_from_u64(11);
        let (first, first_outcome) =
            resolve_decision(&obs, &legal, &policy, &SimpleDetails, &mut first_rng, None).unwrap();
        let mut second_rng = ChaCha8Rng::seed_from_u64(11);
        let (second, second_outcome) =
            resolve_decision(&obs, &legal, &policy, &SimpleDetails, &mut second_rng, None).unwrap();
        assert_eq!(first, second, "no-budget selection must stay deterministic");
        assert_eq!(first_outcome.budget_ms, 0);
        assert_eq!(first_outcome.overshoot_ns, 0);
        assert!(!first_outcome.used_fallback());
        assert!(!first_outcome.is_late());
        // The two runs still measure their own wall-clock cost; only the budget
        // fields are zero.
        assert!(first_outcome.elapsed_ns > 0);
        assert!(second_outcome.elapsed_ns > 0);
    }
}

#[test]
fn an_unbudgeted_declining_detail_policy_still_fails_loudly() {
    let (_, obs, legal) = opening(2);
    let mut rng = ChaCha8Rng::seed_from_u64(3);
    let error = resolve_decision(
        &obs,
        &legal,
        &RandomPolicy,
        &DecliningDetails,
        &mut rng,
        None,
    )
    .unwrap_err();
    assert_eq!(error, "detail policy declined");
}

#[test]
fn an_already_spent_budget_still_returns_a_legal_action() {
    for players in [2, 4] {
        let (_, obs, legal) = opening(players);
        let policy = MctsPolicy {
            simulations: 100_000,
            tree_depth: 2,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        // A one-microsecond budget is spent before the decision can start.
        let started = Instant::now();
        let (action, outcome) = resolve_decision(
            &obs,
            &legal,
            &policy,
            &SimpleDetails,
            &mut rng,
            Some(DecisionBudgetSpec::new(Duration::from_micros(1))),
        )
        .unwrap();
        assert!(legal.contains(&action.space), "the action must stay legal");
        assert_eq!(
            outcome.budget_ms, 0,
            "sub-millisecond budgets round to 0 ms"
        );
        assert!(outcome.elapsed_ns < 5_000_000_000);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "a spent budget must not run a full search"
        );
        assert!(outcome.budget_exhausted_before_details);
        assert!(outcome.detail_fallback);
        assert!(outcome.used_fallback());
    }
}

#[test]
fn detail_generation_is_charged_to_the_same_budget() {
    // The #164 gap: a search that respects its deadline followed by a slow
    // detail policy produced a decision longer than the budget. The reserve plus
    // the measured detail time must expose that instead of hiding it.
    let (_, obs, legal) = opening(2);
    let policy = MctsPolicy {
        simulations: 100_000,
        tree_depth: 2,
    };
    let slow = SlowDetails {
        millis: 60,
        calls: Cell::new(0),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(9);
    let (action, outcome) = resolve_decision(
        &obs,
        &legal,
        &policy,
        &slow,
        &mut rng,
        Some(DecisionBudgetSpec::new(Duration::from_millis(50))),
    )
    .unwrap();
    assert!(legal.contains(&action.space));
    assert_eq!(slow.calls.get(), 1, "details must be attempted once");
    // The 60 ms detail call cannot be preempted, so the decision is late. That
    // is recorded: the contract is "measure and report", not "guarantee".
    assert!(
        outcome.elapsed_ns >= 60_000_000,
        "elapsed {} ns must include the detail cost",
        outcome.elapsed_ns
    );
    assert!(
        outcome.detail_ns >= 60_000_000,
        "detail time {} ns must be recorded separately",
        outcome.detail_ns
    );
    assert!(outcome.is_late());
    assert!(outcome.overshoot_ms() >= 10.0);
}

#[test]
fn a_spent_search_reserves_the_details_on_the_fallback_path() {
    // A budget whose search window is already gone but whose decision window is
    // not: details must still run, through the guaranteed-legal generator.
    let (_, obs, legal) = opening(2);
    let policy = MctsPolicy {
        simulations: 100_000,
        tree_depth: 2,
    };
    let slow = SlowDetails {
        millis: 0,
        calls: Cell::new(0),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(13);
    // Reserve half the budget for details; the search window is 5 ms, which a
    // 100000-simulation cap cannot exhaust, so this budget only proves the split
    // is honoured (search stops early, details still run normally).
    let (action, outcome) = resolve_decision(
        &obs,
        &legal,
        &policy,
        &slow,
        &mut rng,
        Some(DecisionBudgetSpec::with_reserve(
            Duration::from_millis(10),
            Duration::from_millis(5),
        )),
    )
    .unwrap();
    assert!(legal.contains(&action.space));
    assert_eq!(
        slow.calls.get(),
        1,
        "details run inside the decision window"
    );
    assert!(!outcome.detail_fallback);
    assert!(outcome.elapsed_ms() < 200.0);
}

#[test]
fn a_declining_detail_policy_falls_back_to_legal_details_under_a_budget() {
    let (_, obs, legal) = opening(4);
    let mut rng = ChaCha8Rng::seed_from_u64(17);
    let (action, outcome) = resolve_decision(
        &obs,
        &legal,
        &RandomPolicy,
        &DecliningDetails,
        &mut rng,
        Some(DecisionBudgetSpec::new(Duration::from_millis(200))),
    )
    .unwrap();
    assert!(legal.contains(&action.space));
    assert!(outcome.detail_fallback);
    assert!(!outcome.selection_fallback);
    assert!(outcome.used_fallback());
}

#[test]
fn a_budgeted_game_finishes_and_reports_every_decision() {
    // `all_seats` is the "whole game is the thing being timed" shape; paired
    // evaluation uses `focus_only` instead (see the P1 regression below).
    let mut timings = DecisionTimings::new(10);
    let budget = BudgetedDecisions::all_seats(DecisionBudgetSpec::new(Duration::from_millis(10)));
    for players in [2, 4] {
        let mut game = game(players, 777);
        let policy = MctsPolicy {
            simulations: 100_000,
            tree_depth: 2,
        };
        let mut decisions = 0usize;
        play_to_end_with_budget(
            &mut game,
            &policy,
            &SimpleDetails,
            &SimpleHarvestDetails,
            Some(budget),
            |outcome| {
                decisions += 1;
                assert_eq!(outcome.budget_ms, 10);
                assert!(outcome.elapsed_ns > 0);
                timings.record(outcome);
            },
        )
        .unwrap();
        assert!(game.public.finished, "{players}p game must finish");
        assert!(decisions > 0);
    }
    assert_eq!(timings.count(), timings.elapsed_ns().len());
    assert!(timings.p95_whole_ms() <= timings.max_whole_ms());
    assert!(timings.max_detail_ms() >= 0.0);
    // The budget must actually bind: with a 100000-simulation cap an unbudgeted
    // game would be far slower than this ceiling.
    assert!(timings.max_whole_ms() < 5_000.0);
}

/// Review P1 (#168): the wall-clock budget must not reach the opponent seats.
///
/// An opponent whose detail generation can be replaced by the fallback because a
/// clock expired would make the comparison machine-dependent, and would let the
/// measured seat's result depend on how busy the machine was.
#[test]
fn only_the_focus_seat_is_budgeted() {
    for players in [2, 4] {
        let focus = players - 1;
        let mut game = game(players, 909);
        let policy = MctsPolicy {
            simulations: 8,
            tree_depth: 2,
        };
        let mut focus_outcomes = Vec::new();
        let mut opponent_outcomes = Vec::new();
        play_to_end_with_budget(
            &mut game,
            &policy,
            &SimpleDetails,
            &SimpleHarvestDetails,
            Some(BudgetedDecisions::focus_only(
                DecisionBudgetSpec::new(Duration::from_millis(5)),
                focus,
            )),
            |outcome| {
                if outcome.player == focus {
                    focus_outcomes.push(outcome);
                } else {
                    opponent_outcomes.push(outcome);
                }
            },
        )
        .unwrap();
        assert!(game.public.finished);
        assert!(!focus_outcomes.is_empty());
        assert!(!opponent_outcomes.is_empty());
        for outcome in &focus_outcomes {
            assert_eq!(outcome.budget_ms, 5, "the focus seat must be budgeted");
        }
        for outcome in &opponent_outcomes {
            assert_eq!(
                outcome.budget_ms, 0,
                "an opponent seat must not be budgeted"
            );
            assert_eq!(outcome.overshoot_ns, 0);
            assert!(!outcome.used_fallback());
        }
    }
}

/// A scope that contains no seat is the same as no budget at all, byte for byte.
/// This is the property that keeps `simulation-capped` replays identical even
/// though the budgeted entry point is used.
#[test]
fn a_scope_with_no_seats_reproduces_the_unbudgeted_game_exactly() {
    let run = |budget: Option<BudgetedDecisions>| {
        let mut game = game(2, 4242);
        let policy = MctsPolicy {
            simulations: 8,
            tree_depth: 2,
        };
        play_to_end_with_budget(
            &mut game,
            &policy,
            &SimpleDetails,
            &SimpleHarvestDetails,
            budget,
            |_| {},
        )
        .unwrap();
        (game.state_hash(), game.log.events.clone())
    };
    let unbudgeted = run(None);
    let explicit_no_seats = run(Some(BudgetedDecisions {
        spec: DecisionBudgetSpec::new(Duration::from_micros(1)),
        scope: BudgetScope::NoSeats,
    }));
    assert_eq!(unbudgeted.0, explicit_no_seats.0);
    assert_eq!(unbudgeted.1, explicit_no_seats.1);
}

/// The P1 scenario from the review: PUCT opponents with a very short focus
/// budget. The opponents must take the clock-free path, so two runs that differ
/// only in how long the *focus* seat was allowed to search still give every
/// opponent decision the same unbudgeted treatment, and the opponent's recorded
/// outcomes are identical in shape (no fallback, no overshoot).
#[test]
fn a_puct_opponent_is_never_affected_by_the_focus_budget() {
    let puct = PuctPolicy {
        config: PuctConfig {
            simulations: 8,
            ..PuctConfig::default()
        },
    };
    let mut opponent_rows = Vec::new();
    for budget_ms in [1u64, 50] {
        let mut game = game(2, 616);
        let mut focus_ns = Vec::new();
        play_to_end_with_budget(
            &mut game,
            &puct,
            &SimpleDetails,
            &SimpleHarvestDetails,
            Some(BudgetedDecisions::focus_only(
                DecisionBudgetSpec::new(Duration::from_millis(budget_ms)),
                0,
            )),
            |outcome| {
                if outcome.player == 0 {
                    focus_ns.push(outcome.elapsed_ns);
                } else {
                    opponent_rows.push((
                        outcome.budget_ms,
                        outcome.overshoot_ns,
                        outcome.used_fallback(),
                        outcome.elapsed_ns > 0,
                    ));
                }
            },
        )
        .unwrap();
        assert!(game.public.finished);
        assert!(!focus_ns.is_empty(), "the focus seat decided at least once");
    }
    // Every opponent decision in both runs took the clock-free path. The elapsed
    // time is still measured (it is an observation), but no budget applied.
    for (budget_ms, overshoot_ns, fallback, measured) in opponent_rows {
        assert_eq!(budget_ms, 0);
        assert_eq!(overshoot_ns, 0);
        assert!(!fallback);
        assert!(measured);
    }
}

#[test]
fn a_simulation_capped_game_stays_reproducible_under_the_new_loop() {
    // No budget must mean "no clock": two runs of the same seed and policy have
    // to produce the identical action log, which is what `simulation-capped`
    // evaluation and replay verification rely on.
    let run = || {
        let mut game = game(2, 314);
        let policy = MctsPolicy {
            simulations: 8,
            tree_depth: 2,
        };
        play_to_end_with_budget(
            &mut game,
            &policy,
            &SimpleDetails,
            &SimpleHarvestDetails,
            None,
            |_| {},
        )
        .unwrap();
        (game.state_hash(), game.log.events.clone())
    };
    let (first_hash, first_events) = run();
    let (second_hash, second_events) = run();
    assert_eq!(first_hash, second_hash);
    assert_eq!(first_events.len(), second_events.len());
    assert_eq!(first_events, second_events);
}

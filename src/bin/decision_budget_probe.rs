//! Whole-decision time-budget probe (#167 A).
//!
//! `mcts_profile --search-budget-sweep` measures only the search. #164 showed
//! why that is not enough: a 200 ms search budget produced a 203 ms *decision*,
//! because detail generation ran after the search returned. This binary measures
//! the budget the roadmap actually specifies, one complete decision
//! (selection **and** details) through [`resolve_decision`], which is the same
//! code path that plays evaluation games.
//!
//! It runs on a fixed opening position per player count, so the sum of the
//! budgets bounds the runtime (about 1/40 of a full game at the same cost per
//! decision). It is a measurement tool, not a strength benchmark.
//!
//! Usage:
//!   cargo run --release --bin decision_budget_probe
//!   cargo run --release --bin decision_budget_probe -- --players 4 --budgets 50,200,1000,10000
use agricola_re_ai::decision_budget::DecisionBudgetSpec;
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::mcts_puct::{PuctConfig, PuctPolicy};
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{resolve_decision, CardChoiceDetails, SimpleDetails};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::time::Duration;

fn main() {
    let mut players = vec![2usize, 4];
    let mut budgets = vec![50u64, 200, 1_000, 10_000];
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--players" => {
                players = args
                    .next()
                    .expect("--players needs a comma-separated list")
                    .split(',')
                    .map(|value| value.parse().expect("player count"))
                    .collect();
            }
            "--budgets" => {
                budgets = args
                    .next()
                    .expect("--budgets needs a comma-separated list")
                    .split(',')
                    .map(|value| value.parse().expect("budget in ms"))
                    .collect();
            }
            other => panic!("unsupported argument: {other}"),
        }
    }
    // The whole point is the wall clock, so a budget of zero would measure
    // nothing; reject it instead of reporting a meaningless row.
    assert!(
        budgets.iter().all(|&budget| budget > 0),
        "every budget must be positive"
    );

    let details = SimpleDetails;
    let card_details = CardChoiceDetails;
    let mut rows = Vec::new();
    for &player_count in &players {
        let game = Game::new(Config {
            players: player_count,
            seed: 12_345,
            additional_action_space_enabled: false,
        });
        let obs = game
            .observation(game.public.active_player)
            .expect("observation");
        let legal = game.legal_spaces();
        for &budget_ms in &budgets {
            for policy_name in ["mcts", "puct"] {
                // 100000 is the safety cap; the deadline is what stops the
                // search in this probe, exactly as in a time-budgeted game.
                let (space, outcome) = if policy_name == "mcts" {
                    let policy = MctsPolicy {
                        simulations: 100_000,
                        tree_depth: 2,
                    };
                    let mut rng = ChaCha8Rng::seed_from_u64(7);
                    resolve_decision(
                        &obs,
                        &legal,
                        &policy,
                        &details,
                        &mut rng,
                        Some(DecisionBudgetSpec::new(Duration::from_millis(budget_ms))),
                    )
                    .expect("mcts decision")
                } else {
                    let policy = PuctPolicy {
                        config: PuctConfig::default(),
                    };
                    let mut rng = ChaCha8Rng::seed_from_u64(7);
                    resolve_decision(
                        &obs,
                        &legal,
                        &policy,
                        &card_details,
                        &mut rng,
                        Some(DecisionBudgetSpec::new(Duration::from_millis(budget_ms))),
                    )
                    .expect("puct decision")
                };
                rows.push(json!({
                    "players": player_count,
                    "policy": policy_name,
                    "budget_ms": budget_ms,
                    "budget_used_fraction": (outcome.elapsed_ms() / budget_ms as f64).min(1.5),
                    "whole_decision_ms": outcome.elapsed_ms(),
                    "detail_ms": outcome.detail_ns as f64 / 1_000_000.0,
                    "overshoot_ms": outcome.overshoot_ms(),
                    "late": outcome.is_late(),
                    "detail_fallback": outcome.detail_fallback,
                    "budget_exhausted_before_details": outcome.budget_exhausted_before_details,
                    "selected_space_is_legal": legal.contains(&space.space),
                }));
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "probe": "whole_decision_budget",
            "note": "Observed compliance on one fixed opening position per player count. \
                     It is not a real-time guarantee and not a strength measurement.",
            "decisions": rows,
        }))
        .expect("probe JSON")
    );
}

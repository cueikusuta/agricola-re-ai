//! MCTS hot-path profiler for M8-3. Read-only; runs on a fixed position.
//! Reports the cost of a state clone and a full state hash (the two costs the
//! search-only fast path removes) plus MCTS decision time across simulation
//! counts. Output is one JSON object.
//!
//! It also sweeps the wall-clock search modes from #164, because that is the
//! only place where "how much search does a budget actually buy" can be
//! measured without paying for a whole game at 10 s per decision.
//! `--search-budget-sweep` is opt-in: the sweep costs the sum of its budgets.
use agricola_re_ai::mcts::{MctsPolicy, SearchLimit, SearchMode};
use agricola_re_ai::model::{Config, Game};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::time::Instant;

fn timed_us(repeats: usize, mut run: impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..repeats {
        run();
    }
    start.elapsed().as_secs_f64() * 1_000_000.0 / repeats as f64
}

fn main() {
    let game = Game::new(Config {
        players: 2,
        seed: 12_345,
        additional_action_space_enabled: false,
    });
    let obs = game.observation(game.public.active_player).unwrap();
    let legal = game.legal_spaces();
    let repeats = 200;

    let clone_us = timed_us(repeats, || {
        let _ = game.clone();
    });
    let state_hash_us = timed_us(repeats, || {
        let _ = game.state_hash();
    });
    let legal_spaces_us = timed_us(repeats, || {
        let _ = game.legal_spaces();
    });

    let mut sweeps = Vec::new();
    for simulations in [4usize, 16, 64, 256] {
        let policy = MctsPolicy {
            simulations,
            tree_depth: 2,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let start = Instant::now();
        let _ = policy.select_with_budget(&obs, &legal, &mut rng, None);
        let decision_ms = start.elapsed().as_secs_f64() * 1000.0;
        sweeps.push(json!({
            "simulations": simulations,
            "decision_ms": decision_ms,
            "ms_per_simulation": decision_ms / simulations as f64,
        }));
    }

    let budget_sweep = std::env::args().any(|arg| arg == "--search-budget-sweep");
    let mut time_budgets = Vec::new();
    if budget_sweep {
        for millis in [1_000u64, 3_000, 5_000, 10_000] {
            let policy = MctsPolicy {
                simulations: 1,
                tree_depth: 2,
            };
            let mut rng = ChaCha8Rng::seed_from_u64(7);
            let started = Instant::now();
            let (space, outcome) = policy
                .select_with_limit(
                    &obs,
                    &legal,
                    &mut rng,
                    SearchLimit::TimeBudget {
                        deadline: Instant::now() + std::time::Duration::from_millis(millis),
                        max_simulations: 100_000,
                    },
                )
                .expect("budgeted decision");
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            time_budgets.push(json!({
                "budget_ms": millis,
                "mode": SearchMode::TimeBudgeted.as_str(),
                "simulations_completed": outcome.simulations_completed,
                "elapsed_ms": elapsed_ms,
                "overshoot_ms": outcome.overshoot_ms,
                "timed_out": outcome.timed_out,
                "used_fallback": outcome.used_fallback,
                "budget_used_fraction": (elapsed_ms / millis as f64).min(1.5),
                "choice_is_legal": legal.contains(&space),
            }));
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "players": 2,
            "legal_spaces_us": legal_spaces_us,
            "clone_us": clone_us,
            "state_hash_us": state_hash_us,
            "hash_vs_clone_ratio": if clone_us > 0.0 { state_hash_us / clone_us } else { 0.0 },
            "sweeps": sweeps,
            "time_budget_sweep": time_budgets,
        }))
        .expect("profile JSON")
    );
}

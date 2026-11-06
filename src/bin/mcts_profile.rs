//! MCTS hot-path profiler for M8-3. Read-only; runs on a fixed position.
//! Reports the cost of a state clone and a full state hash (the two costs the
//! search-only fast path removes) plus MCTS decision time across simulation
//! counts. Output is one JSON object.
use agricola_re_ai::mcts::MctsPolicy;
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

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "players": 2,
            "legal_spaces_us": legal_spaces_us,
            "clone_us": clone_us,
            "state_hash_us": state_hash_us,
            "hash_vs_clone_ratio": if clone_us > 0.0 { state_hash_us / clone_us } else { 0.0 },
            "sweeps": sweeps,
        }))
        .expect("profile JSON")
    );
}

//! Minimal usage example for the M8-3 (#110) PUCT search.
//!
//! Run with `cargo run --release --example puct_search`. It builds one cardless
//! four-player position, asks the PUCT policy for a move, and reports the
//! search outcome. This is a demonstration, not an evaluation harness; use
//! `--bin paired_eval` for measured AI comparisons.
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::mcts_puct::{LeafMode, PuctConfig, PuctPolicy};
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::SpacePolicy;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::time::Instant;

fn main() {
    let game = Game::new(Config {
        players: 4,
        seed: 870000,
        additional_action_space_enabled: false,
    });
    let obs = game.observation(game.public.active_player).unwrap();
    let legal = game.legal_spaces();

    let baseline = MctsPolicy {
        simulations: 32,
        tree_depth: 2,
    };
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let start = Instant::now();
    let baseline_move = baseline.select(&obs, &legal, &mut rng);
    println!(
        "baseline MCTS 32sim/depth2 -> {baseline_move:?} in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let policy = PuctPolicy {
        config: PuctConfig {
            simulations: 32,
            tree_depth: 2,
            leaf_mode: LeafMode::Terminal,
            ..PuctConfig::default()
        },
    };
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let start = Instant::now();
    let outcome = policy.select_with_budget(&obs, &legal, &mut rng, None);
    match outcome {
        Some((space, outcome)) => println!(
            "PUCT {}/{}sim/depth{} -> {space:?} in {:.1}ms ({} simulations, fallback={})",
            match policy.config.leaf_mode {
                LeafMode::Terminal => "terminal",
                LeafMode::Heuristic => "heuristic",
            },
            policy.config.simulations,
            policy.config.tree_depth,
            start.elapsed().as_secs_f64() * 1000.0,
            outcome.simulations_completed,
            outcome.used_fallback,
        ),
        None => println!("PUCT found no legal move"),
    }
}

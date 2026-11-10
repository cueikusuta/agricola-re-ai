//! MCTS hot-path profiler for M8-3 (#109/#110) and the M13 bottleneck analysis
//! (#106). Read-only: it never changes a game, it only measures fixed positions.
//!
//! Output is one JSON object. Two measurement families are reported:
//!
//! * `sweeps` / `time_budget_sweep` — decision-level cost (kept for the earlier
//!   M8-3 evidence and the #164 time-budget sweep);
//! * `hotpath` (opt-in via `--hotpath`) — component costs inside one search:
//!   state sampling, clone, legal move generation, observation, detail
//!   generation, `apply`, one rollout, and one whole decision for both MCTS and
//!   PUCT, plus heap allocations per component.
//!
//! Allocation counting is provided by the library's `perf-count-alloc`
//! feature module (`agricola_re_ai::counting`), so this binary no longer
//! installs its own `#[global_allocator]` (two global allocators in one
//! process fail to compile). Allocation numbers must be read from runs
//! built with `--features perf-count-alloc` and not mixed with the timing
//! numbers of the uninstrumented runs.
#[cfg(feature = "perf-count-alloc")]
use agricola_re_ai::counting::{total_allocated_byte_count, total_allocation_count};

#[cfg(not(feature = "perf-count-alloc"))]
fn total_allocation_count() -> u64 {
    0
}
#[cfg(not(feature = "perf-count-alloc"))]
fn total_allocated_byte_count() -> u64 {
    0
}
use agricola_re_ai::mcts::{self, MctsPolicy, SearchLimit, SearchMode};
use agricola_re_ai::mcts_puct::{PuctConfig, PuctPolicy};
use agricola_re_ai::model::{Config, Game, Observation, Space};
use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, SimpleDetails, SpacePolicy};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::time::Instant;

fn timed_us(repeats: usize, mut run: impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..repeats {
        run();
    }
    start.elapsed().as_secs_f64() * 1_000_000.0 / repeats as f64
}

/// `(allocations, bytes)` charged to one call, averaged over `repeats`.
/// Reads the library's process-wide counters (`perf-count-alloc` build);
/// the plain build reports zeros.
fn allocations(repeats: usize, mut run: impl FnMut()) -> (f64, f64) {
    let before_allocations = total_allocation_count();
    let before_bytes = total_allocated_byte_count();
    for _ in 0..repeats {
        run();
    }
    let allocations = total_allocation_count() - before_allocations;
    let bytes = total_allocated_byte_count() - before_bytes;
    (
        allocations as f64 / repeats as f64,
        bytes as f64 / repeats as f64,
    )
}

fn components(obs: &Observation, legal: &[Space], game: &Game, action_space: Space) -> Value {
    let details = CardChoiceDetails;
    let simple = SimpleDetails;
    let mut rng = ChaCha8Rng::seed_from_u64(3);
    let sample_repeats = 100;
    let rollout_repeats = 20;
    let per_call_repeats = 200;

    let sample_us = timed_us(sample_repeats, || {
        let _ = mcts::sample_game(obs, &mut rng);
    });
    let clone_us = timed_us(per_call_repeats, || {
        let _ = game.clone();
    });
    let legal_us = timed_us(per_call_repeats, || {
        let _ = game.legal_spaces();
    });
    let observation_us = timed_us(per_call_repeats, || {
        let _ = game.observation(game.public.active_player);
    });
    let card_action = details.produce(obs, action_space).expect("card detail");
    let _ = simple.produce(obs, action_space).expect("simple detail");
    let card_detail_us = timed_us(per_call_repeats, || {
        let _ = details.produce(obs, action_space);
    });
    let simple_detail_us = timed_us(per_call_repeats, || {
        let _ = simple.produce(obs, action_space);
    });
    // `apply` is timed on a pre-made copy so the clone is not charged twice.
    let mut apply_ns = 0.0f64;
    for _ in 0..per_call_repeats {
        let mut copy = game.clone();
        let start = Instant::now();
        copy.apply(card_action.clone()).expect("legal action");
        apply_ns += start.elapsed().as_secs_f64() * 1_000_000.0;
    }
    let apply_us = apply_ns / per_call_repeats as f64;

    // The search plays rollouts on a *sampled* position (`record_events =
    // false`), so the profiler must too; measuring the real game would charge
    // event logging and state hashing that the search never performs.
    let mut sampled_rng = ChaCha8Rng::seed_from_u64(5);
    let sampled = mcts::sample_game(obs, &mut sampled_rng).expect("sampled position");
    let sampled_clone_us = timed_us(per_call_repeats, || {
        let _ = sampled.clone();
    });
    let mut rollout_us = 0.0f64;
    let mut rollout_moves = 0usize;
    for _ in 0..rollout_repeats {
        let mut copy = sampled.clone();
        let before =
            copy.public.workers_placed.iter().sum::<u8>() as usize + usize::from(copy.public.round);
        let mut rollout_rng = ChaCha8Rng::seed_from_u64(11);
        let start = Instant::now();
        mcts::rollout(&mut copy, &mut rollout_rng, None).expect("rollout");
        rollout_us += start.elapsed().as_secs_f64() * 1_000_000.0;
        let after =
            copy.public.workers_placed.iter().sum::<u8>() as usize + usize::from(copy.public.round);
        rollout_moves += after.abs_diff(before).max(1) + 14;
    }
    rollout_us /= rollout_repeats as f64;
    let rollout_moves = rollout_moves as f64 / rollout_repeats as f64;

    // Evaluation-policy selection is what the rollout calls on every move.
    let evaluation = agricola_re_ai::evaluation::DetailAwareEvaluationPolicy {
        weights: agricola_re_ai::evaluation::WeightedEvaluationPolicy {
            prior_weight: 0.5,
            state_weight: 2.0,
        },
        details: &simple,
    };
    let mut eval_rng = ChaCha8Rng::seed_from_u64(4);
    let eval_us = timed_us(per_call_repeats, || {
        let _ = evaluation.select(obs, legal, &mut eval_rng);
    });
    // `apply` on a search position: includes the transactional clone inside
    // `apply_with_plans`, which is charged to `apply` on purpose.
    let mut sampled_apply_ns = 0.0f64;
    for _ in 0..per_call_repeats {
        let mut copy = sampled.clone();
        let action = details.produce(obs, action_space).expect("detail");
        let start = Instant::now();
        copy.apply(action).expect("apply");
        sampled_apply_ns += start.elapsed().as_secs_f64() * 1_000_000.0;
    }

    let (sample_allocs, sample_bytes) = allocations(sample_repeats, || {
        let _ = mcts::sample_game(obs, &mut rng);
    });
    let (rollout_allocs, rollout_bytes) = allocations(5, || {
        let mut copy = sampled.clone();
        let mut rollout_rng = ChaCha8Rng::seed_from_u64(11);
        let _ = mcts::rollout(&mut copy, &mut rollout_rng, None);
    });
    let (decision_allocs, decision_bytes) = allocations(3, || {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let _ = MctsPolicy {
            simulations: 32,
            tree_depth: 2,
        }
        .select_with_limit(obs, legal, &mut rng, SearchLimit::Simulations(32));
    });

    json!({
        "sample_game_us": sample_us,
        "clone_us": clone_us,
        "legal_spaces_us": legal_us,
        "observation_us": observation_us,
        "card_detail_us": card_detail_us,
        "simple_detail_us": simple_detail_us,
        "apply_us": apply_us,
        "rollout_us": rollout_us,
        "sampled_clone_us": sampled_clone_us,
        "evaluation_select_us": eval_us,
        "sampled_apply_us": sampled_apply_ns / per_call_repeats as f64,
        "rollout_moves": rollout_moves,
        "us_per_rollout_move": if rollout_moves > 0.0 { rollout_us / rollout_moves } else { 0.0 },
        "sample_game_allocations": sample_allocs,
        "sample_game_allocated_bytes": sample_bytes,
        "rollout_allocations": rollout_allocs,
        "rollout_allocated_bytes": rollout_bytes,
        "decision32_allocations": decision_allocs,
        "decision32_allocated_bytes": decision_bytes,
        "action_space": format!("{action_space:?}"),
        "legal_moves": legal.len(),
        "uses_cards": !obs.card_definitions.is_empty(),
    })
}

/// One fixed-budget MCTS decision, used to compare the same measurement point
/// before and after a hot-path change.
fn decision_on(obs: &Observation, legal: &[Space], simulations: usize) -> Value {
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let started = Instant::now();
    let (choice, outcome) = MctsPolicy {
        simulations,
        tree_depth: 2,
    }
    .select_with_limit(obs, legal, &mut rng, SearchLimit::Simulations(simulations))
    .expect("mcts decision");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    json!({
        "simulations": simulations,
        "completed": outcome.simulations_completed,
        "decision_ms": elapsed_ms,
        "ms_per_simulation": elapsed_ms / simulations as f64,
        "choice": format!("{choice:?}"),
    })
}

fn decision_on_puct(obs: &Observation, legal: &[Space]) -> Value {
    let simulations = PuctConfig::default().simulations;
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let started = Instant::now();
    let (choice, outcome) = PuctPolicy::default()
        .select_with_limit(obs, legal, &mut rng, SearchLimit::Simulations(simulations))
        .expect("puct decision");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    json!({
        "simulations": simulations,
        "completed": outcome.simulations_completed,
        "decision_ms": elapsed_ms,
        "ms_per_simulation": elapsed_ms / simulations as f64,
        "choice": format!("{choice:?}"),
    })
}

fn decision(mcts_sims: usize) -> Value {
    let game = Game::new(Config {
        players: 2,
        seed: 12_345,
        additional_action_space_enabled: false,
    });
    let obs = game.observation(game.public.active_player).unwrap();
    let legal = game.legal_spaces();
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let started = Instant::now();
    let (choice, outcome) = MctsPolicy {
        simulations: mcts_sims,
        tree_depth: 2,
    }
    .select_with_limit(&obs, &legal, &mut rng, SearchLimit::Simulations(mcts_sims))
    .expect("mcts decision");
    let mcts_ms = started.elapsed().as_secs_f64() * 1000.0;

    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let started = Instant::now();
    let (puct_choice, puct_outcome) = PuctPolicy::default()
        .select_with_limit(
            &obs,
            &legal,
            &mut rng,
            SearchLimit::Simulations(PuctConfig::default().simulations),
        )
        .expect("puct decision");
    let puct_ms = started.elapsed().as_secs_f64() * 1000.0;

    json!({
        "mcts": {
            "simulations": mcts_sims,
            "completed": outcome.simulations_completed,
            "decision_ms": mcts_ms,
            "ms_per_simulation": mcts_ms / mcts_sims as f64,
            "choice": format!("{choice:?}"),
        },
        "puct": {
            "simulations": PuctConfig::default().simulations,
            "completed": puct_outcome.simulations_completed,
            "decision_ms": puct_ms,
            "ms_per_simulation": puct_ms / PuctConfig::default().simulations as f64,
            "choice": format!("{puct_choice:?}"),
        },
    })
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

    // `--golden` records the exact choice of the three policies for fixed
    // seeds so a refactor can be proven behaviour-preserving.
    let mut golden = Value::Null;
    if std::env::args().any(|arg| arg == "--golden") {
        let mut rows = Vec::new();
        for seed in 0..8u64 {
            let game = Game::new(Config {
                players: 2,
                seed,
                additional_action_space_enabled: false,
            });
            let obs = game.observation(game.public.active_player).unwrap();
            let legal = game.legal_spaces();
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let evaluation =
                agricola_re_ai::evaluation::EvaluationPolicy.select(&obs, &legal, &mut rng);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mcts = MctsPolicy {
                simulations: 16,
                tree_depth: 2,
            }
            .select(&obs, &legal, &mut rng);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let puct = PuctPolicy::default().select(&obs, &legal, &mut rng);
            rows.push(json!({
                "seed": seed,
                "evaluation": evaluation.map(|s| format!("{s:?}")),
                "mcts16": mcts.map(|s| format!("{s:?}")),
                "puct": puct.map(|s| format!("{s:?}")),
            }));
        }
        let card_game = Game::with_cards(
            Config {
                players: 2,
                seed: 4242,
                additional_action_space_enabled: false,
            },
            agricola_re_ai::cards::verified_basic_card_set(2).expect("verified pool"),
        )
        .expect("card game");
        let obs = card_game
            .observation(card_game.public.active_player)
            .unwrap();
        let legal = card_game.legal_spaces();
        for seed in 0..4u64 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let evaluation = agricola_re_ai::evaluation::DetailAwareEvaluationPolicy {
                weights: agricola_re_ai::evaluation::WeightedEvaluationPolicy {
                    prior_weight: 0.5,
                    state_weight: 2.0,
                },
                details: &CardChoiceDetails,
            }
            .select(&obs, &legal, &mut rng);
            rows.push(json!({
                "cards_seed": seed,
                "evaluation": evaluation.map(|s| format!("{s:?}")),
            }));
        }
        golden = json!(rows);
    }

    let mut hotpath = Value::Null;
    if std::env::args().any(|arg| arg == "--hotpath") {
        let cardless = Game::new(Config {
            players: 2,
            seed: 12_345,
            additional_action_space_enabled: false,
        });
        let cardless_obs = cardless.observation(cardless.public.active_player).unwrap();
        let cardless_legal = cardless.legal_spaces();
        let cards = Game::with_cards(
            Config {
                players: 2,
                seed: 777,
                additional_action_space_enabled: false,
            },
            agricola_re_ai::cards::verified_basic_card_set(2).expect("verified 2p pool"),
        )
        .expect("card game");
        let cards_obs = cards.observation(cards.public.active_player).unwrap();
        let cards_legal = cards.legal_spaces();
        hotpath = json!({
            "cardless_2p": components(
                &cardless_obs,
                &cardless_legal,
                &cardless,
                cardless_legal[0]
            ),
            "cards_2p": components(&cards_obs, &cards_legal, &cards, cards_legal[0]),
            "decision_32": decision(32),
            "decision_128": decision(128),
            "decision_cards_32": {
                "mcts": decision_on(&cards_obs, &cards_legal, 32),
                "puct": decision_on_puct(&cards_obs, &cards_legal),
            },
        });
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
            "hotpath": hotpath,
            "golden": golden,
        }))
        .expect("profile JSON")
    );
}

//! #237 / #236 Step 0: read-only performance baseline for the current engine.
//!
//! Fixed seeds and fixed scenarios; the harness never changes engine behaviour.
//! It only measures it, so the same binary can be run before and after the
//! card-engine v2 migration and the numbers compared on identical terms
//! (#236 §8: measure *before* claiming any speedup).
//!
//! Output is one JSON object on stdout:
//! * `environment` — CPU model, logical cores, and RSS when available;
//! * `scenarios` — per-operation microbenchmarks (clone, definition lookup,
//!   CardSet/CardState validate, observation, state_hash, validate_action,
//!   apply on the search and committed paths, sample_game, save size);
//! * `fixed_sim_searches` — MCTS and PUCT at a fixed simulation cap;
//! * `ten_second_searches` — wall-clock 10s budgets, the per-move budget of
//!   AGENTS.md rule 11.
//!
//! Allocation counting installs a counting global allocator in this binary
//! only. Read allocation numbers from `allocations` fields; timing numbers
//! come from uninstrumented loops in the same run (the wrapper only counts,
//! it does not change allocation behaviour).
//!
//! Scenarios cover cardless 2/3/4 players and the implemented two-player card
//! pool. Unimplemented card pools are deliberately not extrapolated (#236 §8).

use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::mcts::{self, MctsPolicy, SearchLimit};
use agricola_re_ai::mcts_puct::PuctPolicy;
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, SimpleDetails};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Counts heap allocations/bytes without changing allocation behaviour.
struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Mean microseconds per call over `repeats` calls, after one warmup call.
fn timed_us(repeats: usize, mut run: impl FnMut()) -> f64 {
    run(); // warmup
    let start = Instant::now();
    for _ in 0..repeats {
        run();
    }
    start.elapsed().as_secs_f64() * 1_000_000.0 / repeats as f64
}

/// `(allocations, allocated_bytes)` charged to one call, averaged, uninstrumented
/// for behaviour: the counting allocator only observes.
fn allocations(repeats: usize, mut run: impl FnMut()) -> (f64, f64) {
    let before_calls = ALLOCATIONS.load(Ordering::Relaxed);
    let before_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
    for _ in 0..repeats {
        run();
    }
    let calls = ALLOCATIONS.load(Ordering::Relaxed) - before_calls;
    let bytes = ALLOCATED_BYTES.load(Ordering::Relaxed) - before_bytes;
    (calls as f64 / repeats as f64, bytes as f64 / repeats as f64)
}

fn first_slash_field(file: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(file)
        .ok()?
        .lines()
        .find(|line| line.starts_with(key))
        .map(|line| line[key.len()..].trim().to_string())
}

fn environment() -> Value {
    json!({
        "cpu_model": first_slash_field("/proc/cpuinfo", "model name"),
        "logical_cores": std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        "rss_kib": first_slash_field("/proc/self/status", "VmRSS:"),
    })
}

/// The first action `apply` accepts, found read-only via `validate_action`, so
/// the same legality path that human input and AI selections share picks the
/// measured action. Never falls back to a hand-written detail.
fn first_legal_action(game: &Game, use_cards: bool) -> agricola_re_ai::engine::Action {
    let active = game.public.active_player;
    let obs = game.observation(active).expect("valid player");
    let legal = game.legal_spaces();
    let details: &dyn DetailPolicy = if use_cards {
        &CardChoiceDetails
    } else {
        &SimpleDetails
    };
    for &space in &legal {
        if let Some(action) = details.produce(&obs, space) {
            if game.validate_action(&action).is_ok() {
                return action;
            }
        }
    }
    panic!("no legal action found for the baseline scenario");
}

/// Per-operation microbenchmarks for one fixed starting position.
fn scenario_metrics(name: &str, game: &Game, use_cards: bool) -> Value {
    let repeats = 200;
    let sample_repeats = 100;
    let players = game.config.players;
    let active = game.public.active_player;
    let obs = game.observation(active).expect("valid player");
    let action = first_legal_action(game, use_cards);

    let clone_us = timed_us(repeats, || {
        let _ = game.clone();
    });
    let (clone_allocs, clone_bytes) = allocations(repeats, || {
        let _ = game.clone();
    });
    let state_hash_us = timed_us(repeats, || {
        let _ = game.state_hash();
    });
    let cardset_validate_us = timed_us(repeats, || {
        let _ = game.card_set.validate(players);
    });
    let cardstate_validate_us = timed_us(repeats, || {
        let _ = game.cards.validate(&game.card_set, &game.public.players);
    });
    let observation_us = timed_us(repeats, || {
        let _ = game.observation(active);
    });
    let (observation_allocs, observation_bytes) = allocations(repeats, || {
        let _ = game.observation(active);
    });
    let validate_action_us = timed_us(repeats, || {
        let _ = game.validate_action(&action);
    });

    // Apply on the search path: `record_events = false`, the state every MCTS
    // simulation actually mutates. The outer clone is not timed; the clone
    // inside `apply_with_plans` is a real cost of `apply` and stays inside.
    let mut apply_search_us = 0.0f64;
    for _ in 0..repeats {
        let mut copy = game.clone();
        copy.record_events = false;
        let start = Instant::now();
        copy.apply(action.clone()).expect("legal search apply");
        apply_search_us += start.elapsed().as_secs_f64() * 1_000_000.0;
    }
    apply_search_us /= repeats as f64;
    // Apply on the committed path: full event logging plus before/after state
    // hashing, i.e. what a recorded human or AI move pays.
    let mut apply_committed_us = 0.0f64;
    for _ in 0..repeats {
        let mut copy = game.clone();
        let start = Instant::now();
        copy.apply(action.clone()).expect("legal committed apply");
        apply_committed_us += start.elapsed().as_secs_f64() * 1_000_000.0;
    }
    apply_committed_us /= repeats as f64;

    let save_json_bytes = game.save_json().expect("serializable game").len();

    let mut sample_rng = ChaCha8Rng::seed_from_u64(0x23E0_BA5E_0000_0001);
    let sample_game_us = timed_us(sample_repeats, || {
        let _ = mcts::sample_game(&obs, &mut sample_rng);
    });
    let (sample_game_allocs, sample_game_bytes) = allocations(sample_repeats, || {
        let _ = mcts::sample_game(&obs, &mut sample_rng);
    });

    // Linear `CardSet::definition` lookup: mean over every definition id and
    // the worst case (the last definition in the vector).
    let ids: Vec<String> = game
        .card_set
        .definitions
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let (definition_lookup_total_us, definition_lookup_worst_us) = if ids.is_empty() {
        (None, None)
    } else {
        let total = timed_us(repeats, || {
            for id in &ids {
                let _ = game.card_set.definition(id);
            }
        });
        let worst = timed_us(repeats, || {
            let _ = game.card_set.definition(ids.last().expect("nonempty"));
        });
        (Some(total / ids.len() as f64), Some(worst))
    };

    json!({
        "name": name,
        "players": players,
        "additional_action_space_enabled": game.config.additional_action_space_enabled,
        "uses_cards": use_cards,
        "card_definitions": game.card_set.definitions.len(),
        "round": game.public.round,
        "legal_spaces": game.legal_spaces().len(),
        "clone_us": clone_us,
        "clone_allocations": clone_allocs,
        "clone_allocated_bytes": clone_bytes,
        "state_hash_us": state_hash_us,
        "cardset_validate_us": cardset_validate_us,
        "cardstate_validate_us": cardstate_validate_us,
        "definition_lookup_mean_us": definition_lookup_total_us,
        "definition_lookup_worst_us": definition_lookup_worst_us,
        "observation_us": observation_us,
        "observation_allocations": observation_allocs,
        "observation_allocated_bytes": observation_bytes,
        "validate_action_us": validate_action_us,
        "apply_search_us": apply_search_us,
        "apply_committed_us": apply_committed_us,
        "save_json_bytes": save_json_bytes,
        "sample_game_us": sample_game_us,
        "sample_game_allocations": sample_game_allocs,
        "sample_game_allocated_bytes": sample_game_bytes,
    })
}

/// One fixed-cap search decision. Deterministic for a fixed seed, so completed
/// simulations must match across runs on the same code.
fn fixed_sim_search(name: &str, game: &Game, puct: bool, simulations: usize) -> Value {
    let active = game.public.active_player;
    let obs = game.observation(active).expect("valid player");
    let legal = game.legal_spaces();
    let mut rng = ChaCha8Rng::seed_from_u64(0x23E0_5EED_0000_0002);
    let started = Instant::now();
    let (choice, outcome) = if puct {
        PuctPolicy::default().select_with_limit(
            &obs,
            &legal,
            &mut rng,
            SearchLimit::Simulations(simulations),
        )
    } else {
        MctsPolicy {
            simulations,
            tree_depth: 2,
        }
        .select_with_limit(
            &obs,
            &legal,
            &mut rng,
            SearchLimit::Simulations(simulations),
        )
    }
    .expect("search decision");
    let elapsed = started.elapsed();
    let elapsed_s = elapsed.as_secs_f64();
    json!({
        "name": name,
        "policy": if puct { "puct" } else { "mcts" },
        "simulations_attempted": simulations,
        "simulations_completed": outcome.simulations_completed,
        "attempted_per_sec": simulations as f64 / elapsed_s,
        "completed_per_sec": outcome.simulations_completed as f64 / elapsed_s,
        "elapsed_ms": elapsed.as_millis(),
        "choice": format!("{choice:?}"),
        "choice_is_legal": legal.contains(&choice),
    })
}

/// One wall-clock budget search. The completion count is machine-dependent by
/// design: it answers "how much search fits the per-move budget".
fn budgeted_search(name: &str, game: &Game, puct: bool, budget_ms: u64) -> Value {
    let active = game.public.active_player;
    let obs = game.observation(active).expect("valid player");
    let legal = game.legal_spaces();
    let mut rng = ChaCha8Rng::seed_from_u64(0x23E0_5EED_0000_0003);
    let started = Instant::now();
    let (choice, outcome) = if puct {
        PuctPolicy::default().select_with_limit(
            &obs,
            &legal,
            &mut rng,
            SearchLimit::TimeBudget {
                deadline: Instant::now() + Duration::from_millis(budget_ms),
                max_simulations: 1_000_000,
            },
        )
    } else {
        MctsPolicy {
            simulations: 1,
            tree_depth: 2,
        }
        .select_with_limit(
            &obs,
            &legal,
            &mut rng,
            SearchLimit::TimeBudget {
                deadline: Instant::now() + Duration::from_millis(budget_ms),
                max_simulations: 1_000_000,
            },
        )
    }
    .expect("budgeted decision");
    json!({
        "name": name,
        "policy": if puct { "puct" } else { "mcts" },
        "budget_ms": budget_ms,
        "elapsed_ms": started.elapsed().as_millis(),
        "overshoot_ms": outcome.overshoot_ms,
        "simulations_completed": outcome.simulations_completed,
        "completed_per_sec": outcome.simulations_completed as f64
            / started.elapsed().as_secs_f64(),
        "timed_out": outcome.timed_out,
        "used_fallback": outcome.used_fallback,
        "choice_is_legal": legal.contains(&choice),
    })
}

fn cardless_config(players: usize, additional: bool) -> Config {
    Config {
        seed: 0x23E0_0000_0000_0BA5,
        players,
        additional_action_space_enabled: additional,
    }
}

fn main() {
    let quick = std::env::args().any(|arg| arg == "--quick");

    let cardless_2p = Game::new(cardless_config(2, false));
    let cardless_2p_on = Game::new(cardless_config(2, true));
    let cardless_3p = Game::new(cardless_config(3, false));
    let cardless_4p = Game::new(cardless_config(4, false));
    let cards_2p = {
        let set = verified_basic_card_set(2).expect("implemented two-player pool");
        let game =
            Game::with_cards(cardless_config(2, false), set.clone()).expect("card game setup");
        // `with_cards` already validated; one more full validation here would
        // double-count setup, so only assert the hands exist.
        assert_eq!(game.cards.hands.len(), 2);
        game
    };

    let scenarios = vec![
        scenario_metrics("cardless_2p_off", &cardless_2p, false),
        scenario_metrics("cardless_2p_on", &cardless_2p_on, false),
        scenario_metrics("cardless_3p_off", &cardless_3p, false),
        scenario_metrics("cardless_4p_off", &cardless_4p, false),
        scenario_metrics("cards_2p_off", &cards_2p, true),
    ];

    let fixed_searches = {
        let scenarios = [
            ("cardless_2p_off", &cardless_2p),
            ("cardless_3p_off", &cardless_3p),
            ("cardless_4p_off", &cardless_4p),
            ("cards_2p_off", &cards_2p),
        ];
        let mut out = Vec::with_capacity(scenarios.len() * 2);
        for (name, game) in scenarios {
            out.push(fixed_sim_search(name, game, false, 32));
            out.push(fixed_sim_search(name, game, true, 32));
        }
        out
    };

    let ten_second = {
        if !quick {
            let scenarios = [
                ("cardless_2p_off", &cardless_2p),
                ("cards_2p_off", &cards_2p),
            ];
            let mut out = Vec::with_capacity(scenarios.len() * 2);
            for (name, game) in scenarios {
                out.push(budgeted_search(name, game, false, 10_000));
                out.push(budgeted_search(name, game, true, 10_000));
            }
            out
        } else {
            Vec::new()
        }
    };

    let output = json!({
        "harness": "perf_baseline",
        "harness_version": 1,
        "purpose": "#237 Step 0 baseline; measure before any v2 optimization",
        "engine_version": agricola_re_ai::model::ENGINE_VERSION,
        "rules_version": agricola_re_ai::model::RULES_VERSION,
        "log_version": agricola_re_ai::model::LOG_VERSION,
        "seed": 0x23E0_0000_0000_0BA5u64,
        "quick_mode": quick,
        "environment": environment(),
        "scenarios": scenarios,
        "fixed_sim_searches": fixed_searches,
        "ten_second_searches": ten_second,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("baseline JSON")
    );
}

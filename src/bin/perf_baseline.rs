//! #237 / #236 Step 0: read-only performance baseline for the current engine.
//!
//! Fixed seeds and fixed scenarios; the harness never changes engine behaviour.
//! It only measures it, so the same binary can be run before and after the
//! card-engine v2 migration and the numbers compared on identical terms
//! (#236 §8: measure *before* claiming any speedup).
//!
//! # Review fixes over harness v1 (PR #238, external review)
//!
//! * **P1 allocator overhead**: the binary is built twice. The timing run
//!   uses the plain build (`perf-count-alloc` OFF, normal `System`
//!   allocator); the allocation run uses `--features perf-count-alloc`,
//!   which installs the counting global allocator. Each JSON records
//!   `alloc_instrumented`, so a timing report can never silently come from
//!   the instrumented build, and atomic counter overhead never contaminates
//!   timing numbers.
//! * **P1 optimisation elision**: every measured closure sends its result
//!   through [`std::hint::black_box`], so the optimiser cannot elide work
//!   whose result is discarded. Success-required operations (validate /
//!   apply / sample) assert on failure: a failing operation fails the whole
//!   harness instead of being silently faster.
//! * **P2 statistics**: microbenchmarks run `BATCHES` batches and report
//!   median / p95 / min / max / mean / stddev instead of a single mean.
//! * **P2 phases**: each condition is measured at three phases — early
//!   (round 1), mid (round 8) and late (round 14) — advanced from the fixed
//!   seed with the deterministic detail policies, so log growth and played
//!   cards over a real game are included, not just the opening position.
//! * **P2 conditions**: cardless 2/3/4 players × additional-space OFF/ON
//!   plus cards 2p OFF/ON (the implemented pool) — the full feasible
//!   condition matrix.
//!
//! Output is one JSON object on stdout. Rendering scripts:
//! `scripts/perf_display.py` (console) and `scripts/perf_summary.py`
//! (GitHub step summary).

use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::mcts::{self, MctsPolicy, SearchLimit};
use agricola_re_ai::mcts_puct::PuctPolicy;
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{
    CardChoiceDetails, DetailPolicy, RandomPolicy, SimpleDetails, SpacePolicy,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::hint::black_box;
use std::time::{Duration, Instant};

// The counting allocator is compiled only behind `perf-count-alloc`, so the
// timing build pays no counting overhead at all (review P1).
#[cfg(feature = "perf-count-alloc")]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Counts heap allocations/bytes. Only used in the dedicated allocation
    /// run; timing runs use the plain build and the `System` allocator.
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

    pub fn allocation_count() -> u64 {
        ALLOCATIONS.load(Ordering::Relaxed)
    }
    pub fn allocated_byte_count() -> u64 {
        ALLOCATED_BYTES.load(Ordering::Relaxed)
    }
    pub const INSTRUMENTED: bool = true;
}

#[cfg(not(feature = "perf-count-alloc"))]
mod counting {
    pub fn allocation_count() -> u64 {
        0
    }
    pub fn allocated_byte_count() -> u64 {
        0
    }
    pub const INSTRUMENTED: bool = false;
}

use counting::{allocated_byte_count, allocation_count, INSTRUMENTED};

/// Microbenchmark statistics over several batches (review P2: stability).
#[derive(Clone, Copy, Debug)]
struct Stats {
    median_us: f64,
    p95_us: f64,
    min_us: f64,
    max_us: f64,
    mean_us: f64,
    stddev_us: f64,
}

const BATCHES: usize = 20;
const REPEATS_PER_BATCH: usize = 30;
const SAMPLE_REPEATS_PER_BATCH: usize = 15;

fn stats_from_batch_us(batch_us: &[f64]) -> Stats {
    assert!(!batch_us.is_empty(), "no batches");
    let mut sorted = batch_us.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite times"));
    let n = sorted.len();
    let median = sorted[n / 2];
    // Nearest-rank percentile: p95 of n samples is the ceil(0.95*n)-th
    // smallest value. For n=20 that is the 19th (index 18), never the
    // maximum, so p95 stays below `max` and reflects the distribution tail.
    let p95_rank = (n as f64 * 0.95).ceil() as usize;
    let p95 = sorted[p95_rank.clamp(1, n) - 1];
    let min = sorted[0];
    let max = sorted[n - 1];
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let var = sorted.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
    Stats {
        median_us: median,
        p95_us: p95,
        min_us: min,
        max_us: max,
        mean_us: mean,
        stddev_us: var.sqrt(),
    }
}

fn stats_json(s: &Stats) -> Value {
    json!({
        "median_us": s.median_us,
        "p95_us": s.p95_us,
        "min_us": s.min_us,
        "max_us": s.max_us,
        "mean_us": s.mean_us,
        "stddev_us": s.stddev_us,
    })
}

/// Measure `run` over `BATCHES` batches of `repeats` iterations. One warmup
/// call precedes the batches. Callers consume results through `black_box`
/// so nothing can be elided.
fn batched_stats(repeats: usize, mut run: impl FnMut()) -> Stats {
    run(); // warmup
    let mut batch_us = Vec::with_capacity(BATCHES);
    for _ in 0..BATCHES {
        let start = Instant::now();
        for _ in 0..repeats {
            run();
        }
        batch_us.push(start.elapsed().as_secs_f64() * 1_000_000.0 / repeats as f64);
    }
    stats_from_batch_us(&batch_us)
}

/// `(allocations, allocated_bytes)` charged to one call, averaged. Only
/// meaningful in the `perf-count-alloc` build; the plain build returns
/// zeros (the workflow reads these fields only from the instrumented run).
/// One warmup call first. `run` must be deterministic.
fn allocations_per_call(repeats: usize, mut run: impl FnMut()) -> (f64, f64) {
    run(); // warmup
    let before_calls = allocation_count();
    let before_bytes = allocated_byte_count();
    for _ in 0..repeats {
        run();
    }
    let calls = allocation_count() - before_calls;
    let bytes = allocated_byte_count() - before_bytes;
    (calls as f64 / repeats as f64, bytes as f64 / repeats as f64)
}

fn first_slash_field(file: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(file)
        .ok()?
        .lines()
        .find(|line| line.starts_with(key))
        .map(|line| line[key.len()..].trim().to_string())
}

/// Reads `/proc/cpuinfo` and `/proc/self/status` on Linux.
/// On non-Linux platforms (Windows, macOS) these fields will be `null`.
/// CI runs on ubuntu-24.04 so the data is available there.
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

/// Advance a clone of `game` deterministically until `target_round`, or as
/// far as the game allows without finishing. Fixed seed: the same position
/// is always reached from the same starting game, so before/after
/// comparisons see identical inputs (review P2: mid/late phases, not just
/// the opening). Uses the guaranteed-legal detail policies, so every move
/// is legal or the harness fails.
///
/// Returns the advanced game and a replay record containing the seed, method,
/// number of moves applied, and the final state_hash for identity verification.
fn advance_to_round(game: &Game, target_round: u8) -> (Game, Value) {
    let mut g = game.clone();
    if g.public.finished {
        let state_hash = g.state_hash();
        return (
            g,
            json!({"seed": 0x23E0_5EED_000A_0004u64, "method": "RandomPolicy", "moves": 0, "state_hash": state_hash}),
        );
    }
    let mut rng = ChaCha8Rng::seed_from_u64(0x23E0_5EED_000A_0004);
    let space_policy = RandomPolicy;
    let mut moves = 0usize;
    while !g.public.finished && g.public.round < target_round && moves < 640 {
        let acting = g.public.active_player;
        let obs = g.observation(acting).expect("advance loop observation");
        let legal = g.legal_spaces();
        assert!(
            !legal.is_empty(),
            "no legal move in round {} for player {}",
            g.public.round,
            acting
        );
        let space = space_policy
            .select(&obs, &legal, &mut rng)
            .expect("random policy always selects");
        let details: &dyn DetailPolicy = if game.card_set.enabled() {
            &CardChoiceDetails
        } else {
            &SimpleDetails
        };
        let action = details
            .produce(&obs, space)
            .expect("details always produce for a legal space");
        g.apply(action).expect("advance loop apply");
        moves += 1;
    }
    let state_hash = g.state_hash();
    (
        g,
        json!({
            "seed": 0x23E0_5EED_000A_0004u64,
            "method": "RandomPolicy",
            "moves": moves,
            "state_hash": state_hash,
        }),
    )
}

/// Per-operation microbenchmarks for one fixed position.
///
/// Timing is valid in any build (results are consumed through `black_box`).
/// All success-required operations assert: validate must return Ok, apply
/// must apply, sample must return a game — a failing operation fails the
/// whole harness instead of being silently faster (review P1: failure must
/// not look like a speedup).
fn scenario_metrics(name: &str, game: &Game, use_cards: bool, advance_info: Value) -> Value {
    let repeats = REPEATS_PER_BATCH;
    let sample_repeats = SAMPLE_REPEATS_PER_BATCH;
    let players = game.config.players;
    let active = game.public.active_player;
    let obs = game.observation(active).expect("valid player");
    let action = first_legal_action(game, use_cards);
    let round = game.public.round;

    let clone_stats = batched_stats(repeats, || {
        let copy = game.clone();
        black_box(&copy);
    });
    let state_hash_stats = batched_stats(repeats, || {
        let h = game.state_hash();
        black_box(&h);
    });
    let cardset_validate_stats = batched_stats(repeats, || {
        game.card_set
            .validate(players)
            .expect("CardSet::validate must succeed");
    });
    let cardstate_validate_stats = batched_stats(repeats, || {
        game.cards
            .validate(&game.card_set, &game.public.players)
            .expect("CardState::validate must succeed");
    });
    let observation_stats = batched_stats(repeats, || {
        let o = game.observation(active).expect("observation");
        black_box(&o);
    });
    let validate_action_stats = batched_stats(repeats, || {
        game.validate_action(&action)
            .expect("validate_action must succeed");
    });

    // Apply on the search path: `record_events = false`, the state every MCTS
    // simulation actually mutates. The clones are created OUTSIDE the timed
    // section of each batch (the clone inside `apply_with_plans` is a real
    // cost of `apply` and stays inside). A fresh copy per iteration is
    // required: after one apply the space is `used`, so reusing a copy would
    // fail legality — and a failing apply must never be measured (review P1).
    let apply_search_stats = {
        let mut w = game.clone();
        w.record_events = false;
        w.apply(action.clone())
            .expect("legal search apply (warmup)");
        let mut batch_us = Vec::with_capacity(BATCHES);
        for _ in 0..BATCHES {
            let mut search_copies: Vec<Game> = (0..repeats).map(|_| game.clone()).collect();
            let start = Instant::now();
            for copy in &mut search_copies {
                copy.record_events = false;
                copy.apply(action.clone()).expect("legal search apply");
            }
            batch_us.push(start.elapsed().as_secs_f64() * 1_000_000.0 / repeats as f64);
        }
        stats_from_batch_us(&batch_us)
    };

    // Apply on the committed path: full event logging plus before/after state
    // hashing, i.e. what a recorded human or AI move pays. Fresh copies per
    // iteration for the same legality reason.
    let apply_committed_stats = {
        let mut w = game.clone();
        w.apply(action.clone())
            .expect("legal committed apply (warmup)");
        let mut batch_us = Vec::with_capacity(BATCHES);
        for _ in 0..BATCHES {
            let mut committed_copies: Vec<Game> = (0..repeats).map(|_| game.clone()).collect();
            let start = Instant::now();
            for copy in &mut committed_copies {
                copy.apply(action.clone()).expect("legal committed apply");
            }
            batch_us.push(start.elapsed().as_secs_f64() * 1_000_000.0 / repeats as f64);
        }
        stats_from_batch_us(&batch_us)
    };

    let save_json_bytes = game.save_json().expect("serializable game").len();

    // sample_game timing. Every sample must succeed: a None return would
    // otherwise short-circuit the loop and look like a speedup.
    let mut sample_rng = ChaCha8Rng::seed_from_u64(0x23E0_BA5E_0000_0001);
    let sample_game_stats = batched_stats(sample_repeats, || {
        let sampled = mcts::sample_game(&obs, &mut sample_rng)
            .expect("sample_game must succeed for the baseline position");
        black_box(&sampled);
    });
    // Reset RNG to the same seed so allocation measurement uses the exact same
    // game sequences as the timing measurement.
    let mut sample_rng = ChaCha8Rng::seed_from_u64(0x23E0_BA5E_0000_0001);
    let (sample_game_allocs, sample_game_bytes) = allocations_per_call(sample_repeats, || {
        let sampled = mcts::sample_game(&obs, &mut sample_rng)
            .expect("sample_game must succeed for the baseline position");
        black_box(&sampled);
    });

    // Linear `CardSet::definition` lookup: mean over every definition id and
    // the worst case (the last definition in the vector). The lookup result
    // is consumed through black_box so the linear search cannot be elided.
    let ids: Vec<String> = game
        .card_set
        .definitions
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let (definition_lookup_total_us, definition_lookup_worst_us) = if ids.is_empty() {
        (None, None)
    } else {
        let total = batched_stats(repeats, || {
            for id in &ids {
                let def = game.card_set.definition(id).expect("definition must exist");
                black_box(def);
            }
        });
        let worst = batched_stats(repeats, || {
            let def = game
                .card_set
                .definition(ids.last().expect("nonempty"))
                .expect("definition must exist");
            black_box(def);
        });
        (
            Some(total.median_us / ids.len() as f64),
            Some(worst.median_us),
        )
    };

    // Allocation counters are only valid in the instrumented build.
    let (clone_allocs, clone_bytes) = allocations_per_call(repeats, || {
        let copy = game.clone();
        black_box(&copy);
    });
    let (observation_allocs, observation_bytes) = allocations_per_call(repeats, || {
        let o = game.observation(active).expect("observation");
        black_box(&o);
    });

    json!({
        "name": name,
        "players": players,
        "round": round,
        "phase": if round <= 1 { "early" } else if round <= 8 { "mid" } else { "late" },
        "additional_action_space_enabled": game.config.additional_action_space_enabled,
        "uses_cards": use_cards,
        "card_definitions": game.card_set.definitions.len(),
        "legal_spaces": game.legal_spaces().len(),
        "position_identity": advance_info,
        "clone": stats_json(&clone_stats),
        "clone_allocations": clone_allocs,
        "clone_allocated_bytes": clone_bytes,
        "state_hash": stats_json(&state_hash_stats),
        "cardset_validate": stats_json(&cardset_validate_stats),
        "cardstate_validate": stats_json(&cardstate_validate_stats),
        "definition_lookup_mean_us": definition_lookup_total_us,
        "definition_lookup_worst_us": definition_lookup_worst_us,
        "observation": stats_json(&observation_stats),
        "observation_allocations": observation_allocs,
        "observation_allocated_bytes": observation_bytes,
        "validate_action": stats_json(&validate_action_stats),
        "apply_search": stats_json(&apply_search_stats),
        "apply_committed": stats_json(&apply_committed_stats),
        "save_json_bytes": save_json_bytes,
        "sample_game": stats_json(&sample_game_stats),
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
    let completed_per_sec = if elapsed_s > 0.0 {
        outcome.simulations_completed as f64 / elapsed_s
    } else {
        0.0
    };
    let attempted_per_sec = if elapsed_s > 0.0 {
        simulations as f64 / elapsed_s
    } else {
        0.0
    };
    // Review P1: an illegal choice must fail the harness, not be recorded.
    assert!(
        legal.contains(&choice),
        "search produced an illegal choice {choice:?} for {name}"
    );
    json!({
        "name": name,
        "policy": if puct { "puct" } else { "mcts" },
        "simulations_attempted": simulations,
        "simulations_completed": outcome.simulations_completed,
        "attempted_per_sec": attempted_per_sec,
        "completed_per_sec": completed_per_sec,
        "elapsed_ms": elapsed.as_millis(),
        "choice": format!("{choice:?}"),
        "choice_is_legal": true,
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
    let elapsed = started.elapsed();
    let elapsed_s = elapsed.as_secs_f64();
    let completed_per_sec = if elapsed_s > 0.0 {
        outcome.simulations_completed as f64 / elapsed_s
    } else {
        0.0
    };
    // Review P1: an illegal choice must fail the harness, not be recorded.
    assert!(
        legal.contains(&choice),
        "search produced an illegal choice {choice:?} for {name}"
    );
    json!({
        "name": name,
        "policy": if puct { "puct" } else { "mcts" },
        "budget_ms": budget_ms,
        "elapsed_ms": elapsed.as_millis(),
        "overshoot_ms": outcome.overshoot_ms,
        "simulations_completed": outcome.simulations_completed,
        "completed_per_sec": completed_per_sec,
        "timed_out": outcome.timed_out,
        "used_fallback": outcome.used_fallback,
        "choice_is_legal": true,
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
    let args: Vec<String> = std::env::args().collect();
    let quick = args.iter().any(|a| a == "--quick");
    let alloc_only = args.iter().any(|a| a == "--alloc-only");

    // Fixed scenarios: cardless 2/3/4p × additional space OFF/ON, plus the
    // implemented two-player card pool OFF/ON (review P2: the full feasible
    // condition matrix; 3/4-player card pools are not implemented yet).
    let cardless_2p_off = Game::new(cardless_config(2, false));
    let cardless_2p_on = Game::new(cardless_config(2, true));
    let cardless_3p_off = Game::new(cardless_config(3, false));
    let cardless_3p_on = Game::new(cardless_config(3, true));
    let cardless_4p_off = Game::new(cardless_config(4, false));
    let cardless_4p_on = Game::new(cardless_config(4, true));
    let cards_2p_off = {
        let set = verified_basic_card_set(2).expect("implemented two-player pool");
        let game =
            Game::with_cards(cardless_config(2, false), set.clone()).expect("card game setup");
        // `with_cards` already validated; only assert the hands exist here.
        assert_eq!(game.cards.hands.len(), 2);
        game
    };
    let cards_2p_on = {
        let set = verified_basic_card_set(2).expect("implemented two-player pool");
        let game =
            Game::with_cards(cardless_config(2, true), set.clone()).expect("card game setup");
        assert_eq!(game.cards.hands.len(), 2);
        game
    };

    // Phase positions: early/mid/late per condition (review P2).
    let mut scenario_entries: Vec<(String, Game, bool, Value)> = Vec::new();
    for (name, game, use_cards) in [
        ("cardless_2p_off", &cardless_2p_off, false),
        ("cardless_2p_on", &cardless_2p_on, false),
        ("cardless_3p_off", &cardless_3p_off, false),
        ("cardless_3p_on", &cardless_3p_on, false),
        ("cardless_4p_off", &cardless_4p_off, false),
        ("cardless_4p_on", &cardless_4p_on, false),
        ("cards_2p_off", &cards_2p_off, true),
        ("cards_2p_on", &cards_2p_on, true),
    ] {
        let early = game.clone();
        let (mid, mid_advance) = advance_to_round(game, 8);
        let (late, late_advance) = advance_to_round(game, 14);
        for (suffix, phase_game, advance_info) in [
            (
                "_early",
                early,
                json!({"seed": 0x23E0_0000_0000_0BA5u64, "method": "initial", "moves": 0, "state_hash": game.state_hash()}),
            ),
            ("_mid", mid, mid_advance),
            ("_late", late, late_advance),
        ] {
            let uses = use_cards && phase_game.card_set.enabled();
            scenario_entries.push((format!("{name}{suffix}"), phase_game, uses, advance_info));
        }
    }

    let scenarios: Vec<Value> = scenario_entries
        .iter()
        .map(|(name, game, use_cards, advance_info)| {
            scenario_metrics(name, game, *use_cards, advance_info.clone())
        })
        .collect();

    // Fixed-cap searches: 3 repetitions each for stability (review P2). The
    // completed count stays deterministic per repetition index.
    // Skipped in --alloc-only mode since these are timing measurements.
    let fixed_searches = if !alloc_only {
        let mut out = Vec::new();
        for (name, game, _, _) in &scenario_entries {
            for rep in 0..3 {
                for puct in [false, true] {
                    out.push(fixed_sim_search(
                        &format!("{name}_rep{rep}"),
                        game,
                        puct,
                        32,
                    ));
                }
            }
        }
        out
    } else {
        Vec::new()
    };

    let ten_second = {
        if !quick && !alloc_only {
            let mut out = Vec::new();
            for (name, game, _, _) in &scenario_entries {
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
        "harness_version": 2,
        "purpose": "#237 Step 0 baseline; measure before any v2 optimization",
        "engine_version": agricola_re_ai::model::ENGINE_VERSION,
        "rules_version": agricola_re_ai::model::RULES_VERSION,
        "log_version": agricola_re_ai::model::LOG_VERSION,
        "seed": 0x23E0_0000_0000_0BA5u64,
        "quick_mode": quick,
        "alloc_only": alloc_only,
        "alloc_instrumented": INSTRUMENTED,
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

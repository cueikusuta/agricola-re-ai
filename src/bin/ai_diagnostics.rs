//! AI outcome diagnostics (roadmap M12/M13 preparation, Issue #106).
//!
//! This tool does NOT add or modify a search algorithm. It plays cardless
//! self-play games under a single policy, records the observable per-player
//! outcome metrics, and aggregates which failure modes are *associated* with
//! finishing last instead of first. It measures correlation, not causation;
//! the output is meant to turn "the AI feels weak" into countable hypotheses
//! (food failures, stalled family growth, empty rooms, unused farm cells, ...)
//! that can each become a small, measurable experiment.
//!
//! Design constraints:
//! * Every policy reads only `Observation`; private deck order and the rule
//!   engine RNG are never sampled by the analysis.
//! * The tool **calls the canonical `policy::play_to_end`** for the actual
//!   games; it never re-implements the decision loop, so it cannot drift from
//!   the engine's policy execution. Family-growth timing is derived afterwards
//!   from the recorded log by replaying canonically (`Game::replay`) up to each
//!   growth event, which also re-validates every `before`/`after` state hash.
//! * Output is deterministic: sorted maps, no timestamps, no wall-clock data.
//!
//! Usage:
//!   cargo run --release --bin ai_diagnostics -- \
//!     --players 4 --seeds 200 --seed-start 900000 \
//!     --policy eval --output diagnostics.jsonl [--summary-output summary.json]

use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::{Animal, Crop, Event, Space};
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy};
use agricola_re_ai::score::standings;
use agricola_re_ai::{Config, Game};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyKind {
    Eval,
    Greedy,
    Random,
}

impl PolicyKind {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "eval" => Ok(PolicyKind::Eval),
            "greedy" => Ok(PolicyKind::Greedy),
            "random" => Ok(PolicyKind::Random),
            _ => Err(format!("unsupported policy: {name}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            PolicyKind::Eval => "eval",
            PolicyKind::Greedy => "greedy",
            PolicyKind::Random => "random",
        }
    }
}

struct Options {
    players: usize,
    seed_start: u64,
    seeds: usize,
    policy: PolicyKind,
    additional_action_space: bool,
    output: String,
    summary_output: Option<String>,
}

fn option<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn parse_options() -> Result<Options, String> {
    let allowed = [
        "players",
        "seed-start",
        "seeds",
        "policy",
        "additional-action-space",
        "output",
        "summary-output",
    ];
    let mut values = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let key = arg
            .strip_prefix("--")
            .ok_or_else(|| format!("invalid argument: {arg}"))?;
        if !allowed.contains(&key) {
            return Err(format!("unsupported option: --{key}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if values.insert(key.to_string(), value).is_some() {
            return Err(format!("duplicate option: --{key}"));
        }
    }
    let players = option(&values, "players")?
        .parse::<usize>()
        .map_err(|_| "invalid players".to_string())?;
    let seed_start = option(&values, "seed-start")?
        .parse::<u64>()
        .map_err(|_| "invalid seed-start".to_string())?;
    let seeds = option(&values, "seeds")?
        .parse::<usize>()
        .map_err(|_| "invalid seeds".to_string())?;
    let policy = PolicyKind::parse(option(&values, "policy")?)?;
    let additional_action_space = match option(&values, "additional-action-space")? {
        "true" => true,
        "false" => false,
        _ => return Err("additional-action-space must be true or false".into()),
    };
    let output = option(&values, "output")?.to_string();
    let summary_output = values.get("summary-output").cloned();
    if !(2..=4).contains(&players) {
        return Err("players must be 2, 3, or 4".into());
    }
    if seeds == 0 || seeds > 1_000_000 {
        return Err("seeds must be 1..=1000000".into());
    }
    if seed_start.checked_add(seeds as u64).is_none() {
        return Err("seed range overflows u64".into());
    }
    Ok(Options {
        players,
        seed_start,
        seeds,
        policy,
        additional_action_space,
        output,
        summary_output,
    })
}

/// Snapshot of one player's end-observable outcome plus the failure-mode flags
/// used for aggregation. `grow_round[k]` is the round in which the player's
/// family count first reached `k` (index 3..=5), or `None` if it never did.
#[derive(Clone, Debug)]
struct PlayerMetrics {
    player: usize,
    seat: usize,
    rank: usize,
    score: i32,
    begging: u32,
    family_final: u8,
    grow_third: Option<u8>,
    grow_fourth: Option<u8>,
    grow_fifth: Option<u8>,
    rooms_final: usize,
    empty_rooms: i32,
    fields_final: usize,
    pastures_final: usize,
    stables: usize,
    sheep_final: u32,
    boar_final: u32,
    cattle_final: u32,
    grain_final: u32,
    vegetable_final: u32,
    improvements: usize,
    unused_cells: usize,
}

impl PlayerMetrics {
    /// Failure-mode flags, keyed by a stable name for aggregation and output.
    fn flags(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("begged", self.begging > 0),
            ("never_grew_beyond_two", self.grow_third.is_none()),
            ("late_third_worker", self.grow_third.is_none_or(|r| r > 7)),
            ("no_fields", self.fields_final == 0),
            ("no_pastures", self.pastures_final == 0),
            ("empty_rooms", self.empty_rooms > 0),
            ("farm_left_unused", self.unused_cells >= 5),
            (
                "no_animals",
                self.sheep_final + self.boar_final + self.cattle_final == 0,
            ),
            ("no_vegetables", self.vegetable_final == 0),
        ]
    }
}

fn collect_player_metrics(
    game: &Game,
    rank: &[usize],
    scores: &[i32],
    growth: &[BTreeMap<u8, u8>],
) -> Vec<PlayerMetrics> {
    game.public
        .players
        .iter()
        .enumerate()
        .map(|(id, p)| {
            let unused_cells = p
                .farm
                .iter()
                .filter(|c| matches!(c.tile, agricola_re_ai::model::Tile::Empty) && !c.stable)
                .count();
            let growth_of = |k: u8| growth[id].get(&k).copied();
            PlayerMetrics {
                player: id,
                seat: id,
                rank: rank[id],
                score: scores[id],
                begging: p.begging,
                family_final: p.family,
                grow_third: growth_of(3),
                grow_fourth: growth_of(4),
                grow_fifth: growth_of(5),
                rooms_final: p.rooms(),
                empty_rooms: p.rooms() as i32 - p.family as i32,
                fields_final: p.fields(),
                pastures_final: p.pastures.len(),
                stables: p.occupied_stables(),
                sheep_final: p.animal_count(Animal::Sheep),
                boar_final: p.animal_count(Animal::Boar),
                cattle_final: p.animal_count(Animal::Cattle),
                grain_final: p.total_crop(Crop::Grain),
                vegetable_final: p.total_crop(Crop::Vegetable),
                improvements: p.improvements.len(),
                unused_cells,
            }
        })
        .collect()
}

/// Spaces whose action increases the family count. In cardless play these are
/// the only ways a family can grow, so a growth event in the log is exactly a
/// work action on one of them. (`ModestFamilyGrowth` is the 2-player extra
/// tile; a card that grows the family still uses one of these spaces.)
fn is_family_growth_space(space: Space) -> bool {
    matches!(
        space,
        Space::FamilyGrowthWithRoom | Space::FamilyGrowthNoRoom | Space::ModestFamilyGrowth
    )
}

/// Derive each player's family-growth rounds from the canonical log without
/// re-running any policy decision.
///
/// For every work action that can grow a family, replay the log canonically up
/// to (before) and including (after) that event. `Game::replay` re-checks every
/// `before`/`after` state hash, so this cannot silently use a wrong state. The
/// growth is attributed to the round the action was taken in, read from the
/// pre-action state; this avoids the off-by-one that occurs when the action
/// also ends the round.
fn record_growth_from_log(
    config: &Config,
    events: &[Event],
    growth: &mut [BTreeMap<u8, u8>],
) -> Result<(), String> {
    for index in 0..events.len() {
        let action = match serde_json::from_value::<agricola_re_ai::engine::RecordedAction>(
            events[index].payload.clone(),
        ) {
            Ok(agricola_re_ai::engine::RecordedAction::Work { action, .. }) => action,
            _ => continue,
        };
        if !is_family_growth_space(action.space) {
            continue;
        }
        let before = Game::replay(config.clone(), &events[..index])?;
        let after = Game::replay(config.clone(), &events[..=index])?;
        let round = before.public.round;
        for (id, slots) in growth.iter_mut().enumerate() {
            let was = before.public.players[id].family;
            let now = after.public.players[id].family;
            for threshold in (was + 1)..=now {
                if (3..=5).contains(&threshold) {
                    slots.entry(threshold).or_insert(round);
                }
            }
        }
    }
    Ok(())
}

#[derive(Default, Clone)]
struct FlagAggregate {
    rank1_hits: u64,
    rank1_total: u64,
    other_hits: u64,
    other_total: u64,
}

#[derive(Default, Clone)]
struct SeatAggregate {
    games: u64,
    rank_sum: u64,
    score_sum: i64,
    first_credit: f64,
}

fn run(options: &Options) -> Result<String, String> {
    let file = File::create(&options.output).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);

    let eval = EvaluationPolicy;
    let greedy = GreedyPolicy;
    let random = RandomPolicy;
    let selected: &dyn SpacePolicy = match options.policy {
        PolicyKind::Eval => &eval,
        PolicyKind::Greedy => &greedy,
        PolicyKind::Random => &random,
    };

    let mut flag_names: Vec<&'static str> = Vec::new();
    let mut flags: BTreeMap<&'static str, FlagAggregate> = BTreeMap::new();
    let mut seats: Vec<SeatAggregate> = vec![SeatAggregate::default(); options.players];
    let mut total_begging: u64 = 0;
    let mut score_by_rank_sum: BTreeMap<usize, (i64, u64)> = BTreeMap::new();

    for offset in 0..options.seeds {
        let seed = options.seed_start + offset as u64;
        let config = Config {
            players: options.players,
            seed,
            additional_action_space_enabled: options.additional_action_space,
        };
        let mut game = Game::new(config.clone());
        play_to_end(&mut game, selected, &SimpleDetails)
            .map_err(|e| format!("seed {seed}: {e}"))?;
        let mut growth = vec![BTreeMap::<u8, u8>::new(); options.players];
        record_growth_from_log(&config, &game.log.events, &mut growth)
            .map_err(|e| format!("seed {seed}: growth log replay failed: {e}"))?;
        if !game.public.finished {
            return Err(format!("seed {seed}: game did not finish"));
        }
        game.validate().map_err(|e| format!("seed {seed}: {e}"))?;
        let summary = standings(&game.public.players);
        let scores: Vec<i32> = summary.scores.iter().map(|s| s.total).collect();
        let metrics = collect_player_metrics(&game, &summary.rank, &scores, &growth);
        let first_place_count = summary.rank.iter().filter(|&&r| r == 1).count();

        for m in &metrics {
            if flag_names.is_empty() {
                flag_names = m.flags().into_iter().map(|(name, _)| name).collect();
            }
            let rank1 = m.rank == 1;
            for (name, hit) in m.flags() {
                let entry = flags.entry(name).or_default();
                if rank1 {
                    entry.rank1_total += 1;
                    entry.rank1_hits += u64::from(hit);
                } else {
                    entry.other_total += 1;
                    entry.other_hits += u64::from(hit);
                }
            }
            let seat = &mut seats[m.seat];
            seat.games += 1;
            seat.rank_sum += m.rank as u64;
            seat.score_sum += m.score as i64;
            if rank1 {
                seat.first_credit += 1.0 / first_place_count as f64;
            }
            total_begging += u64::from(m.begging);
            let bucket = score_by_rank_sum.entry(m.rank).or_insert((0, 0));
            bucket.0 += i64::from(m.score);
            bucket.1 += 1;
        }

        let row = json!({
            "seed": seed,
            "players": options.players,
            "additional_action_space_enabled": options.additional_action_space,
            "policy": options.policy.name(),
            "state_hash": game.state_hash(),
            "ranks": summary.rank,
            "scores": scores,
            "metrics": metrics.iter().map(metrics_json).collect::<Vec<_>>(),
        });
        writeln!(writer, "{row}").map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())?;

    let flag_summary: Vec<_> = flag_names
        .iter()
        .map(|name| {
            let agg = flags.get(name).cloned().unwrap_or_default();
            let rank1_rate = rate(agg.rank1_hits, agg.rank1_total);
            let other_rate = rate(agg.other_hits, agg.other_total);
            json!({
                "flag": name,
                "rank1_rate": rank1_rate,
                "other_rate": other_rate,
                "rate_gap": rank1_rate - other_rate,
                "rank1_hits": agg.rank1_hits,
                "rank1_total": agg.rank1_total,
                "other_hits": agg.other_hits,
                "other_total": agg.other_total,
            })
        })
        .collect();

    let seat_summary: Vec<_> = seats
        .iter()
        .enumerate()
        .map(|(seat, agg)| {
            json!({
                "seat": seat,
                "games": agg.games,
                "mean_rank": if agg.games == 0 { 0.0 } else { agg.rank_sum as f64 / agg.games as f64 },
                "mean_score": if agg.games == 0 { 0.0 } else { agg.score_sum as f64 / agg.games as f64 },
                "first_credit_rate": if agg.games == 0 { 0.0 } else { agg.first_credit / agg.games as f64 },
            })
        })
        .collect();

    let score_by_rank: Vec<_> = score_by_rank_sum
        .iter()
        .map(|(rank, (sum, count))| {
            json!({
                "rank": rank,
                "games": count,
                "mean_score": if *count == 0 { 0.0 } else { *sum as f64 / *count as f64 },
            })
        })
        .collect();

    let summary = json!({
        "engine_version": agricola_re_ai::model::ENGINE_VERSION,
        "rules_version": agricola_re_ai::model::RULES_VERSION,
        "players": options.players,
        "policy": options.policy.name(),
        "seeds": options.seeds,
        "seed_start": options.seed_start,
        "additional_action_space_enabled": options.additional_action_space,
        "total_begging_cards": total_begging,
        "flag_summary": flag_summary,
        "seat_summary": seat_summary,
        "score_by_rank": score_by_rank,
    });
    let text = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    if let Some(path) = &options.summary_output {
        std::fs::write(path, &text).map_err(|e| e.to_string())?;
    }
    Ok(text)
}

fn rate(hits: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64
    }
}

fn metrics_json(m: &PlayerMetrics) -> serde_json::Value {
    json!({
        "player": m.player,
        "seat": m.seat,
        "rank": m.rank,
        "score": m.score,
        "begging": m.begging,
        "family_final": m.family_final,
        "grow_third_round": m.grow_third,
        "grow_fourth_round": m.grow_fourth,
        "grow_fifth_round": m.grow_fifth,
        "rooms_final": m.rooms_final,
        "empty_rooms": m.empty_rooms,
        "fields_final": m.fields_final,
        "pastures_final": m.pastures_final,
        "stables": m.stables,
        "sheep_final": m.sheep_final,
        "boar_final": m.boar_final,
        "cattle_final": m.cattle_final,
        "grain_final": m.grain_final,
        "vegetable_final": m.vegetable_final,
        "improvements": m.improvements,
        "unused_cells": m.unused_cells,
    })
}

fn main() {
    match parse_options().and_then(|options| run(&options)) {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("ai_diagnostics: {error}");
            std::process::exit(2);
        }
    }
}

use agricola_re_ai::benchmark::{compare_policies, compare_policies_with_cards};
use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::evaluation::{DetailAwareEvaluationPolicy, EvaluationPolicy, WeightedEvaluationPolicy};
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::policy::{play_to_end, CardChoiceDetails, DetailPolicy, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy};
use agricola_re_ai::score::standings;
use agricola_re_ai::web::serve;
use agricola_re_ai::{Config, Game};
use std::collections::BTreeMap;
use std::time::Instant;
fn main() {
    let mut config = Config::default();
    let mut batch = 1usize;
    let mut greedy = false;
    let mut evaluated = false;
    let mut web = false;
    let mut player_option_supplied = false;
    let mut mcts_budget: Option<usize> = None;
    let mut benchmark: Option<usize> = None;
    let mut benchmark_cards: Option<usize> = None;
    let mut log_file: Option<String> = None;
    let mut state_file: Option<String> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--seed" => {
                config.seed = it
                    .next()
                    .expect("--seed requires integer")
                    .parse()
                    .expect("invalid seed")
            }
            "--batch" => {
                batch = it
                    .next()
                    .expect("--batch requires integer")
                    .parse()
                    .expect("invalid batch")
            }
            "--additional-action-space" => config.additional_action_space_enabled = true,
            "--greedy" => greedy = true,
            "--eval" => evaluated = true,
            "--web" => web = true,
            "--players" => {
                player_option_supplied = true;
                config.players = it
                    .next()
                    .expect("--players requires 2, 3 or 4")
                    .parse()
                    .expect("invalid player count");
            }
            "--mcts" => mcts_budget = Some(48),
            "--mcts-simulations" => {
                mcts_budget = Some(
                    it.next()
                        .expect("--mcts-simulations requires N")
                        .parse()
                        .expect("invalid MCTS simulation count"),
                );
            }
            "--benchmark" => {
                benchmark = Some(
                    it.next()
                        .expect("--benchmark requires seed count")
                        .parse()
                        .expect("invalid benchmark seed count"),
                )
            }
            "--benchmark-cards" => {
                benchmark_cards = Some(
                    it.next()
                        .expect("--benchmark-cards requires seed count")
                        .parse()
                        .expect("invalid benchmark seed count"),
                )
            }
            "--log-file" => log_file = Some(it.next().expect("--log-file requires path").clone()),
            "--state-file" => {
                state_file = Some(it.next().expect("--state-file requires path").clone())
            }
            "--help" | "-h" => {
                println!("Usage: agricola-re-ai [--seed N] [--players 2|3|4] [--web] [--batch N] [--additional-action-space] [--greedy | --eval | --mcts | --mcts-simulations N] [--benchmark N | --benchmark-cards N] [--log-file FILE] [--state-file FILE]");
                return;
            }
            x => {
                eprintln!("Unknown argument {x}");
                std::process::exit(2)
            }
        }
    }
    if !(2..=4).contains(&config.players) {
        eprintln!("--players requires 2, 3 or 4");
        std::process::exit(2);
    }
    if web {
        if !player_option_supplied {
            config.players = 4;
        }
        if let Err(error) = serve(config) {
            eprintln!("web UI error: {error}");
            std::process::exit(1);
        }
        return;
    }
    if batch != 1 && (log_file.is_some() || state_file.is_some()) {
        eprintln!("--log-file and --state-file require --batch 1");
        std::process::exit(2)
    }
    if (greedy as u8 + evaluated as u8 + mcts_budget.is_some() as u8) > 1 {
        eprintln!("choose one of --greedy, --eval, --mcts");
        std::process::exit(2)
    }
    if benchmark.is_some() && benchmark_cards.is_some() {
        eprintln!("choose only one of --benchmark and --benchmark-cards");
        std::process::exit(2);
    }
    if let Some(seeds) = benchmark.or(benchmark_cards) {
        if config.players != 2 {
            eprintln!("--benchmark supports exactly 2 players");
            std::process::exit(2);
        }
        if seeds == 0 || batch != 1 || log_file.is_some() || state_file.is_some() {
            eprintln!("--benchmark requires N > 0 and cannot use --batch or state/log files");
            std::process::exit(2)
        }
        let random = RandomPolicy;
        let greedy_policy = GreedyPolicy;
        let evaluation = EvaluationPolicy;
        let card_set = if benchmark_cards.is_some() {
            Some(verified_basic_card_set(2).expect("verified two-player card set"))
        } else {
            None
        };
        let mcts_benchmark = MctsPolicy {
            simulations: mcts_budget.unwrap_or(8),
            tree_depth: 2,
        };
        let card_details = CardChoiceDetails;
        let card_evaluation = DetailAwareEvaluationPolicy {
            weights: WeightedEvaluationPolicy {
                prior_weight: 0.5,
                state_weight: 2.0,
            },
            details: &card_details,
        };
        let eval_strategy: &dyn SpacePolicy = if card_set.is_some() {
            &card_evaluation
        } else {
            &evaluation
        };
        let mut pairs: Vec<(&str, &dyn SpacePolicy, &str, &dyn SpacePolicy)> = vec![
            ("greedy", &greedy_policy, "random", &random),
            ("eval", eval_strategy, "greedy", &greedy_policy),
            ("eval", eval_strategy, "random", &random),
        ];
        if card_set.is_some() && mcts_budget.is_some() {
            pairs.push(("mcts", &mcts_benchmark, "eval", eval_strategy));
        }
        for (left_name, left, right_name, right) in pairs {
            let result = if let Some(set) = &card_set {
                compare_policies_with_cards(
                    left,
                    right,
                    seeds,
                    config.seed,
                    config.additional_action_space_enabled,
                    set,
                )
            } else {
                compare_policies(
                    left,
                    right,
                    seeds,
                    config.seed,
                    config.additional_action_space_enabled,
                )
            }
            .unwrap_or_else(|err| {
                eprintln!("Benchmark failed: {err}");
                std::process::exit(1)
            });
            println!(
                "{left_name} vs {right_name}: {}",
                serde_json::to_string(&result).expect("benchmark JSON")
            );
        }
        return;
    }
    let mcts_policy = MctsPolicy {
        simulations: mcts_budget.unwrap_or(48),
        ..MctsPolicy::default()
    };
    let mut scores = Vec::new();
    let mut histogram: BTreeMap<i32, usize> = BTreeMap::new();
    let mut duration = 0f64;
    for n in 0..batch {
        let mut game = Game::new(Config {
            players: config.players,
            seed: config.seed + n as u64,
            additional_action_space_enabled: config.additional_action_space_enabled,
        });
        let t = Instant::now();
        let strategy: &dyn SpacePolicy = if mcts_budget.is_some() {
            &mcts_policy
        } else if evaluated {
            &EvaluationPolicy
        } else if greedy {
            &GreedyPolicy
        } else {
            &RandomPolicy
        };
        let details: &dyn DetailPolicy = if mcts_budget.is_some() {
            &CardChoiceDetails
        } else {
            &SimpleDetails
        };
        if let Err(err) = play_to_end(&mut game, strategy, details) {
            eprintln!("Game {} failed: {}", n, err);
            std::process::exit(1)
        }
        duration += t.elapsed().as_secs_f64();
        let result = standings(&game.public.players);
        if batch == 1 {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("score JSON")
            );
            println!("final_state_hash: {}", game.state_hash());
            if let Some(path) = &log_file {
                let json = serde_json::to_string_pretty(&game.log).expect("log JSON");
                std::fs::write(path, json).unwrap_or_else(|err| {
                    eprintln!("log write failed: {err}");
                    std::process::exit(1)
                });
            }
            if let Some(path) = &state_file {
                let json = game.save_json().expect("game state JSON");
                std::fs::write(path, json).unwrap_or_else(|err| {
                    eprintln!("state write failed: {err}");
                    std::process::exit(1)
                });
            }
        }
        for player_score in &result.scores {
            scores.push(player_score.total as f64);
            *histogram.entry(player_score.total).or_default() += 1;
        }
    }
    if batch > 1 {
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let min = scores.iter().copied().fold(f64::INFINITY, f64::min);
        let max = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        println!("games: {batch}, additional_action_space_enabled: {}, mean_game_seconds: {:.6}, score_mean: {:.2}, score_min: {min}, score_max: {max}",config.additional_action_space_enabled,duration/batch as f64,mean);
        println!(
            "score_histogram: {}",
            serde_json::to_string(&histogram).expect("histogram JSON")
        );
    }
}

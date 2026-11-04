use agricola_re_ai::{Config,Game};
use agricola_re_ai::policy::{GreedyPolicy,RandomPolicy,SimpleDetails,SpacePolicy,play_to_end};
use agricola_re_ai::score::standings;
use std::time::Instant;
fn main(){
    let mut config=Config::default();let mut batch=1usize;let mut greedy=false;let mut log_file:Option<String>=None;let mut state_file:Option<String>=None;
    let args:Vec<String>=std::env::args().skip(1).collect();let mut it=args.iter();
    while let Some(arg)=it.next(){match arg.as_str(){
        "--seed"=>{config.seed=it.next().expect("--seed requires integer").parse().expect("invalid seed")},
        "--batch"=>{batch=it.next().expect("--batch requires integer").parse().expect("invalid batch")},
        "--additional-action-space"=>config.additional_action_space_enabled=true,
        "--greedy"=>greedy=true,
        "--log-file"=>log_file=Some(it.next().expect("--log-file requires path").clone()),
        "--state-file"=>state_file=Some(it.next().expect("--state-file requires path").clone()),
        "--help"|"-h"=>{println!("Usage: agricola-re-ai [--seed N] [--batch N] [--additional-action-space] [--greedy] [--log-file FILE] [--state-file FILE]");return},
        x=>{eprintln!("Unknown argument {x}");std::process::exit(2)}
    }}
    if batch!=1&&(log_file.is_some()||state_file.is_some()){eprintln!("--log-file and --state-file require --batch 1");std::process::exit(2)}
    let mut scores=Vec::new();let mut duration=0f64;
    for n in 0..batch {
        let mut game=Game::new(Config{seed:config.seed+n as u64,additional_action_space_enabled:config.additional_action_space_enabled});
        let t=Instant::now();
        let strategy:&dyn SpacePolicy=if greedy{&GreedyPolicy}else{&RandomPolicy};
        if let Err(err)=play_to_end(&mut game,strategy,&SimpleDetails){eprintln!("Game {} failed: {}",n,err);std::process::exit(1)}
        duration+=t.elapsed().as_secs_f64();
        let result=standings(&game.public.players);
        if batch==1 {
            println!("{}",serde_json::to_string_pretty(&result).expect("score JSON"));
            println!("final_state_hash: {}",game.state_hash());
            if let Some(path)=&log_file{
                let json=serde_json::to_string_pretty(&game.log).expect("log JSON");
                std::fs::write(path,json).unwrap_or_else(|err|{eprintln!("log write failed: {err}");std::process::exit(1)});
            }
            if let Some(path)=&state_file{
                let json=game.save_json().expect("game state JSON");
                std::fs::write(path,json).unwrap_or_else(|err|{eprintln!("state write failed: {err}");std::process::exit(1)});
            }
        }
        scores.extend(result.scores.iter().map(|s|s.total as f64));
    }
    if batch>1 {
        let mean=scores.iter().sum::<f64>()/scores.len() as f64;
        let min=scores.iter().copied().fold(f64::INFINITY,f64::min);
        let max=scores.iter().copied().fold(f64::NEG_INFINITY,f64::max);
        println!("games: {batch}, additional_action_space_enabled: {}, mean_game_seconds: {:.6}, score_mean: {:.2}, score_min: {min}, score_max: {max}",config.additional_action_space_enabled,duration/batch as f64,mean);
    }
}

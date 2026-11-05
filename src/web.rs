//! Loopback-only, single-user web UI. No external services or accounts.
use crate::breeding::BreedingPlan;
use crate::engine::{Action, Detail, FreeAction};
use crate::evaluation::EvaluationPolicy;
use crate::harvest::FeedingPlan;
use crate::mcts::MctsPolicy;
use crate::model::{Config, Game, Space};
use crate::policy::{DetailPolicy, SimpleDetails, SpacePolicy};
use crate::score::standings;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

const INDEX: &str = include_str!("../web/index.html");
const MAX_BODY: usize = 65_536;

#[derive(Clone)]
pub struct Session {
    game: Game,
    mcts: bool,
    budget: usize,
    pending_harvest: Option<Action>,
}
impl Session {
    pub fn new(config: Config) -> Result<Self, String> {
        let mut session = Self {
            game: Game::new(config),
            mcts: false,
            budget: 8,
            pending_harvest: None,
        };
        session.advance_cpu()?;
        Ok(session)
    }
    pub fn advance_cpu(&mut self) -> Result<(), String> {
        let details = SimpleDetails;
        for _ in 0..320 {
            if self.pending_harvest.is_some()
                || self.game.public.finished
                || self.game.public.active_player == 0
            {
                return Ok(());
            }
            let obs = self.game.observation(self.game.public.active_player)?;
            let legal = self.game.legal_spaces();
            let event_index = self.game.log.events.len() as u64;
            let mut rng = ChaCha8Rng::seed_from_u64(
                self.game.config.seed ^ event_index.wrapping_mul(0x9e37_79b9_7f4a_7c15),
            );
            let space = if self.mcts {
                MctsPolicy {
                    simulations: self.budget,
                    tree_depth: 2,
                }
                .select(&obs, &legal, &mut rng)
            } else {
                EvaluationPolicy.select(&obs, &legal, &mut rng)
            }
            .ok_or("CPU could not select an action")?;
            let action = details
                .produce(&obs, space)
                .ok_or("CPU could not make action details")?;
            if self.game.will_harvest_after_work() {
                self.pending_harvest = Some(action);
                return Ok(());
            }
            self.game.apply(action)?;
        }
        Err("CPU turn count exceeded".into())
    }
    pub fn view(&self) -> Value {
        let human_turn = !self.game.public.finished
            && self.pending_harvest.is_none()
            && self.game.public.active_player == 0;
        let human_observation = self.game.observation(0).expect("valid human player");
        json!({
            "public": &self.game.public,
            "legal": if human_turn { self.game.legal_spaces() } else { vec![] },
            "standings": standings(&self.game.public.players),
            "hand": human_observation.hand,
            "card_definitions": human_observation.card_definitions,
            "harvest_pending": self.pending_harvest.is_some(),
            "pending_space": self.pending_harvest.as_ref().map(|a| a.space),
            "last_actions": self.game.log.events.iter().rev().take(5).map(|e| json!({
                "player": e.player,
                "kind": &e.kind,
                "space": e.payload.get("Work").and_then(|w| w.get("action"))
                    .and_then(|a| a.get("space")),
            })).collect::<Vec<_>>(),
            "mcts": self.mcts,
            "seed": self.game.config.seed,
        })
    }
    pub fn reset(&mut self, input: &Value) -> Result<(), String> {
        let players = input.get("players").and_then(Value::as_u64).unwrap_or(2);
        if !(2..=4).contains(&players) {
            return Err("players must be 2, 3 or 4".into());
        }
        let budget = input.get("budget").and_then(Value::as_u64).unwrap_or(8);
        if !(1..=256).contains(&budget) {
            return Err("MCTS simulations must be 1..=256".into());
        }
        let config = Config {
            players: players as usize,
            seed: input.get("seed").and_then(Value::as_u64).unwrap_or(42),
            additional_action_space_enabled: input
                .get("extra")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        let mut next = Self {
            game: Game::new(config),
            mcts: input.get("ai").and_then(Value::as_str) == Some("mcts"),
            budget: budget as usize,
            pending_harvest: None,
        };
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }
    pub fn play(&mut self, input: &Value) -> Result<(), String> {
        if self.pending_harvest.is_some()
            || self.game.public.finished
            || self.game.public.active_player != 0
        {
            return Err("not the human player's turn".into());
        }
        // No automatic human details: the browser must submit its own choice.
        let space: Space =
            serde_json::from_value(input.get("space").cloned().ok_or("space is required")?)
                .map_err(|e| e.to_string())?;
        let detail: Detail =
            serde_json::from_value(input.get("detail").cloned().ok_or("detail is required")?)
                .map_err(|e| e.to_string())?;
        if !self.game.legal_spaces().contains(&space) {
            return Err("selected space is not legal".into());
        }
        let action = Action { space, detail };
        let mut next = self.clone();
        if next.game.will_harvest_after_work() {
            next.pending_harvest = Some(action);
        } else {
            next.game.apply(action)?;
            next.advance_cpu()?;
        }
        *self = next;
        Ok(())
    }
    pub fn free(&mut self, input: &Value) -> Result<(), String> {
        if self.game.public.finished {
            return Err("game already finished".into());
        }
        let action: FreeAction =
            serde_json::from_value(input.get("action").cloned().ok_or("action is required")?)
                .map_err(|e| e.to_string())?;
        let mut next = self.clone();
        next.game.apply_free(0, action)?;
        *self = next;
        Ok(())
    }
    pub fn harvest_preview(&self, input: &Value) -> Result<Value, String> {
        let feeding: FeedingPlan = serde_json::from_value(
            input
                .get("feeding")
                .cloned()
                .ok_or("feeding choices required")?,
        )
        .map_err(|e| e.to_string())?;
        let action = self.pending_harvest.as_ref().ok_or("no harvest pending")?;
        let public = self
            .game
            .preview_harvest_after_feeding(action, 0, &feeding)?;
        Ok(json!({"public": public}))
    }
    pub fn harvest(&mut self, input: &Value) -> Result<(), String> {
        let feeding: FeedingPlan = serde_json::from_value(
            input
                .get("feeding")
                .cloned()
                .ok_or("feeding choices required")?,
        )
        .map_err(|e| e.to_string())?;
        let breeding: BreedingPlan = serde_json::from_value(
            input
                .get("breeding")
                .cloned()
                .ok_or("breeding choices required")?,
        )
        .map_err(|e| e.to_string())?;
        let mut next = self.clone();
        let action = next.pending_harvest.take().ok_or("no harvest pending")?;
        next.game
            .apply_with_human_harvest(action, 0, feeding, breeding)?;
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }
}

fn local_request_allowed(host: Option<&str>, origin: Option<&str>) -> bool {
    let local_host = matches!(host, Some("127.0.0.1:8787" | "localhost:8787"));
    let local_origin = origin
        .is_none_or(|value| matches!(value, "http://127.0.0.1:8787" | "http://localhost:8787"));
    local_host && local_origin
}

fn json_content_type(value: Option<&str>) -> bool {
    value
        .and_then(|content_type| content_type.split(';').next())
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
}

fn response(stream: &mut TcpStream, status: &str, mime: &str, content: &str) -> Result<(), String> {
    let bytes = content.as_bytes();
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(bytes).map_err(|e| e.to_string())
}
fn handle(mut stream: TcpStream, session: &mut Session) -> Result<(), String> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| e.to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let mut length = 0usize;
    let mut count = 0usize;
    let mut host: Option<String> = None;
    let mut origin: Option<String> = None;
    let mut content_type: Option<String> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if line.is_empty() || line == "\r\n" || line == "\n" {
            break;
        }
        count += 1;
        if count > 80 {
            return response(
                &mut stream,
                "431 Request Header Fields Too Large",
                "application/json",
                "{\"error\":\"too many headers\"}",
            );
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value
                    .trim()
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            } else if name.eq_ignore_ascii_case("host") {
                host = Some(value.trim().to_ascii_lowercase());
            } else if name.eq_ignore_ascii_case("origin") {
                origin = Some(value.trim().to_ascii_lowercase());
            } else if name.eq_ignore_ascii_case("content-type") {
                content_type = Some(value.trim().to_owned());
            }
        }
    }
    if !local_request_allowed(host.as_deref(), origin.as_deref()) {
        return response(
            &mut stream,
            "403 Forbidden",
            "application/json",
            "{\"error\":\"non-local request rejected\"}",
        );
    }
    if method == "POST" && !json_content_type(content_type.as_deref()) {
        return response(
            &mut stream,
            "415 Unsupported Media Type",
            "application/json",
            "{\"error\":\"application/json required\"}",
        );
    }
    if length > MAX_BODY {
        return response(
            &mut stream,
            "413 Content Too Large",
            "application/json",
            "{\"error\":\"body too large\"}",
        );
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    if method == "GET" && path == "/" {
        return response(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX);
    }
    let result: Result<Value, String> = (|| match (method, path) {
        ("GET", "/api/state") => Ok(session.view()),
        ("POST", "/api/new")
        | ("POST", "/api/play")
        | ("POST", "/api/free")
        | ("POST", "/api/harvest")
        | ("POST", "/api/harvest-preview") => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if path == "/api/harvest-preview" {
                return session.harvest_preview(&value);
            }
            match path {
                "/api/new" => session.reset(&value)?,
                "/api/play" => session.play(&value)?,
                "/api/free" => session.free(&value)?,
                "/api/harvest" => session.harvest(&value)?,
                _ => unreachable!(),
            }
            Ok(session.view())
        }
        _ => Err("endpoint not found".into()),
    })();
    match result {
        Ok(value) => response(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            &value.to_string(),
        ),
        Err(error) => response(
            &mut stream,
            "400 Bad Request",
            "application/json; charset=utf-8",
            &json!({"error": error}).to_string(),
        ),
    }
}
pub fn serve(config: Config) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", 8787)).map_err(|e| e.to_string())?;
    let mut session = Session::new(config)?;
    println!("Agricola local UI: http://127.0.0.1:8787/");
    for accepted in listener.incoming() {
        match accepted {
            Ok(stream) => {
                if let Err(error) = handle(stream, &mut session) {
                    eprintln!("UI request error: {error}");
                }
            }
            Err(error) => eprintln!("UI connection error: {error}"),
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_api_rejects_cross_site_and_simple_form_posts() {
        assert!(local_request_allowed(Some("127.0.0.1:8787"), None));
        assert!(local_request_allowed(
            Some("localhost:8787"),
            Some("http://localhost:8787")
        ));
        assert!(!local_request_allowed(
            Some("evil.example:8787"),
            Some("http://evil.example:8787")
        ));
        assert!(!local_request_allowed(
            Some("127.0.0.1:8787"),
            Some("https://evil.example")
        ));
        assert!(!local_request_allowed(None, None));
        assert!(json_content_type(Some("application/json; charset=utf-8")));
        assert!(!json_content_type(Some("text/plain")));
        assert!(!json_content_type(None));
    }

    #[test]
    fn rejects_invalid_configuration_and_move() {
        let mut session = Session::new(Config::default()).unwrap();
        assert!(session.reset(&json!({"players":5})).is_err());
        assert!(session.play(&json!({"space":"not_a_space"})).is_err());
        session
            .reset(&json!({"players":4,"seed":100,"extra":true}))
            .unwrap();
        assert_eq!(session.game.public.players.len(), 4);
        session.game.validate().unwrap();
    }
    fn finish_pending_harvest(session: &mut Session) {
        if !session.view()["harvest_pending"].as_bool().unwrap() {
            return;
        }
        // Test fixture obtains a legal plan by applying the pending action to
        // a copy; production web actions never call this automatic policy.
        let mut copy = session.game.clone();
        copy.apply(session.pending_harvest.clone().unwrap())
            .unwrap();
        let last = &copy.log.events.last().unwrap().payload;
        let record: crate::engine::RecordedAction = serde_json::from_value(last.clone()).unwrap();
        if let crate::engine::RecordedAction::Work {
            harvest_plans: Some(feeding),
            breeding_plans: Some(breeding),
            ..
        } = record
        {
            session
                .harvest(&json!({"feeding":feeding[0], "breeding":breeding[0]}))
                .unwrap();
        } else {
            panic!("final worker must record harvest decisions")
        }
    }

    #[test]
    fn manual_detail_and_free_actions_do_not_fallback_to_ai() {
        let mut session = Session::new(Config::default()).unwrap();
        let before = session.game.state_hash();
        let space = session.game.legal_spaces()[0];
        assert!(session.play(&json!({"space":space})).is_err());
        assert_eq!(session.game.state_hash(), before);
        assert!(session
            .free(&json!({"action":{"ConvertCrop":{"good":"grain","quantity":0}}}))
            .is_err());
        assert_eq!(session.game.state_hash(), before);
    }

    #[test]
    fn human_and_cpu_can_take_turns() {
        let mut session = Session::new(Config {
            seed: 5,
            ..Config::default()
        })
        .unwrap();
        for _ in 0..3 {
            if session.game.public.finished {
                break;
            }
            assert_eq!(session.game.public.active_player, 0);
            let space = session.game.legal_spaces()[0];
            let obs = session.game.observation(0).unwrap();
            let action = SimpleDetails.produce(&obs, space).unwrap();
            session
                .play(&json!({"space":space,"detail":action.detail}))
                .unwrap();
            finish_pending_harvest(&mut session);
            session.game.validate().unwrap();
        }
    }
    #[test]
    fn complete_four_player_human_vs_cpu_match() {
        let mut session = Session::new(Config {
            seed: 103,
            players: 4,
            additional_action_space_enabled: true,
        })
        .unwrap();
        for turn in 0..120 {
            if session.game.public.finished {
                break;
            }
            assert_eq!(session.game.public.active_player, 0, "turn {turn}");
            let obs = session.game.observation(0).unwrap();
            let legal = session.game.legal_spaces();
            let mut rng = ChaCha8Rng::seed_from_u64(turn);
            let space = EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap();
            let action = SimpleDetails.produce(&obs, space).unwrap();
            session
                .play(&json!({"space":space,"detail":action.detail}))
                .unwrap();
            finish_pending_harvest(&mut session);
        }
        assert!(session.game.public.finished);
        let replay = Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }
}

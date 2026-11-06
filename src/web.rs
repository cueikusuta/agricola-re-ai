//! Loopback-only, single-user web UI. No external services or accounts.
use crate::breeding::BreedingPlan;
use crate::cards::CardSet;
use crate::engine::{Action, Detail, FreeAction};
use crate::harvest::{FeedingPlan, HarvestDetailPolicy};

use crate::evaluation::{DetailAwareEvaluationPolicy, WeightedEvaluationPolicy};
use crate::mcts::MctsPolicy;
use crate::model::{Config, Game, Observation, Space};
use crate::policy::{CardChoiceDetails, DetailPolicy, SimpleDetails, SpacePolicy};
use crate::score::standings_with_cards;
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
    pending: Option<PendingHarvest>,
}

#[derive(Clone)]
enum PendingHarvest {
    Feeding(Action),
    Breeding(Action, Vec<FeedingPlan>),
}

struct ManualHarvest {
    feeding: Vec<FeedingPlan>,
    breeding: Vec<BreedingPlan>,
}
impl HarvestDetailPolicy for ManualHarvest {
    fn plan_feeding(&self, obs: &Observation) -> FeedingPlan {
        self.feeding[obs.player_id].clone()
    }
    fn plan_breeding(&self, obs: &Observation) -> BreedingPlan {
        self.breeding[obs.player_id].clone()
    }
}
impl Session {
    pub fn new(config: Config) -> Result<Self, String> {
        let mut session = Self {
            game: Game::new(config),
            mcts: false,
            budget: 8,
            pending: None,
        };
        session.advance_cpu()?;
        Ok(session)
    }
    fn queue_or_apply(&mut self, action: Action) -> Result<(), String> {
        if self.game.will_harvest_after_work() {
            self.game.preview_harvest(&action, None)?;
            self.pending = Some(PendingHarvest::Feeding(action));
        } else {
            self.game.apply(action)?;
        }
        Ok(())
    }

    pub fn advance_cpu(&mut self) -> Result<(), String> {
        let details: &dyn DetailPolicy = if self.game.card_set.enabled() {
            &CardChoiceDetails
        } else {
            &SimpleDetails
        };
        let evaluation = DetailAwareEvaluationPolicy {
            weights: WeightedEvaluationPolicy {
                prior_weight: 0.5,
                state_weight: 2.0,
            },
            details,
        };
        for _ in 0..320 {
            if self.pending.is_some()
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
                evaluation.select(&obs, &legal, &mut rng)
            }
            .ok_or("CPU could not select an action")?;
            let action = details
                .produce(&obs, space)
                .ok_or("CPU could not make action details")?;
            self.queue_or_apply(action)?;
        }
        Err("CPU turn count exceeded".into())
    }
    pub fn view(&self) -> Value {
        let human_turn = self.pending.is_none()
            && !self.game.public.finished
            && self.game.public.active_player == 0;
        let harvest = match &self.pending {
            None => Value::Null,
            Some(PendingHarvest::Feeding(action)) => json!({
                "phase": "feeding",
                "preview": self.game.preview_harvest(action, None).expect("validated action"),
            }),
            Some(PendingHarvest::Breeding(action, feeding)) => json!({
                "phase": "breeding",
                "preview": self.game.preview_harvest(action, Some(feeding)).expect("validated plans"),
            }),
        };
        let detail_hints = if human_turn {
            let mut map = serde_json::Map::new();
            for space in self.game.available_spaces() {
                if let Ok(Value::String(key)) = serde_json::to_value(space) {
                    if let Ok(value) = serde_json::to_value(self.game.detail_hints(space)) {
                        map.insert(key, value);
                    }
                }
            }
            Value::Object(map)
        } else {
            Value::Object(serde_json::Map::new())
        };
        let human_player = &self.game.public.players[0];
        let free_action_hints = if human_turn {
            serde_json::to_value(crate::hints::free_action_hints(human_player))
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let fence_hints = if human_turn {
            serde_json::to_value(self.game.fence_hints()).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let feeding_hints = match &self.pending {
            Some(PendingHarvest::Feeding(action)) => self
                .game
                .preview_harvest(action, None)
                .ok()
                .map(|preview| {
                    serde_json::to_value(crate::hints::feeding_hints(&preview.players[0]))
                        .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        let breeding_hints = match &self.pending {
            Some(PendingHarvest::Breeding(action, feeding)) => self
                .game
                .preview_harvest(action, Some(feeding))
                .ok()
                .map(|preview| {
                    serde_json::to_value(crate::hints::breeding_hints(&preview.players[0]))
                        .unwrap_or(Value::Null)
                })
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        json!({
            "harvest": harvest,
            "hand": &self.game.cards.hands[0],
            "card_definitions": &self.game.card_set.definitions,
            "card_enabled": self.game.card_set.enabled(),
            "public": &self.game.public,
            "legal": if human_turn { self.game.legal_spaces() } else { vec![] },
            "available": if human_turn { self.game.available_spaces() } else { vec![] },
            "detail_hints": detail_hints,
            "free_action_hints": free_action_hints,
            "fence_hints": fence_hints,
            "feeding_hints": feeding_hints,
            "breeding_hints": breeding_hints,
            "standings": standings_with_cards(&self.game.public.players, &self.game.card_set),
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
        let card_set: CardSet = match input.get("preset").and_then(Value::as_str) {
            Some("verified") => crate::cards::verified_basic_card_set(config.players)?,
            Some(_) => return Err("unknown card preset".into()),
            None => match input.get("card_set") {
                Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                None => CardSet::default(),
            },
        };
        let mut next = Self {
            game: Game::with_cards(config, card_set)?,
            mcts: input.get("ai").and_then(Value::as_str) == Some("mcts"),
            budget: budget as usize,
            pending: None,
        };
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }
    pub fn play(&mut self, input: &Value) -> Result<(), String> {
        if self.pending.is_some()
            || self.game.public.finished
            || self.game.public.active_player != 0
        {
            return Err("not the human player's turn".into());
        }
        let space: Space =
            serde_json::from_value(input.get("space").cloned().ok_or("space is required")?)
                .map_err(|e| e.to_string())?;
        // Human input is always explicit. SimpleDetails remains CPU-only.
        let detail: Detail = serde_json::from_value(
            input
                .get("detail")
                .cloned()
                .ok_or("manual detail is required")?,
        )
        .map_err(|e| format!("invalid detail: {e}"))?;
        // Human and CPU share the engine's transactional validator. The
        // candidate-narrowed `legal_spaces` listing never defines legality.
        let action = Action { space, detail };
        self.game.validate_action(&action)?;
        let mut next = self.clone();
        next.queue_or_apply(action)?;
        next.advance_cpu()?;
        *self = next;
        Ok(())
    }

    pub fn free(&mut self, input: &Value) -> Result<(), String> {
        if self.game.public.finished || self.pending.is_some() {
            return Err("free action not available during harvest/final results".into());
        }
        let action: FreeAction = serde_json::from_value(
            input
                .get("action")
                .cloned()
                .ok_or("free action is required")?,
        )
        .map_err(|e| e.to_string())?;
        let mut next = self.clone();
        next.game.apply_free(0, action)?;
        *self = next;
        Ok(())
    }

    /// A human approves food/workshop choices first, then sees the exact
    /// post-feeding state before choosing animal births. CPU uses its default.
    pub fn harvest(&mut self, input: &Value) -> Result<(), String> {
        let mut next = self.clone();
        let pending = next.pending.clone().ok_or("harvest is not pending")?;
        match pending {
            PendingHarvest::Feeding(action) => {
                let plan: FeedingPlan = serde_json::from_value(
                    input.get("feeding").cloned().ok_or("feeding is required")?,
                )
                .map_err(|e| e.to_string())?;
                plan.validate()?;
                let mut plans = vec![FeedingPlan::default(); next.game.config.players];
                plans[0] = plan;
                next.game.preview_harvest(&action, Some(&plans))?;
                next.pending = Some(PendingHarvest::Breeding(action, plans));
            }
            PendingHarvest::Breeding(action, feeding) => {
                let choice: BreedingPlan = serde_json::from_value(
                    input
                        .get("breeding")
                        .cloned()
                        .ok_or("breeding is required")?,
                )
                .map_err(|e| e.to_string())?;
                let preview = next.game.preview_harvest(&action, Some(&feeding))?;
                choice.apply(&mut preview.players[0].clone())?;
                let breeding = preview
                    .players
                    .iter()
                    .enumerate()
                    .map(|(i, player)| {
                        if i == 0 {
                            choice.clone()
                        } else {
                            BreedingPlan::simple(player)
                        }
                    })
                    .collect();
                let policy = ManualHarvest { feeding, breeding };
                next.game.apply_with_harvest_policy(action, &policy)?;
                next.pending = None;
                next.advance_cpu()?;
            }
        }
        *self = next;
        Ok(())
    }
}

fn local_request_allowed(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let local_host = host.is_some_and(|value| {
        value == format!("127.0.0.1:{port}") || value == format!("localhost:{port}")
    });
    let local_origin = origin.is_none_or(|value| {
        value == format!("http://127.0.0.1:{port}") || value == format!("http://localhost:{port}")
    });
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
fn handle(mut stream: TcpStream, session: &mut Session, port: u16) -> Result<(), String> {
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
    if !local_request_allowed(host.as_deref(), origin.as_deref(), port) {
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
        | ("POST", "/api/harvest") => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if path == "/api/new" {
                session.reset(&value)?;
            } else if path == "/api/play" {
                session.play(&value)?;
            } else if path == "/api/free" {
                session.free(&value)?;
            } else {
                session.harvest(&value)?;
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
/// Start the local UI on the port from `AGRICOLA_WEB_PORT`, or 8787.
pub fn serve(config: Config) -> Result<(), String> {
    let port = std::env::var("AGRICOLA_WEB_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8787);
    serve_on(config, port)
}

/// Bind the loopback-only UI on `port` and serve until the process stops.
///
/// Port `0` asks the OS for a free port and prints the bound URL. Only
/// loopback `Host`/`Origin` values for the bound port are accepted, so the
/// boundary stays single-user and local regardless of the chosen port.
pub fn serve_on(config: Config, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("failed to bind 127.0.0.1:{port}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let mut session = Session::new(config)?;
    println!("Agricola local UI: http://127.0.0.1:{bound}/");
    for accepted in listener.incoming() {
        match accepted {
            Ok(stream) => {
                if let Err(error) = handle(stream, &mut session, bound) {
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
    use crate::evaluation::EvaluationPolicy;

    #[test]
    fn local_api_rejects_cross_site_and_simple_form_posts() {
        assert!(local_request_allowed(Some("127.0.0.1:8787"), None, 8787));
        assert!(local_request_allowed(
            Some("localhost:8787"),
            Some("http://localhost:8787"),
            8787
        ));
        assert!(!local_request_allowed(
            Some("evil.example:8787"),
            Some("http://evil.example:8787"),
            8787
        ));
        assert!(!local_request_allowed(
            Some("127.0.0.1:8787"),
            Some("https://evil.example"),
            8787
        ));
        assert!(!local_request_allowed(None, None, 8787));
        // A different bound port must not accept the default port's host.
        assert!(!local_request_allowed(Some("127.0.0.1:8787"), None, 8790));
        assert!(local_request_allowed(Some("127.0.0.1:8790"), None, 8790));
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
    fn play_simple(session: &mut Session, space: Space) {
        let detail = SimpleDetails
            .produce(&session.game.observation(0).unwrap(), space)
            .expect("test policy detail")
            .detail;
        session
            .play(&json!({"space": space, "detail": detail}))
            .unwrap();
        while let Some(pending) = session.pending.clone() {
            match pending {
                PendingHarvest::Feeding(_) => session
                    .harvest(&json!({"feeding": FeedingPlan::default()}))
                    .unwrap(),
                PendingHarvest::Breeding(action, feeding) => {
                    let preview = session
                        .game
                        .preview_harvest(&action, Some(&feeding))
                        .unwrap();
                    let choice = BreedingPlan::simple(&preview.players[0]);
                    session.harvest(&json!({"breeding": choice})).unwrap();
                }
            }
        }
    }

    #[test]
    fn manual_details_required_and_preview_does_not_commit() {
        let mut session = Session::new(Config::default()).unwrap();
        if session.game.public.active_player != 0 {
            session.advance_cpu().unwrap();
        }
        let legal = session.game.legal_spaces();
        let first = legal[0];
        let hash = session.game.state_hash();
        assert!(session.play(&json!({"space": first})).is_err());
        assert_eq!(session.game.state_hash(), hash);
    }

    #[test]
    fn human_path_accepts_a_legal_non_default_detail_and_rejects_illegal() {
        let seed = (0..2_000u64)
            .find(|&seed| {
                let probe = Game::new(Config {
                    players: 2,
                    seed,
                    additional_action_space_enabled: false,
                });
                probe.public.start_player == 0
                    && probe.public.revealed.first() == Some(&crate::model::RoundCard::Fencing)
            })
            .expect("seed with human start and Fencing revealed");
        let mut session = Session::new(Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        })
        .unwrap();
        assert_eq!(session.game.public.active_player, 0);
        session.game.public.players[0].supply.wood = 10;

        let hash = session.game.state_hash();
        // An illegal detail is rejected by the shared validator without
        // changing state; the web endpoint no longer filters by candidate set.
        assert!(session
            .play(&json!({"space":"fencing","detail":{"Fence":{"pastures":[[0,1]]}}}))
            .is_err());
        assert_eq!(session.game.state_hash(), hash);
        session
            .play(&json!({"space":"fencing","detail":{"Fence":{"pastures":[[1,2]]}}}))
            .expect("legal non-default detail accepted through the human endpoint");
        assert!(session.game.public.players[0]
            .pastures
            .iter()
            .any(|pasture| pasture.cells == vec![1, 2]));
    }

    #[test]
    fn manual_free_action_is_logged_and_replays() {
        let mut session = Session::new(Config {
            seed: 34,
            ..Config::default()
        })
        .unwrap();
        let before = session.game.public.players[0].supply.food;
        session
            .free(&json!({"action": {"DiscardGood": {"good": "food", "quantity": 1}}}))
            .unwrap();
        assert_eq!(session.game.public.players[0].supply.food, before - 1);
        let replay = Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }

    #[test]
    fn harvest_decisions_require_human_and_replay() {
        let mut session = Session::new(Config {
            seed: 123,
            ..Config::default()
        })
        .unwrap();
        for _ in 0..80 {
            if let Some(PendingHarvest::Feeding(action)) = session.pending.clone() {
                let before = session.game.state_hash();
                let preview = session.game.preview_harvest(&action, None).unwrap();
                assert_eq!(preview.round, 4);
                let mut plan = FeedingPlan::default();
                plan.priorities.reverse();
                session.harvest(&json!({"feeding": plan})).unwrap();
                assert_eq!(session.game.state_hash(), before);
                let (action, plans) = match session.pending.clone().unwrap() {
                    PendingHarvest::Breeding(action, plans) => (action, plans),
                    _ => panic!("breeding choice must follow feeding choice"),
                };
                let after_feeding = session.game.preview_harvest(&action, Some(&plans)).unwrap();
                let choice = BreedingPlan::simple(&after_feeding.players[0]);
                session.harvest(&json!({"breeding": choice})).unwrap();
                let replay =
                    Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
                assert_eq!(replay.state_hash(), session.game.state_hash());
                return;
            }
            let space = session.game.legal_spaces()[0];
            let detail = SimpleDetails
                .produce(&session.game.observation(0).unwrap(), space)
                .unwrap()
                .detail;
            session
                .play(&json!({"space": space, "detail": detail}))
                .unwrap();
        }
        panic!("harvest was never reached");
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
            play_simple(&mut session, space);
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
            play_simple(&mut session, space);
        }
        assert!(session.game.public.finished);
        let replay = Game::replay(session.game.config.clone(), &session.game.log.events).unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }

    #[test]
    fn complete_two_player_verified_cards_through_manual_requests() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .reset(&json!({"players": 2, "seed": 22, "preset": "verified"}))
            .unwrap();
        assert_eq!(session.view()["card_enabled"], true);
        for turn in 0..80 {
            if session.game.public.finished {
                break;
            }
            assert_eq!(session.game.public.active_player, 0, "turn {turn}");
            let obs = session.game.observation(0).unwrap();
            let legal = session.game.legal_spaces();
            let mut rng = ChaCha8Rng::seed_from_u64(turn);
            let space = if legal.contains(&Space::Lessons) {
                Space::Lessons
            } else {
                EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap()
            };
            play_simple(&mut session, space);
            session.game.validate().unwrap();
        }
        assert!(session.game.public.finished, "manual card game must finish");
        assert_eq!(session.game.public.round, 14);
        assert!(
            session
                .game
                .public
                .players
                .iter()
                .any(|player| !player.played_cards.is_empty()),
            "the card-enabled UI path must actually play a card"
        );
        let replay = Game::replay_with_cards(
            session.game.config.clone(),
            session.game.card_set.clone(),
            &session.game.log.events,
        )
        .unwrap();
        assert_eq!(replay.state_hash(), session.game.state_hash());
    }
}

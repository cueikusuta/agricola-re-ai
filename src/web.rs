//! Loopback-only, single-user web UI. No external services or accounts.
use crate::breeding::BreedingPlan;
use crate::cards::CardSet;
use crate::draft::{
    cpu_choice, cpu_pick, CardKind, Direction, DraftMode, DraftObservation, DraftPhase, DraftPool,
    DraftRuleset, DraftSetup, DraftState,
};
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
    draft: Option<DraftWeb>,
}

/// A server-owned draft session. The authoritative state stays on the server;
/// the browser only ever receives `DraftObservation` for the human seat.
#[derive(Clone)]
struct DraftWeb {
    setup: DraftSetup,
    state: DraftState,
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
            draft: None,
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
            "draft": self
                .draft
                .as_ref()
                .and_then(|d| d.state.observe(0, d.state.current_token()).ok())
                .map(|obs| draft_json(&obs, &self.draft.as_ref().unwrap().state, &self.draft.as_ref().unwrap().setup))
                .unwrap_or(Value::Null),
        })
    }

    // ---- M11 draft foundation (Issue #104) --------------------------------

    fn draft(&self) -> Result<&DraftWeb, String> {
        self.draft.as_ref().ok_or("no draft session".into())
    }

    pub fn draft_view(&self) -> Result<Value, String> {
        let d = self.draft()?;
        let obs = d.state.observe(0, d.state.current_token())?;
        Ok(draft_json(&obs, &d.state, &d.setup))
    }

    /// Start a draft with an explicit ruleset. Missing direction or phase-order
    /// parameters are rejected rather than defaulted, because those are
    /// unconfirmed tournament conventions (see `docs/draft-protocol-m11.md`).
    pub fn start_draft(&mut self, input: &Value) -> Result<(), String> {
        let mode = match input.get("mode").and_then(Value::as_str) {
            Some("deal_7") => DraftMode::Deal7,
            Some("draft_7_7") => DraftMode::Draft77,
            Some("draft_10_7_simultaneous") => DraftMode::Draft10_7Simultaneous,
            Some("choose_10_7") => DraftMode::Choose10_7,
            _ => return Err("unknown draft mode".into()),
        };
        let players = input.get("players").and_then(Value::as_u64).unwrap_or(2) as usize;
        if !(2..=4).contains(&players) {
            return Err("draft players must be 2, 3 or 4".into());
        }
        let direction_occupation = parse_direction(
            input.get("direction_occupation").and_then(Value::as_str),
            "direction_occupation",
        )?;
        let direction_minor = parse_direction(
            input.get("direction_minor").and_then(Value::as_str),
            "direction_minor",
        )?;
        let occupation_phase_first = match input.get("occupation_phase_first") {
            Some(value) => value
                .as_bool()
                .ok_or("occupation_phase_first must be a boolean")?,
            None if mode == DraftMode::Draft77 => {
                return Err("draft_7_7 requires an explicit occupation_phase_first".into())
            }
            None => true,
        };
        let seed = input.get("seed").and_then(Value::as_u64).unwrap_or(42);
        let pool = fixture_pool(mode, players);
        let setup = DraftSetup {
            mode,
            ruleset: DraftRuleset::explicit(
                format!("fixture-explicit-{}", mode.id()),
                direction_occupation,
                direction_minor,
                occupation_phase_first,
            ),
            pool,
            player_count: players,
            seed,
            deck_version: "test-fixture-v1".into(),
        };
        let state = setup.start()?;
        self.draft = Some(DraftWeb { setup, state });
        Ok(())
    }

    /// Human seat 0 commits one round pick; the CPU seats then follow.
    pub fn draft_pick(&mut self, input: &Value) -> Result<(), String> {
        let token = input
            .get("token")
            .and_then(Value::as_u64)
            .ok_or("draft token is required")?;
        let occupation = optional_string(input.get("occupation"))?;
        let minor = optional_string(input.get("minor"))?;
        // Whole request is transactional: the human commit and every CPU
        // follow-up either all apply or none do.
        let mut next = self.draft.clone().ok_or("no draft session")?;
        next.state.commit_pick(0, token, occupation, minor)?;
        auto_commit_cpus(&mut next)?;
        self.draft = Some(next);
        Ok(())
    }

    /// Human seat 0 finalises a 10-7 non-draft hand; the CPU seats follow.
    pub fn draft_choice(&mut self, input: &Value) -> Result<(), String> {
        let token = input
            .get("token")
            .and_then(Value::as_u64)
            .ok_or("draft token is required")?;
        let occupations = string_list(input.get("occupations"), "occupations")?;
        let minors = string_list(input.get("minors"), "minors")?;
        let mut next = self.draft.clone().ok_or("no draft session")?;
        next.state.commit_choice(0, token, occupations, minors)?;
        auto_commit_cpus(&mut next)?;
        self.draft = Some(next);
        Ok(())
    }

    pub fn reset_draft(&mut self) {
        self.draft = None;
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
            // A fresh game resets the draft setup; the draft is a distinct
            // pre-game phase and must not leak into a new match.
            draft: None,
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

/// Advance every CPU seat that has not yet committed, using only each seat's
/// own observation. Never reads another seat's private packet. The caller runs
/// this on a clone so a failure cannot partially mutate the session.
fn auto_commit_cpus(draft: &mut DraftWeb) -> Result<(), String> {
    let players = draft.state.player_count;
    let mode = draft.state.mode;
    for seat in 1..players {
        let pending = draft.state.commitments[seat].is_none() && !draft.state.is_complete();
        if !pending {
            continue;
        }
        let token = draft.state.current_token();
        let obs = draft.state.observe(seat, token)?;
        match mode {
            DraftMode::Choose10_7 => {
                let (occ, minor) = cpu_choice(&obs)?;
                draft.state.commit_choice(seat, token, occ, minor)?;
            }
            _ => {
                let commit = cpu_pick(&obs)?;
                draft
                    .state
                    .commit_pick(seat, token, commit.occupation, commit.minor)?;
            }
        }
    }
    Ok(())
}

/// Build the observable draft payload for the browser. It contains only the
/// human seat's own packets and history plus public counts; the seed, other
/// seats' packets, and the private discard pile are never serialized here.
fn draft_json(obs: &DraftObservation, state: &DraftState, setup: &DraftSetup) -> Value {
    let complete = state.is_complete();
    // Only the viewer's own final hand may be revealed. Other seats' hands are
    // private even after the draft ends.
    let own_final_hands = if complete {
        state
            .final_hands()
            .get(obs.viewer)
            .map(|(occupations, minors)| json!({"occupations": occupations, "minors": minors}))
    } else {
        None
    };
    json!({
        "mode": obs.mode.id(),
        "ruleset_id": obs.ruleset_id,
        "pool_kind": "test_fixture",
        "deck_version": setup.deck_version,
        "phase": match obs.phase {
            DraftPhase::Drafting => "drafting",
            DraftPhase::Choosing => "choosing",
            DraftPhase::Complete => "complete",
        },
        "round": obs.round,
        "active_kind": obs.active_kind.map(CardKind::as_str),
        "hand_counts": obs.hand_counts,
        "own_picked": obs.own_picked,
        "offered_occupation": obs.offered_occupation,
        "offered_minor": obs.offered_minor,
        "remaining_to_pick": obs.remaining_to_pick,
        "request_token": obs.request_token,
        "history": obs.own_observation_history,
        "complete": complete,
        "own_final_hands": own_final_hands,
    })
}

fn parse_direction(value: Option<&str>, field: &str) -> Result<Direction, String> {
    match value {
        Some("left") => Ok(Direction::Left),
        Some("right") => Ok(Direction::Right),
        _ => Err(format!("{field} must be explicitly left or right")),
    }
}

/// Deterministic fixture pool. These IDs are NOT official cards; they exist so
/// the draft foundation can be exercised before the verified card pool (#115)
/// and the card subsystem (#139) are ready.
fn fixture_pool(mode: DraftMode, players: usize) -> DraftPool {
    let n = mode.dealt_per_kind() * players;
    DraftPool::new(
        (0..n).map(|i| format!("FIX-OCC-{i:02}")).collect(),
        (0..n).map(|i| format!("FIX-MIN-{i:02}")).collect(),
    )
}

fn optional_string(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err("expected a string card id".into()),
    }
}

fn string_list(value: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    let array = value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field} must be an array of card ids"))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{field} must contain only strings"))
        })
        .collect()
}

fn local_request_allowed(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let local_host = host.is_some_and(|value| match value.rsplit_once(':') {
        Some((name, value_port)) => {
            (name == "127.0.0.1" || name == "localhost") && value_port.parse::<u16>() == Ok(port)
        }
        None => false,
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
        ("GET", "/api/draft/state") => session.draft_view(),
        ("POST", "/api/draft/new")
        | ("POST", "/api/draft/pick")
        | ("POST", "/api/draft/choice")
        | ("POST", "/api/draft/reset") => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            match path {
                "/api/draft/new" => session.start_draft(&value)?,
                "/api/draft/pick" => session.draft_pick(&value)?,
                "/api/draft/choice" => session.draft_choice(&value)?,
                _ => session.reset_draft(),
            }
            Ok(session.view())
        }
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
pub fn serve(config: Config) -> Result<(), String> {
    let port: u16 = std::env::var("AGRICOLA_WEB_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8787);
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let mut session = Session::new(config)?;
    println!("Agricola local UI: http://127.0.0.1:{port}/");
    for accepted in listener.incoming() {
        match accepted {
            Ok(stream) => {
                if let Err(error) = handle(stream, &mut session, port) {
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
        assert!(!local_request_allowed(Some("127.0.0.1:9999"), None, 8787));
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

    #[test]
    fn draft_endpoints_require_explicit_directions_and_phase_order() {
        let mut session = Session::new(Config::default()).unwrap();
        assert!(
            session
                .start_draft(&json!({"mode":"draft_7_7","players":2}))
                .is_err(),
            "missing directions must be rejected, not defaulted"
        );
        assert!(
            session
                .start_draft(&json!({
                    "mode":"draft_7_7","players":2,
                    "direction_occupation":"left","direction_minor":"right"
                }))
                .is_err(),
            "7-7 needs an explicit phase order"
        );
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":3,
                "direction_occupation":"left","direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        assert!(session.draft_view().is_ok());
    }

    #[test]
    fn draft_web_plays_full_simultaneous_and_redacts_private_data() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_10_7_simultaneous","players":2,"seed":9,
                "direction_occupation":"left","direction_minor":"right"
            }))
            .unwrap();
        for _ in 0..40 {
            let view = session.draft_view().unwrap();
            if view["complete"].as_bool().unwrap() {
                break;
            }
            let token = view["request_token"].as_u64().unwrap();
            let occupation = view["offered_occupation"][0].as_str().unwrap().to_owned();
            let minor = view["offered_minor"][0].as_str().unwrap().to_owned();
            session
                .draft_pick(&json!({"token":token,"occupation":occupation,"minor":minor}))
                .unwrap();
        }
        let view = session.draft_view().unwrap();
        assert!(view["complete"].as_bool().unwrap());
        assert_eq!(view["phase"], "complete");
        assert_eq!(
            view["own_final_hands"]["occupations"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        // The browser view never carries the private seed or a deck field.
        assert!(view.get("seed").is_none());
        assert!(view.get("packets").is_none());
        // Other seats' final hands are not exposed in the viewer payload.
        assert!(view.get("final_hands").is_none());
        session
            .draft()
            .unwrap()
            .state
            .validate_invariants()
            .unwrap();
    }

    #[test]
    fn draft_web_rejected_pick_does_not_advance_cpu_or_state() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"draft_7_7","players":2,"seed":3,
                "direction_occupation":"left","direction_minor":"right",
                "occupation_phase_first":true
            }))
            .unwrap();
        let before = session.draft().unwrap().state.state_hash();
        let token = session.draft().unwrap().state.current_token();
        // Off-packet card: the human commit and the CPU follow-ups must not
        // happen at all.
        assert!(session
            .draft_pick(&json!({"token":token,"occupation":"NOT-OFFERED"}))
            .is_err());
        assert_eq!(session.draft().unwrap().state.state_hash(), before);
        assert!(session
            .draft()
            .unwrap()
            .state
            .commitments
            .iter()
            .all(Option::is_none));
        // A stale token is likewise atomic.
        assert!(session
            .draft_pick(&json!({"token":token + 5,"occupation":"NOT-OFFERED"}))
            .is_err());
        assert_eq!(session.draft().unwrap().state.state_hash(), before);
    }

    #[test]
    fn draft_web_auto_plays_cpu_choice_mode() {
        let mut session = Session::new(Config::default()).unwrap();
        session
            .start_draft(&json!({
                "mode":"choose_10_7","players":3,"seed":17,
                "direction_occupation":"right","direction_minor":"right"
            }))
            .unwrap();
        let view = session.draft_view().unwrap();
        let token = view["request_token"].as_u64().unwrap();
        let occ = view["offered_occupation"]
            .as_array()
            .unwrap()
            .iter()
            .take(7)
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let minor = view["offered_minor"]
            .as_array()
            .unwrap()
            .iter()
            .take(7)
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        session
            .draft_choice(&json!({"token":token,"occupations":occ,"minors":minor}))
            .unwrap();
        assert!(session.draft_view().unwrap()["complete"].as_bool().unwrap());
    }
}

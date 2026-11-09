//! M12 unified evaluation/learning episode API.
//!
//! `Episode` is a deliberately thin, observation-first wrapper around the
//! existing rule engine. It provides the stable boundary named in
//! `docs/ai-experiments-m12-m14.md` (`reset / observe / legal_actions / step /
//! clone / replay`) without reimplementing any rule.
//!
//! Guarantees:
//! * The engine validates and applies every action; this module never
//!   duplicates legality, scoring, harvest or card logic.
//! * **Trusted coordinator / untrusted policy boundary**: the coordinator owns
//!   [`Episode`], which can expose private replay metadata through `config()`
//!   and `events()`. Only pass [`PolicyInput`] (or its `observation` field) to
//!   AI policies. `PolicyInput` contains public state, that viewer's hand, and
//!   candidate/available action spaces, never the seed or event log.
//! * A fixed [`Config::seed`] reproduces the whole episode, and
//!   [`Episode::state_hash`]/[`Episode::replay`] stay compatible with the CLI
//!   and browser save/replay paths.
//! * Existing [`crate::mcts::MctsPolicy`] and
//!   [`crate::evaluation::EvaluationPolicy`] policies keep working because the
//!   observation type is unchanged.

use crate::cards::CardSet;
use crate::engine::{Action, FreeAction};
use crate::harvest::{HarvestDetailPolicy, SimpleHarvestDetails};
use crate::model::{Config, Event, Game, Observation, Space};
use crate::score::{standings_with_cards, Standings};
use serde::Serialize;

/// A reproducible game owned by a **trusted** coordinator.
///
/// Do not pass an `Episode` to an untrusted AI: `observe(other_player)`,
/// `config()` and `events()` are intentionally available to the coordinator
/// and can reveal private information or permit reconstruction from the seed.
/// Give policies only [`PolicyInput`] from [`Self::policy_input`].
#[derive(Clone, Debug)]
pub struct Episode {
    game: Game,
}

/// Whitelisted input safe to hand to an untrusted policy for one active turn.
///
/// Unlike `Episode`, this standalone value contains neither the initial seed
/// nor the full replay log. Construct it via [`Episode::policy_input`]; keep
/// `Episode`, `config()`, `events()` and `state_hash()` with the trusted host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyInput {
    /// Public state and the viewing player's own private hand only.
    pub observation: Observation,
    /// AI-generated candidates; not the complete legal action set.
    pub legal_actions: Vec<Space>,
    /// All structurally available spaces, including those outside AI candidates.
    pub available_actions: Vec<Space>,
}

/// Terminal summary for one episode.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EpisodeResult {
    /// Round reached at the end of the game (14 in a completed game).
    pub rounds: u8,
    pub standings: Standings,
    /// Official-rank first-place credit: `1/k` for each of the `k` tied
    /// leaders, `0` otherwise. This is an evaluation credit only and does not
    /// change the official ranking.
    pub first_credit: Vec<f64>,
}

impl Episode {
    /// Start a card-less episode from a fixed configuration.
    pub fn reset(config: Config) -> Result<Self, String> {
        Game::with_cards(config, CardSet::default()).map(|game| Self { game })
    }

    /// Start an episode with an explicit, already validated card set.
    pub fn reset_with_cards(config: Config, cards: CardSet) -> Result<Self, String> {
        Game::with_cards(config, cards).map(|game| Self { game })
    }

    /// Public state plus only this viewer's private hand.
    ///
    /// Opponent hands, the undealt round order and the internal RNG are never
    /// included, matching the M5/M8 observation contract.
    pub fn observe(&self, player: usize) -> Result<Observation, String> {
        self.game.observation(player)
    }

    /// Produce the *only* input a trusted coordinator should give to an AI.
    ///
    /// `Episode` itself is not a policy interface: its `config()` and
    /// `events()` can expose seed/replay data, and `observe()` can view any
    /// player's private hand. Policies receive this detached, serialized-safe
    /// snapshot for the current actor instead.
    pub fn policy_input(&self, player: usize) -> Result<PolicyInput, String> {
        self.require_turn(player)?;
        Ok(PolicyInput {
            observation: self.observe(player)?,
            legal_actions: self.game.legal_spaces(),
            available_actions: self.game.available_spaces(),
        })
    }

    pub fn player_count(&self) -> usize {
        self.game.public.players.len()
    }

    pub fn active_player(&self) -> usize {
        self.game.public.active_player
    }

    pub fn round(&self) -> u8 {
        self.game.public.round
    }

    pub fn is_terminal(&self) -> bool {
        self.game.public.finished
    }

    /// Candidate-narrowed action list for the active player.
    ///
    /// This is a subset of [`Self::available_actions`] and must not be treated
    /// as the definition of the legal action set.
    pub fn legal_actions(&self, player: usize) -> Result<Vec<Space>, String> {
        self.require_turn(player)?;
        Ok(self.game.legal_spaces())
    }

    /// Every structurally available action space for the active player.
    ///
    /// The engine accepts any legal detail here even when the candidate
    /// generator would never propose it (M8-2 contract).
    pub fn available_actions(&self, player: usize) -> Result<Vec<Space>, String> {
        self.require_turn(player)?;
        Ok(self.game.available_spaces())
    }

    /// Read-only validation through the exact transactional path used by
    /// [`Self::step`]. It never modifies the episode.
    pub fn validate(&self, player: usize, action: &Action) -> Result<(), String> {
        self.require_turn(player)?;
        self.game.validate_action(action)
    }

    /// Apply one action for the active player, resolving any harvest with the
    /// engine's default detail policy.
    pub fn step(&mut self, player: usize, action: Action) -> Result<(), String> {
        self.step_with_harvest_policy(player, action, &SimpleHarvestDetails)
    }

    /// Apply one action for the active player with an explicit harvest policy.
    /// Search code can supply its own feeding/breeding decisions while the
    /// engine still validates the action.
    ///
    /// Single validated transition (#255 Phase B): `apply_with_harvest_policy`
    /// is itself transactional — it only commits after its own validation
    /// passes, so a separate `validate_action` probe would run the whole
    /// clone+apply path twice. `require_turn` still guards turn ownership,
    /// and a rejected action leaves the state completely unchanged.
    pub fn step_with_harvest_policy(
        &mut self,
        player: usize,
        action: Action,
        harvest: &dyn HarvestDetailPolicy,
    ) -> Result<(), String> {
        self.require_turn(player)?;
        self.game.apply_with_harvest_policy(action, harvest)
    }

    /// Apply a worker-free action. The engine still checks legality, so the
    /// episode can call this even when it is not `player`'s placement turn.
    pub fn apply_free(&mut self, player: usize, free: FreeAction) -> Result<(), String> {
        if self.game.public.finished {
            return Err("game is finished".into());
        }
        if player >= self.game.public.players.len() {
            return Err("invalid player".into());
        }
        self.game.apply_free(player, free)
    }

    /// Full private-state hash, identical to the CLI and browser value.
    /// This is trusted-coordinator metadata, not a policy input.
    pub fn state_hash(&self) -> String {
        self.game.state_hash()
    }

    /// Committed event log for trusted replay/verification only.
    /// Never provide this to a policy (its history and hashes are not an
    /// observation-scoped information boundary).
    pub fn events(&self) -> &[Event] {
        &self.game.log.events
    }

    /// Trusted-coordinator configuration, including the **private seed**.
    /// Never provide to a policy: the seed can reconstruct hidden card order.
    pub fn config(&self) -> &Config {
        &self.game.config
    }

    /// Trusted-coordinator view of the underlying engine.
    ///
    /// This exists so the coordinator can serialise a complete
    /// [`crate::episode_data::EpisodeRecord`] (engine/rules versions, the
    /// initial state hash, the card set and the full event log) from an
    /// episode. Like [`Self::events`] and [`Self::config`], it exposes private
    /// state and must never be handed to an untrusted policy; give policies
    /// only [`Self::policy_input`].
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Terminal reward summary, or `None` while the game is still running.
    pub fn result(&self) -> Option<EpisodeResult> {
        if !self.is_terminal() {
            return None;
        }
        let standings = standings_with_cards(&self.game.public.players, &self.game.card_set);
        Some(EpisodeResult {
            rounds: self.game.public.round,
            first_credit: first_credit(&standings.rank),
            standings,
        })
    }

    /// Rebuild an episode from a committed event log, preserving the exact
    /// state-hash chain.
    pub fn replay(config: Config, cards: CardSet, events: &[Event]) -> Result<Self, String> {
        Game::replay_with_cards(config, cards, events).map(|game| Self { game })
    }

    fn require_turn(&self, player: usize) -> Result<(), String> {
        if player >= self.game.public.players.len() {
            return Err("invalid player".into());
        }
        if self.game.public.finished {
            return Err("game is finished".into());
        }
        if player != self.game.public.active_player {
            return Err("not the active player".into());
        }
        Ok(())
    }
}

/// `1/k` first-place credit for `k` tied leaders, `0` for every other rank.
fn first_credit(rank: &[usize]) -> Vec<f64> {
    let leaders = rank.iter().filter(|&&r| r == 1).count().max(1);
    rank.iter()
        .map(|&r| if r == 1 { 1.0 / leaders as f64 } else { 0.0 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::verified_basic_card_set;
    use crate::evaluation::EvaluationPolicy;
    use crate::mcts::MctsPolicy;
    use crate::policy::{DetailPolicy, SimpleDetails, SpacePolicy};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn config(seed: u64) -> Config {
        Config {
            seed,
            players: 2,
            additional_action_space_enabled: false,
        }
    }

    #[test]
    fn reset_is_deterministic_and_observation_hides_private_state() {
        let a = Episode::reset(config(31)).unwrap();
        let b = Episode::reset(config(31)).unwrap();
        assert_eq!(a.state_hash(), b.state_hash());

        let obs = a.observe(0).unwrap();
        let json = serde_json::to_string(&obs).unwrap();
        for secret in [
            "hidden_round_deck",
            "round_seed",
            "action_seed",
            "delayed_well",
            "\"rng\"",
        ] {
            assert!(!json.contains(secret), "observation leaked {secret}");
        }
        // Only the viewer's own hand is present.
        assert_eq!(obs.hand, a.game.cards.hands[0]);
        assert_eq!(obs.hand_counts[0], obs.hand.len());
    }

    #[test]
    fn legal_and_available_actions_require_the_active_player() {
        let episode = Episode::reset(config(9)).unwrap();
        let active = episode.active_player();
        let other = (active + 1) % episode.player_count();
        let legal = episode.legal_actions(active).unwrap();
        let available = episode.available_actions(active).unwrap();
        assert!(legal.iter().all(|space| available.contains(space)));
        assert!(episode.legal_actions(other).is_err());
        assert!(episode.observe(episode.player_count()).is_err());
    }

    #[test]
    fn wrong_player_step_is_rejected_without_changing_state() {
        let mut episode = Episode::reset(config(9)).unwrap();
        let active = episode.active_player();
        let other = (active + 1) % episode.player_count();
        let obs = episode.observe(active).unwrap();
        let space = episode.legal_actions(active).unwrap()[0];
        let action = SimpleDetails.produce(&obs, space).unwrap();
        let before = episode.state_hash();
        assert!(episode.step(other, action.clone()).is_err());
        assert_eq!(episode.state_hash(), before);
        episode.step(active, action).unwrap();
        assert_ne!(episode.state_hash(), before);
    }

    #[test]
    fn a_full_episode_replays_byte_for_byte_and_reports_first_credit() {
        let mut episode = Episode::reset(config(275)).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(275);
        for _ in 0..320 {
            if episode.is_terminal() {
                break;
            }
            let active = episode.active_player();
            let obs = episode.observe(active).unwrap();
            let legal = episode.legal_actions(active).unwrap();
            let space = EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap();
            let action = SimpleDetails.produce(&obs, space).unwrap();
            episode.step(active, action).unwrap();
        }
        assert!(episode.is_terminal());
        let result = episode.result().unwrap();
        assert_eq!(result.rounds, 14);
        assert_eq!(result.first_credit.len(), 2);
        let total: f64 = result.first_credit.iter().sum();
        assert!((total - 1.0).abs() < 1e-9, "first credit must sum to 1");

        let replay = Episode::replay(
            episode.config().clone(),
            CardSet::default(),
            episode.events(),
        )
        .unwrap();
        assert_eq!(replay.state_hash(), episode.state_hash());
        assert!(Episode::reset(config(275)).unwrap().result().is_none());
    }

    #[test]
    fn clone_is_independent_and_search_policies_can_drive_the_api() {
        let mut episode = Episode::reset(config(64)).unwrap();
        let snapshot = episode.clone();
        let active = episode.active_player();
        let obs = episode.observe(active).unwrap();
        let legal = episode.legal_actions(active).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let space = MctsPolicy {
            simulations: 1,
            tree_depth: 1,
        }
        .select(&obs, &legal, &mut rng)
        .unwrap();
        let action = SimpleDetails.produce(&obs, space).unwrap();
        episode.step(active, action).unwrap();
        assert_ne!(episode.state_hash(), snapshot.state_hash());
        assert_eq!(
            snapshot.state_hash(),
            Episode::reset(config(64)).unwrap().state_hash()
        );
    }

    #[test]
    fn verified_card_episode_exposes_only_the_viewers_hand() {
        let cards = verified_basic_card_set(2).unwrap();
        let episode = Episode::reset_with_cards(config(12), cards).unwrap();
        let obs = episode.observe(0).unwrap();
        assert_eq!(obs.hand, episode.game.cards.hands[0]);
        assert!(!obs.hand.is_empty());
        let opponent = &episode.game.cards.hands[1];
        assert!(!opponent.is_empty());
        for id in opponent {
            assert!(
                !obs.hand.contains(id),
                "opponent card {id} leaked into the viewer's hand"
            );
        }
        assert_eq!(obs.hand_counts[1], opponent.len());
        // The vocabulary is public, but the opponent's concrete hand is not.
        assert!(serde_json::to_string(&obs)
            .unwrap()
            .contains("card_definitions"));
    }
}

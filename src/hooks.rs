//! M5 extension interface for future occupation/minor-improvement effects.
//! M1 has no such cards. Hook inputs never include the hidden round deck or RNG.
use crate::model::{Good, Observation, Space};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookTiming {
    RoundOpened,
    BeforeWorker,
    AfterWorker,
    HarvestField,
    HarvestFeeding,
    HarvestBreeding,
    Scoring,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookTrigger {
    pub round: u8,
    pub timing: HookTiming,
    pub actor: Option<usize>,
    pub space: Option<Space>,
}

/// Effects describe deterministic engine commands, never imperative arbitrary
/// closures. Once cards are introduced in M5, the event dispatcher must log
/// the chosen commands to retain action-only replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardCommand {
    Gain {
        player: usize,
        good: Good,
        amount: u32,
    },
}

/// This interface is intentionally not invoked for M1's handless variant.
/// In M5 a dispatcher will request commands and pass them to the engine's
/// validated transaction layer, keeping AI decisions separate from rules.
pub trait CardEffectHook {
    fn produce(&self, observed: &Observation, trigger: &HookTrigger) -> Vec<CardCommand>;
}

pub struct NoCardEffects;
impl CardEffectHook for NoCardEffects {
    fn produce(&self, _: &Observation, _: &HookTrigger) -> Vec<CardCommand> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Config, Game};
    #[test]
    fn m1_hook_is_empty_and_consumes_only_public_observation() {
        let g = Game::new(Config::default());
        let obs = g.observation(0).unwrap();
        let trigger = HookTrigger {
            round: g.public.round,
            timing: HookTiming::BeforeWorker,
            actor: Some(0),
            space: None,
        };
        assert!(NoCardEffects.produce(&obs, &trigger).is_empty());
        assert!(!serde_json::to_string(&obs)
            .unwrap()
            .contains("hidden_round_deck"));
    }
}

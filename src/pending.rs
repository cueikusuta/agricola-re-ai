//! PendingDecision — interruptible player choices with save/restore/resume integrity.
//!
//! When an effect requires a player decision (e.g., choose gain, choose payment),
//! the engine creates a PendingDecision, pauses execution, and waits for the
//! player (or AI) to resolve it. The decision context is serialized so that
//! save/load/resume produces identical state hashes.

use crate::model::Good;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unique ID for a pending decision (monotonically increasing).
pub type DecisionId = u64;

/// The kind of decision pending.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingKind {
    /// Choose which good to gain and how much (e.g., "gain 1 food or 1 wood").
    ChooseGain {
        /// Source card ID that triggered this decision.
        source_card: String,
        /// Available options.
        options: Vec<GainOption>,
    },
    /// Choose how to pay a cost (e.g., "pay 3 wood or 2 wood + 1 stone").
    ChoosePayment {
        /// Card ID being played/purchased.
        card_id: String,
        /// Payment options.
        options: Vec<PaymentOption>,
    },
    /// Delayed well relief choice (public information per rules).
    /// Finding A from PR #265: delayed_well is hidden ledger but well purchases
    /// are PUBLIC actions. The schedule is mirrored in Observation.
    DelayedWellRelief {
        /// Round when food is granted.
        round: u8,
        /// Players who receive food (public knowledge).
        recipients: Vec<usize>,
    },
    /// Generic extensible decision for future effects.
    Custom {
        source_card: String,
        /// Opaque payload for the specific effect.
        payload: serde_json::Value,
    },
}

/// A single gain option.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GainOption {
    pub good: Good,
    pub amount: u32,
}

/// A single payment option (multiset of goods).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentOption {
    pub cost: BTreeMap<Good, u32>,
}

/// A pending decision awaiting resolution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDecision {
    pub id: DecisionId,
    pub player: usize,
    pub kind: PendingKind,
    /// State hash BEFORE the decision was created. Must match on resume.
    pub state_hash_before: String,
    /// Additional context for the resolver (e.g., which space triggered it).
    pub context: serde_json::Value,
}

/// Resolution provided by the player/AI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingResolution {
    ChooseGain { chosen: GainOption },
    ChoosePayment { chosen: PaymentOption },
    DelayedWellRelief { confirmed: bool },
    Custom { payload: serde_json::Value },
}

impl PendingDecision {
    /// Create a new pending decision with the current state hash.
    pub fn new(
        id: DecisionId,
        player: usize,
        kind: PendingKind,
        state_hash_before: String,
        context: serde_json::Value,
    ) -> Self {
        Self {
            id,
            player,
            kind,
            state_hash_before,
            context,
        }
    }

    /// Validate that the provided resolution matches this decision's kind.
    pub fn validate_resolution(&self, resolution: &PendingResolution) -> Result<(), String> {
        match (&self.kind, resolution) {
            (PendingKind::ChooseGain { options, .. }, PendingResolution::ChooseGain { chosen }) => {
                if options.iter().any(|o| o == chosen) {
                    Ok(())
                } else {
                    Err("chosen gain option not in available options".into())
                }
            }
            (
                PendingKind::ChoosePayment { options, .. },
                PendingResolution::ChoosePayment { chosen },
            ) => {
                if options.iter().any(|o| o == chosen) {
                    Ok(())
                } else {
                    Err("chosen payment option not in available options".into())
                }
            }
            (
                PendingKind::DelayedWellRelief { .. },
                PendingResolution::DelayedWellRelief { .. },
            ) => Ok(()),
            (PendingKind::Custom { .. }, PendingResolution::Custom { .. }) => Ok(()),
            _ => Err("resolution kind does not match pending decision kind".into()),
        }
    }
}

/// Game extension for pending decision management.
pub trait PendingExt {
    /// Returns true if there is a pending decision.
    fn has_pending(&self) -> bool;

    /// Resolve the pending decision. The state_hash_before must match the
    /// current game state hash, otherwise returns Err (prevents replay attacks).
    fn resolve_pending(&mut self, decision: PendingResolution) -> Result<(), String>;

    /// Get a reference to the pending decision (for inspection/UI).
    fn pending(&self) -> Option<&PendingDecision>;

    /// Get a mutable reference to the pending decision.
    fn pending_mut(&mut self) -> Option<&mut PendingDecision>;
}

// The actual implementation is in model.rs on the Game struct.
// This trait is for documentation and potential future abstraction.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Config, Game, Good};

    #[test]
    fn pending_decision_serialize_roundtrip() {
        let decision = PendingDecision::new(
            1,
            0,
            PendingKind::ChooseGain {
                source_card: "TEST".into(),
                options: vec![
                    GainOption {
                        good: Good::Food,
                        amount: 1,
                    },
                    GainOption {
                        good: Good::Wood,
                        amount: 2,
                    },
                ],
            },
            "abc123".into(),
            serde_json::json!({ "space": "Forest" }),
        );

        let json = serde_json::to_string(&decision).unwrap();
        let decoded: PendingDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(decision, decoded);
    }

    #[test]
    fn pending_decision_validate_resolution() {
        let decision = PendingDecision::new(
            1,
            0,
            PendingKind::ChooseGain {
                source_card: "TEST".into(),
                options: vec![
                    GainOption {
                        good: Good::Food,
                        amount: 1,
                    },
                    GainOption {
                        good: Good::Wood,
                        amount: 2,
                    },
                ],
            },
            "hash".into(),
            serde_json::json!({}),
        );

        assert!(decision
            .validate_resolution(&PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Food,
                    amount: 1
                }
            })
            .is_ok());

        assert!(decision
            .validate_resolution(&PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Stone,
                    amount: 1
                }
            })
            .is_err());
    }

    #[test]
    fn payment_option_equality() {
        let mut opt1 = BTreeMap::new();
        opt1.insert(Good::Wood, 3);
        let mut opt2 = BTreeMap::new();
        opt2.insert(Good::Wood, 3);
        let mut opt3 = BTreeMap::new();
        opt3.insert(Good::Clay, 2);

        assert_eq!(opt1, opt2);
        assert_ne!(opt1, opt3);
    }

    /// End-to-end PendingDecision lifecycle: create -> save -> load -> resume,
    /// with `integrity_hash` stable across the round-trip and the pending
    /// decision still resolvable afterwards.
    #[test]
    fn pending_decision_survives_save_restore_and_resumes() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 11,
            additional_action_space_enabled: false,
        });
        let decision = PendingDecision::new(
            game.next_decision_id,
            0,
            PendingKind::ChooseGain {
                source_card: "A001".into(),
                options: vec![
                    GainOption {
                        good: Good::Food,
                        amount: 2,
                    },
                    GainOption {
                        good: Good::Wood,
                        amount: 1,
                    },
                ],
            },
            game.state_hash(),
            serde_json::json!({ "space": "Forest" }),
        );
        game.next_decision_id += 1;
        game.pending = Some(decision.clone());
        assert!(game.has_pending());

        let integrity = game.integrity_hash();
        let saved = game.save_json().unwrap();

        let mut restored = Game::load_json(&saved).unwrap();
        assert_eq!(
            restored.integrity_hash(),
            integrity,
            "integrity hash must survive save/load"
        );
        assert_eq!(
            restored.pending(),
            Some(&decision),
            "pending decision must survive save/load"
        );
        assert_eq!(
            restored.next_decision_id, 1,
            "decision counter must survive save/load"
        );
        assert!(restored.has_pending());

        // Resume: the stored state_hash_before still matches, so the choice applies.
        let food_before = restored.public.players[0].supply.food;
        restored
            .resolve_pending(PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Food,
                    amount: 2,
                },
            })
            .unwrap();
        assert_eq!(restored.public.players[0].supply.food, food_before + 2);
        assert!(!restored.has_pending(), "decision cleared after resolution");
    }

    /// A mismatch between the recorded `state_hash_before` and the current
    /// state must refuse the resolution and keep the decision pending.
    #[test]
    fn pending_decision_rejects_a_changed_state_and_stays_pending() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 12,
            additional_action_space_enabled: false,
        });
        game.pending = Some(PendingDecision::new(
            game.next_decision_id,
            0,
            PendingKind::ChooseGain {
                source_card: "A001".into(),
                options: vec![GainOption {
                    good: Good::Food,
                    amount: 1,
                }],
            },
            game.state_hash(),
            serde_json::json!({}),
        ));
        // Mutate the game after the decision was created.
        game.public.players[0].supply.wood += 1;

        let err = game
            .resolve_pending(PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Food,
                    amount: 1,
                },
            })
            .unwrap_err();
        assert!(err.contains("state hash mismatch"), "got: {err}");
        assert!(
            game.has_pending(),
            "decision must stay pending after a rejected resume"
        );
    }

    /// Only one pending decision can be outstanding: resolving without one is
    /// an explicit error rather than a silent no-op.
    #[test]
    fn resolving_without_a_pending_decision_is_an_error() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 13,
            additional_action_space_enabled: false,
        });
        assert!(!game.has_pending());
        let err = game
            .resolve_pending(PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Food,
                    amount: 1,
                },
            })
            .unwrap_err();
        assert!(err.contains("no pending decision"), "got: {err}");
    }

    /// Per-card use limits and the event budget are both enforced (Issue #266
    /// acceptance: "use limits and unbounded-chain guards proven by tests").
    #[test]
    fn card_use_limit_and_event_budget_are_enforced() {
        use crate::card_events::dispatch_start_round;
        use crate::cards::CardSet;
        // Build a two-card chain where the same card fires repeatedly.
        let set = CardSet {
            definitions: vec![crate::cards::CardDefinition {
                id: "LOOP".into(),
                kind: crate::cards::CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![crate::cards::CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 1,
                    house: None,
                    rooms: None,
                }],
            }],
        };
        let mut players = vec![crate::model::Player::new(0, 0)];
        for _ in 0..5 {
            players[0].played_cards.push("LOOP".into());
        }
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = std::collections::BTreeMap::new();
        let mut uses = std::collections::BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;

        // Budget generous: all five instances resolve and each is counted.
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &mut pending,
        )
        .unwrap();
        assert_eq!(players[0].supply.food, 5);
        assert_eq!(
            uses.get("LOOP"),
            Some(&5),
            "each resolution must be counted"
        );
        assert_eq!(budget, 59, "one budget unit per resolved effect");

        // Now a budget of 2 must stop the chain mid-way with an explicit error.
        let mut players2 = vec![crate::model::Player::new(0, 0)];
        for _ in 0..5 {
            players2[0].played_cards.push("LOOP".into());
        }
        let mut uses2 = std::collections::BTreeMap::new();
        let mut budget2 = 2u32;
        let mut pending2 = None;
        let err = dispatch_start_round(
            &set,
            &mut players2,
            &mut cards,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget2,
            &mut uses2,
            &mut pending2,
        )
        .unwrap_err();
        assert!(err.contains("budget exhausted"), "got: {err}");
        assert_eq!(budget2, 0, "the budget is fully consumed before the error");
        assert_eq!(
            players2[0].supply.food, 2,
            "only the affordable prefix of the chain resolved"
        );
    }
}

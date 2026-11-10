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
            (PendingKind::DelayedWellRelief { .. }, PendingResolution::DelayedWellRelief { .. }) => {
                Ok(())
            }
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
    use crate::model::{Good, Config, Game};

    #[test]
    fn pending_decision_serialize_roundtrip() {
        let decision = PendingDecision::new(
            1,
            0,
            PendingKind::ChooseGain {
                source_card: "TEST".into(),
                options: vec![
                    GainOption { good: Good::Food, amount: 1 },
                    GainOption { good: Good::Wood, amount: 2 },
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
                    GainOption { good: Good::Food, amount: 1 },
                    GainOption { good: Good::Wood, amount: 2 },
                ],
            },
            "hash".into(),
            serde_json::json!({}),
        );

        assert!(decision
            .validate_resolution(&PendingResolution::ChooseGain {
                chosen: GainOption { good: Good::Food, amount: 1 }
            })
            .is_ok());

        assert!(decision
            .validate_resolution(&PendingResolution::ChooseGain {
                chosen: GainOption { good: Good::Stone, amount: 1 }
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
}
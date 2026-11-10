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
                use_limit: None,
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
        let card_use_limits = std::collections::BTreeMap::new();
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
            &card_use_limits,
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
            &card_use_limits,
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

    /// A save whose Step 3 bookkeeping (here: the pending decision) was edited
    /// after the fact must be rejected on load. This is the defence the
    /// `integrity_hash` stamping in `save_json`/`load_json` provides.
    #[test]
    fn a_tampered_pending_decision_is_rejected_on_load() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 21,
            additional_action_space_enabled: false,
        });
        game.pending = Some(PendingDecision::new(
            0,
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
        let saved = game.save_json().unwrap();
        assert!(
            Game::load_json(&saved).is_ok(),
            "the untampered save must load"
        );

        // Tamper: change the pending decision's options (a bigger gain).
        let tampered = saved.replace("\"amount\": 1", "\"amount\": 99");
        assert_ne!(tampered, saved, "the tamper must actually change the file");
        let err = Game::load_json(&tampered).unwrap_err();
        assert!(
            err.contains("integrity"),
            "tampering must be caught by the integrity stamp, got: {err}"
        );
    }

    /// A legacy (pre-Step3) save has no integrity stamp and no pending
    /// decision; it must still load, while one that somehow carries a pending
    /// decision is refused.
    #[test]
    fn a_legacy_save_without_a_stamp_still_loads() {
        let game = Game::new(Config {
            players: 2,
            seed: 22,
            additional_action_space_enabled: false,
        });
        let mut value: serde_json::Value =
            serde_json::from_str(&game.save_json().unwrap()).unwrap();
        // Drop the stamp to emulate a pre-Step3 save.
        value["log"]
            .as_object_mut()
            .unwrap()
            .remove("integrity_hash");
        let legacy = serde_json::to_string(&value).unwrap();
        assert!(
            Game::load_json(&legacy).is_ok(),
            "a legacy save without the stamp must keep loading"
        );

        // Same file, but with a pending decision injected: refuse it.
        value["pending"] = serde_json::json!({
            "id": 0,
            "player": 0,
            "kind": {"kind": "custom", "source_card": "A001", "payload": null},
            "state_hash_before": "",
            "context": null
        });
        let forged = serde_json::to_string(&value).unwrap();
        let err = Game::load_json(&forged).unwrap_err();
        assert!(
            err.contains("legacy save must not carry a pending decision"),
            "got: {err}"
        );
    }

    /// The event budget bounds one action's chain and is replenished for the
    /// next action, so it cannot starve a long game.
    #[test]
    fn the_event_budget_is_replenished_per_action() {
        use crate::model::DEFAULT_EVENT_BUDGET;
        let mut game = Game::new(Config {
            players: 2,
            seed: 23,
            additional_action_space_enabled: false,
        });
        // Simulate a chain that consumed the whole budget in a previous action.
        game.event_budget = 0;
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        use crate::policy::DetailPolicy;
        let action = crate::policy::SimpleDetails
            .produce(&obs, legal[0])
            .expect("a detail for the first legal space");
        game.apply(action).unwrap();
        assert_eq!(
            game.event_budget, DEFAULT_EVENT_BUDGET,
            "a new action must start from the full budget, not a depleted one"
        );
    }

    /// The production engine path must enforce the same guards as the
    /// dispatcher: a card's declared per-round use limit is honoured by a real
    /// CommittedAction, not just by a direct dispatcher call.
    #[test]
    fn a_real_card_use_limit_is_enforced_through_the_engine() {
        use crate::cards::{CardDefinition, CardEffect, CardKind, CardSet};
        // Occupation whose effects fire on every worker placement, limited to
        // one resolution per round.
        let card = |id: &str| CardDefinition {
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterWorkerGain {
                space: crate::model::Space::DayLaborer,
                good: Good::Wood,
                amount: 1,
            }],
            use_limit: Some(1),
        };
        let set = CardSet {
            definitions: vec![card("LIMITED"), card("USER")],
        };
        let mut game = Game::new(Config {
            players: 2,
            seed: 31,
            additional_action_space_enabled: false,
        });
        // Install the probe set directly: the deck-shape validation applies to
        // real pools, while this test isolates the use-limit guard.
        game.card_set = std::sync::Arc::new(set.clone());
        game.card_use_limits = set
            .definitions
            .iter()
            .filter_map(|c| c.use_limit.map(|l| (c.id.clone(), l)))
            .collect();
        game.cards = crate::cards::CardState {
            hands: vec![vec![], vec![]],
            undealt: vec![],
        };
        // Both cards are played in front of the ACTIVE player, so both would
        // fire on DayLaborer and the second one must hit its limit.
        let active = game.public.active_player;
        // The same id twice: two resolutions of ONE limited card. This isolates
        // the limit from card-count effects.
        game.public.players[active].played_cards =
            vec!["LIMITED".to_string(), "LIMITED".to_string()];

        // The engine reads the limit from card data.
        assert_eq!(game.card_use_limits.get("LIMITED"), Some(&1));

        // A legal DayLaborer action must fail: the second resolution hits the
        // limit, which the engine reports rather than silently over-granting.
        let action = crate::engine::Action {
            space: crate::model::Space::DayLaborer,
            detail: crate::engine::Detail::None,
        };
        let err = game.apply(action).unwrap_err();
        assert!(
            err.contains("use limit"),
            "the production path must enforce the declared limit, got: {err}"
        );
    }

    /// A card without a declared limit resolves freely, so limits are opt-in
    /// data and do not change existing card behaviour.
    #[test]
    fn cards_without_a_declared_limit_are_unlimited() {
        use crate::cards::{CardDefinition, CardEffect, CardKind, CardSet};
        let set = CardSet {
            definitions: vec![CardDefinition {
                id: "FREE".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::AfterWorkerGain {
                    space: crate::model::Space::DayLaborer,
                    good: Good::Wood,
                    amount: 1,
                }],
                use_limit: None,
            }],
        };
        let mut game = Game::new(Config {
            players: 2,
            seed: 32,
            additional_action_space_enabled: false,
        });
        game.card_set = std::sync::Arc::new(set.clone());
        game.card_use_limits = set
            .definitions
            .iter()
            .filter_map(|c| c.use_limit.map(|l| (c.id.clone(), l)))
            .collect();
        game.cards = crate::cards::CardState {
            hands: vec![vec![], vec![]],
            undealt: vec![],
        };
        assert!(
            game.card_use_limits.is_empty(),
            "no limit declared means no entry"
        );
        let active = game.public.active_player;
        game.public.players[active].played_cards.push("FREE".into());
        game.apply(crate::engine::Action {
            space: crate::model::Space::DayLaborer,
            detail: crate::engine::Detail::None,
        })
        .unwrap();
        assert_eq!(
            game.public.players[active].supply.wood, 1,
            "an unlimited card resolves normally"
        );
    }
}

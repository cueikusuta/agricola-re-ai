//! PendingDecision — interruptible player choices with save/restore/resume integrity.
//!
//! When an effect requires a player decision (e.g., choose gain, choose payment),
//! the engine creates a PendingDecision, pauses execution, and waits for the
//! player (or AI) to resolve it. The decision context is serialized so that
//! save/load/resume produces identical state hashes.

use crate::breeding::BreedingPlan;
use crate::card_events::TriggerPhase;
use crate::engine::Action;
use crate::harvest::FeedingPlan;
use crate::model::{Good, Player};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unique ID for a pending decision (monotonically increasing).
pub type DecisionId = u64;

/// Outcome of one engine transition (Issue #268: a pause is a distinct
/// outcome, not an `Err("pending decision")` sentinel).
///
/// `apply` returns [`Transition::Completed`] when the whole action —
/// including its card-effect chain — is applied and committed. It returns
/// [`Transition::Paused`] when a card effect interrupted the chain: the
/// mid-action state IS committed (with the decision stored in
/// `Game::pending`), and the remainder runs after `resolve_pending`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transition {
    /// Fully applied; no decision outstanding.
    Completed,
    /// Paused: the carried decision is stored on the game and awaits
    /// `resolve_pending`.
    Paused(Box<PendingDecision>),
}

/// Where in a card's effect list a pause occurred, so the resumed scan can
/// skip everything up to and including this position (Issue #268). The
/// indices address the same scan order the dispatcher used: `card_index`
/// over the acting player's `played_cards`, `effect_index` over that
/// card's `effects`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChainPosition {
    pub card_index: usize,
    pub effect_index: usize,
}

/// One dispatcher of the post-work effect chain. The pause/resume contract
/// in this issue covers the post-work chain; other dispatch sites surface
/// typed errors instead (they never half-commit a decision).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainStep {
    AfterHarvestBake,
    AfterClayConstruction,
    AfterWorker,
}

/// Where the post-work chain paused: which dispatcher step, which phase
/// inside that step, and the effect position to resume strictly after.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostWorkResume {
    pub step: ChainStep,
    pub phase: TriggerPhase,
    pub after: ChainPosition,
}

/// The remainder of a work action interrupted by a pending decision
/// (Issue #268). Everything the resumed execution needs is frozen here
/// because the committed pause state no longer contains it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkCompletion {
    /// The action whose effect chain paused. Its space is not yet in
    /// `used` and its worker not yet counted: both happen at completion.
    pub action: Action,
    /// Pre-action snapshot of the acting player. The clay-construction
    /// trigger compares against it, so it must survive the pause.
    pub previous_player: Player,
    /// Explicit feeding plans supplied to the original apply. `None` means
    /// the resumed completion generates plans with the default policy
    /// (`SimpleHarvestDetails`); the policy object itself is not
    /// serialized.
    pub harvest_plans: Option<Vec<FeedingPlan>>,
    /// Explicit breeding plans, same contract as `harvest_plans`.
    pub breeding_plans: Option<Vec<BreedingPlan>>,
    /// Pre-action state hash for the work event's `before` field when the
    /// game records events; empty string otherwise.
    pub logged_before: String,
}

/// What to execute after the decision resolves (Issue #268 typed
/// continuation). `None` covers decisions created outside the transactional
/// action path (direct injection, legacy saves): resolving them applies the
/// choice and nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Continuation {
    /// No engine chain to resume.
    #[default]
    None,
    /// Resume the post-work card chain from `resume`, then finish the
    /// paused work action described by `completion`.
    PostWork {
        player: usize,
        resume: PostWorkResume,
        completion: Box<WorkCompletion>,
    },
}

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
    /// How to continue after resolution (Issue #268). Defaults to `None`
    /// so pre-#268 saves keep loading.
    #[serde(default)]
    pub continuation: Continuation,
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
    #[allow(clippy::too_many_arguments)]
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
            continuation: Continuation::None,
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
    ///
    /// Applying the resolution does NOT by itself finish a paused work
    /// action: after this returns Ok the game is mid-action with the
    /// decision cleared but `Continuation::PostWork` still to run. Call
    /// `Game::resume_pending_work` (via `apply_pending_resolution` /
    /// `resolve_pending_and_resume`) to complete the action.
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

    /// Minimum reproduction for P1-B (Issue #268): the previous
    /// `resolve_pending` called `take()` before validating, so a selection that
    /// was not one of the offered options destroyed the decision and left the
    /// game unresolvable. The decision, the state hash and the supply must all
    /// be untouched by the rejected attempt.
    #[test]
    fn an_invalid_selection_leaves_the_decision_and_the_state_unchanged() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 41,
            additional_action_space_enabled: false,
        });
        let options = vec![
            GainOption {
                good: Good::Food,
                amount: 1,
            },
            GainOption {
                good: Good::Wood,
                amount: 2,
            },
        ];
        let decision = PendingDecision::new(
            game.next_decision_id,
            0,
            PendingKind::ChooseGain {
                source_card: "A001".into(),
                options,
            },
            game.state_hash(),
            serde_json::json!({}),
        );
        game.pending = Some(decision.clone());

        let hash_before = game.state_hash();
        let integrity_before = game.integrity_hash();
        let supply_before = game.public.players[0].supply.clone();

        // Stone is not offered.
        let err = game
            .resolve_pending(PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Stone,
                    amount: 1,
                },
            })
            .unwrap_err();
        assert!(
            err.contains("not in available options"),
            "the selection must be rejected as unavailable, got: {err}"
        );
        assert_eq!(
            game.pending(),
            Some(&decision),
            "a rejected selection must not consume the decision"
        );
        assert_eq!(game.state_hash(), hash_before, "state must be unchanged");
        assert_eq!(
            game.integrity_hash(),
            integrity_before,
            "no bookkeeping may change either"
        );
        assert_eq!(
            game.public.players[0].supply, supply_before,
            "no resource may be granted"
        );

        // The decision is still usable: a legal option now succeeds.
        game.resolve_pending(PendingResolution::ChooseGain {
            chosen: GainOption {
                good: Good::Wood,
                amount: 2,
            },
        })
        .expect("a legal selection must still resolve after a rejected attempt");
        assert_eq!(game.public.players[0].supply.wood, supply_before.wood + 2);
        assert!(!game.has_pending());
    }

    /// Same defect, different branch: a resolution of the wrong kind used to be
    /// an error raised *after* the decision had already been taken.
    #[test]
    fn a_resolution_of_the_wrong_kind_keeps_the_decision_pending() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 42,
            additional_action_space_enabled: false,
        });
        let decision = PendingDecision::new(
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
        );
        game.pending = Some(decision.clone());
        let mut cost = BTreeMap::new();
        cost.insert(Good::Wood, 1);

        let err = game
            .resolve_pending(PendingResolution::ChoosePayment {
                chosen: PaymentOption { cost },
            })
            .unwrap_err();
        assert!(err.contains("does not match"), "got: {err}");
        assert_eq!(
            game.pending(),
            Some(&decision),
            "a kind mismatch must not consume the decision"
        );
    }

    /// Minimum reproduction for the ChoosePayment atomicity requirement
    /// (Issue #268): `Food` sorts before `Wood`, so the old sequential loop
    /// deducted the affordable food and only then failed on the wood.
    #[test]
    fn a_failed_multi_good_payment_leaves_no_partial_deduction() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 43,
            additional_action_space_enabled: false,
        });
        game.public.players[0].supply.food = 1;
        game.public.players[0].supply.wood = 1;

        let mut cost = BTreeMap::new();
        cost.insert(Good::Food, 1);
        cost.insert(Good::Wood, 5);
        let offered = PaymentOption { cost };
        let decision = PendingDecision::new(
            game.next_decision_id,
            0,
            PendingKind::ChoosePayment {
                card_id: "A002".into(),
                options: vec![offered.clone()],
            },
            game.state_hash(),
            serde_json::json!({}),
        );
        game.pending = Some(decision.clone());

        // The offered option is legal as a *choice*, but unaffordable.
        let err = game
            .resolve_pending(PendingResolution::ChoosePayment {
                chosen: offered.clone(),
            })
            .unwrap_err();
        assert!(err.contains("insufficient"), "got: {err}");
        assert_eq!(
            game.public.players[0].supply.food, 1,
            "the affordable prefix of the cost must not be deducted"
        );
        assert_eq!(game.public.players[0].supply.wood, 1);
        assert_eq!(
            game.pending(),
            Some(&decision),
            "a failed payment must not consume the decision"
        );

        // Once affordable, the same choice deducts the whole cost exactly once.
        game.public.players[0].supply.wood = 5;
        let hash_after_topup = game.state_hash();
        game.pending.as_mut().unwrap().state_hash_before = hash_after_topup;
        game.resolve_pending(PendingResolution::ChoosePayment { chosen: offered })
            .expect("an affordable payment resolves");
        assert_eq!(game.public.players[0].supply.food, 0);
        assert_eq!(game.public.players[0].supply.wood, 0);
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

    /// End-to-end test for Issue #268 completion condition:
    /// real card event → pause → save → restore → resolve → resume → replay succeeds consistently.
    ///
    /// This test uses a real card (PendingProbe) that triggers a pending decision during
    /// the post-work card effect chain, exercises the full pause/resume cycle including
    /// save/restore, and verifies the replay produces identical state.
    #[test]
    fn e2e_real_card_event_pause_save_restore_resolve_resume_replay() {
        use crate::cards::{CardDefinition, CardEffect, CardKind, CardSet};
        use crate::engine::Action;
        use crate::model::Space;

        // Create a valid card set with 14 occupations and 14 minor improvements
        // (required by CardState::deal for 2 players: 7 cards of each kind per player).
        let mut definitions = Vec::new();
        // The probe card that triggers a pending decision on worker completion (Occupation played via Lessons)
        definitions.push(CardDefinition {
            use_limit: None,
            id: "CHOICE_CARD".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::PendingProbe {
                options: vec![(Good::Food, 2), (Good::Wood, 1)],
            }],
        });
        // Add 13 more occupations to reach 14 total
        for i in 1..=13 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("OCC_{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        // Add 14 minor improvements
        for i in 1..=14 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 1,
                effects: vec![],
            });
        }
        let set = CardSet { definitions };

        // Create a game with the custom card set and exact initial hands from the start
        let active = 0; // We'll make player 0 the active player
                        // Split 14 occupations and 14 minors between the two players (7 each)
        let occ_ids: Vec<String> = (1..=13).map(|i| format!("OCC_{i}")).collect();
        let minor_ids: Vec<String> = (1..=14).map(|i| format!("MINOR_{i}")).collect();
        let mut hands = vec![
            vec!["CHOICE_CARD".to_string()], // Player 0 gets the probe card (Occupation)
            vec![],
        ];
        // Player 0: probe card (Occupation) + 6 more occupations + 7 minor improvements = 14 cards
        hands[0].extend(occ_ids.iter().cloned().take(6));
        hands[0].extend(minor_ids[0..7].iter().cloned());
        // Player 1: 7 occupations + 7 minor improvements = 14 cards
        hands[1].extend(occ_ids[6..13].iter().cloned());
        hands[1].extend(minor_ids[7..14].iter().cloned());

        let mut game = Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 42,
                additional_action_space_enabled: false,
            },
            set.clone(),
            hands.clone(),
        )
        .expect("game creation must succeed");
        // Ensure player 0 is the active player (start player)
        game.public.active_player = active;
        game.public.start_player = active;
        game.card_use_limits = set
            .definitions
            .iter()
            .filter_map(|c| c.use_limit.map(|l| (c.id.clone(), l)))
            .collect();

        // First worker action: Player 0 uses Lessons to play the probe card (Occupation).
        // This triggers a pause during AfterWorker phase (probe card fires in Interrupt phase).
        // We resolve it AND resume the continuation to complete the play action.
        let play_action = Action {
            space: Space::Lessons,
            detail: crate::engine::Detail::PlayOccupation {
                card_id: "CHOICE_CARD".into(),
            },
        };
        let play_result = game.apply_with_plans(
            play_action,
            None,
            None,
            &crate::harvest::SimpleHarvestDetails,
        );
        assert!(
            matches!(play_result, Ok(Transition::Paused(_))),
            "playing probe card should pause during AfterWorker, got: {play_result:?}"
        );
        assert!(game.has_pending(), "pending decision after play action");
        // Resolve the pause AND resume the continuation (completes the work action)
        let play_resolution = crate::pending::PendingResolution::ChooseGain {
            chosen: crate::pending::GainOption {
                good: Good::Food,
                amount: 2,
            },
        };
        let play_resume_result = game
            .resolve_pending_and_resume(play_resolution)
            .expect("resolve and resume for play action must succeed");
        assert_eq!(
            play_resume_result,
            Transition::Completed,
            "play action resume should complete"
        );
        assert!(
            !game.has_pending(),
            "no pending decision after play action resume"
        );

        // Second worker action: Player 1 does a dummy action (DayLaborer) so turn advances back to player 0.
        let dummy_action = Action {
            space: Space::DayLaborer,
            detail: crate::engine::Detail::None,
        };
        game.apply(dummy_action).expect("dummy action must succeed");

        // Third worker action: Player 0 uses Forest. The probe card is now in played_cards,
        // so its PendingProbe effect will fire during the AfterWorker -> Interrupt phase chain,
        // triggering the pause we want to test.
        let action = Action {
            space: Space::Forest,
            detail: crate::engine::Detail::None,
        };
        let result = game.apply_with_plans(
            action.clone(),
            None,
            None,
            &crate::harvest::SimpleHarvestDetails,
        );

        // The chain should pause with a pending decision
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "worker action should pause, got: {result:?}"
        );
        assert!(
            game.has_pending(),
            "game should have pending decision after pause"
        );

        // Save the game state (includes the pending decision with continuation)
        let saved = game.save_json().expect("save must succeed");
        let integrity_hash = game.integrity_hash();

        // Restore the game from save
        let mut restored = Game::load_json(&saved).expect("load must succeed");
        assert_eq!(
            restored.integrity_hash(),
            integrity_hash,
            "integrity hash must survive save/load"
        );
        assert!(
            restored.has_pending(),
            "pending decision must survive save/load"
        );

        // Resolve the pending decision AND resume the continuation
        let food_before = restored.public.players[active].supply.food;
        let resolution = crate::pending::PendingResolution::ChooseGain {
            chosen: crate::pending::GainOption {
                good: Good::Food,
                amount: 2,
            },
        };
        let resume_result = restored
            .resolve_pending_and_resume(resolution)
            .expect("resolve and resume must succeed");

        // The action should now be fully completed
        assert_eq!(
            resume_result,
            Transition::Completed,
            "resume should complete the action"
        );
        assert!(
            !restored.has_pending(),
            "decision should be cleared after resolution"
        );

        // Verify the choice was applied (the resolution grants 2 food). The
        // absolute total is not asserted: the play action earlier in the game
        // also resolved a probe choice, so only the delta is meaningful.
        assert_eq!(
            restored.public.players[active].supply.food,
            food_before + 2,
            "the resumed choice must grant exactly its 2 food"
        );

        // Verify the action completed for real (space marked used, worker placed):
        // a resume that merely applied the choice would leave these unset.
        assert!(
            restored.public.used.contains(&Space::Forest),
            "action space should be marked used"
        );
        assert_eq!(
            restored.public.workers_placed[active], 2,
            "both of player 0's workers (Lessons then Forest) must be placed"
        );

        // Now test replay: replay the events from the restored game and verify
        // the final state matches
        let replayed = Game::replay_with_setup_and_start_player(
            restored.config.clone(),
            (*restored.card_set).clone(),
            restored.log.initial_hands.clone(),
            restored.log.initial_start_player_override,
            &restored.log.events,
        )
        .expect("replay must succeed");

        // The replayed game should have the same state hash as the restored game
        assert_eq!(
            replayed.state_hash(),
            restored.state_hash(),
            "replay must produce identical state hash"
        );
        assert_eq!(
            replayed.integrity_hash(),
            restored.integrity_hash(),
            "replay must produce identical integrity hash"
        );

        // Verify the replayed game also has the final food total and used space
        assert_eq!(
            replayed.public.players[active].supply.food,
            restored.public.players[active].supply.food,
            "replay must reproduce the final food total"
        );
        assert!(
            replayed.public.used.contains(&Space::Forest),
            "replay must have the used space"
        );
        assert_eq!(
            replayed.public.workers_placed, restored.public.workers_placed,
            "replay must reproduce the worker placements"
        );
    }

    /// R2-P0-2 regression: the one-shot `apply` API must refuse a card-effect
    /// interrupt with a precise `Err` and leave the game byte-for-byte unchanged,
    /// instead of silently committing a mid-action state that later surfaces as
    /// an opaque "illegal space" refusal.
    #[test]
    fn apply_rejects_a_card_interrupt_without_changing_state() {
        use crate::cards::{CardDefinition, CardEffect, CardKind, CardSet};
        use crate::engine::Action;
        use crate::model::Space;

        let probe = CardDefinition {
            use_limit: None,
            id: "PROBE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::PendingProbe {
                options: vec![(Good::Food, 2)],
            }],
        };
        let set = CardSet {
            definitions: vec![probe],
        };

        let mut game = Game::new(Config {
            players: 2,
            seed: 51,
            additional_action_space_enabled: false,
        });
        game.card_set = std::sync::Arc::new(set.clone());
        game.card_use_limits = std::collections::BTreeMap::new();
        game.cards = crate::cards::CardState {
            hands: vec![vec![], vec![]],
            undealt: vec![],
        };
        let active = game.public.active_player;
        game.public.players[active]
            .played_cards
            .push("PROBE".into());

        let hash_before = game.state_hash();
        let food_before = game.public.players[active].supply.food;
        let wood_before = game.public.players[active].supply.wood;
        let used_before = game.public.used.clone();
        let workers_before = game.public.workers_placed.clone();

        let action = Action {
            space: Space::Forest,
            detail: crate::engine::Detail::None,
        };
        let err = game.apply(action).unwrap_err();
        assert!(
            err.contains("cannot represent an"),
            "apply must refuse with a precise Err, got: {err}"
        );
        // No state change at all: no pending, no hash drift, no resources granted.
        assert!(!game.has_pending(), "no mid-action pause may be committed");
        assert_eq!(game.state_hash(), hash_before, "state must be unchanged");
        assert_eq!(
            game.public.players[active].supply.food, food_before,
            "food must be unchanged"
        );
        assert_eq!(
            game.public.players[active].supply.wood, wood_before,
            "wood must be unchanged"
        );
        assert_eq!(game.public.used, used_before, "no space may be marked used");
        assert_eq!(
            game.public.workers_placed, workers_before,
            "no worker may be placed"
        );
    }

    /// R2-P1-1 regression: a pending work resume must re-check that the frozen
    /// action's space is still revealed/available. A stale or tampered save must
    /// not smuggle a space that is not on the board into `used`.
    #[test]
    fn resume_rejects_a_space_that_is_not_available_at_resume_time() {
        use crate::engine::Action;
        use crate::model::Space;

        let mut game = Game::new(Config {
            players: 2,
            seed: 61,
            additional_action_space_enabled: false,
        });
        // `Copse` is an additional space and additional spaces are OFF, so it is not
        // on this board.
        let bogus = Space::Copse;
        assert!(
            !game.available_spaces_for_resume().contains(&bogus),
            "the probe space must genuinely be unavailable"
        );

        game.pending = Some(PendingDecision {
            id: 0,
            player: 0,
            kind: PendingKind::ChooseGain {
                source_card: "PROBE".into(),
                options: vec![GainOption {
                    good: Good::Food,
                    amount: 1,
                }],
            },
            state_hash_before: game.state_hash(),
            context: serde_json::json!({}),
            continuation: Continuation::PostWork {
                player: 0,
                resume: PostWorkResume {
                    step: ChainStep::AfterWorker,
                    phase: crate::card_events::TriggerPhase::Interrupt,
                    after: ChainPosition {
                        card_index: 0,
                        effect_index: 0,
                    },
                },
                completion: Box::new(WorkCompletion {
                    action: Action {
                        space: bogus,
                        detail: crate::engine::Detail::None,
                    },
                    previous_player: game.public.players[0].clone(),
                    harvest_plans: None,
                    breeding_plans: None,
                    logged_before: String::new(),
                }),
            },
        });
        let food_before = game.public.players[0].supply.food;
        let used_before = game.public.used.clone();
        let workers_before = game.public.workers_placed.clone();

        let err = game
            .resolve_pending_and_resume(PendingResolution::ChooseGain {
                chosen: GainOption {
                    good: Good::Food,
                    amount: 1,
                },
            })
            .unwrap_err();
        assert!(
            err.contains("not available at resume time"),
            "the resume must refuse an unavailable space, got: {err}"
        );
        // Refused atomically: nothing applied, decision preserved.
        assert_eq!(game.public.used, used_before, "no space may be marked used");
        assert_eq!(
            game.public.workers_placed, workers_before,
            "no worker may be placed"
        );
        assert_eq!(
            game.public.players[0].supply.food, food_before,
            "the choice must not be applied"
        );
        assert!(game.has_pending(), "the decision must stay resolvable");
    }
}

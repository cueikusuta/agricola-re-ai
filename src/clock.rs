//! Match clock state machine for per-player time measurement (Issue #217).
//!
//! This module is **measurement only**: it tracks how much thinking time each
//! competitor has left under an initial-time-plus-increment scheme, with an
//! optional bank cap and an optional per-decision cap. It deliberately does
//! *not* decide anything else:
//!
//! * The speed values (initial/increment/caps) are all supplied by the caller.
//!   No external service's preset (BGA or otherwise) is hard-coded here.
//! * Time-out **handling** (who wins, what the game does) is out of scope.
//!   Reaching zero only sets an "expired" flag that callers may read.
//! * Wiring into the engine, web UI, or session persistence is out of scope;
//!   the module is exposed as a plain `pub mod` and nothing else.
//!
//! All operations take the current time as a parameter instead of reading a
//! wall clock, so tests can drive the state machine with fake times and the
//! module stays deterministic.
//!
//! Error and idempotency policy:
//!
//! * A time input that goes backwards (earlier than the last accepted time for
//!   that clock) is an error and leaves the state unchanged.
//! * Re-`start_decision` with the same decision token never adds a second
//!   increment; the increment is applied at most once per token.
//! * `commit_decision` for an unknown or already-committed token is an
//!   idempotent no-op (it does not charge time twice), not an error.
//! * `pause` while stopped and `resume` while running are no-ops.
//!
//! Invariant conditions (INV):
//! - INV-1: `accepted_up_to_ms` never decreases.
//! - INV-2: For any token, increment is applied at most once (tracked in `incremented_tokens`).
//! - INV-3: For any token, time is charged at most once (tracked in `committed_tokens`).
//! - INV-4: `remaining_ms` is `None` iff `mode == ClockMode::Unlimited`; otherwise `Some(u64)`.
//! - INV-5: `expired` is true iff `mode == ClockMode::Timed` and `remaining_ms == Some(0)`.
//! - INV-6: `current_token.is_some()` iff `phase != DecisionPhase::Idle`.
//! - INV-7: If `phase == DecisionPhase::Running { started_at_ms }`, then `started_at_ms <= accepted_up_to_ms`.
//! - INV-8: `incremented_tokens` and `committed_tokens` are disjoint for any given token until commit.
//! - INV-9: All time arithmetic uses saturating operations; no panics or wraps in release.

/// Clock mode for the whole match. [`ClockMode::Unlimited`] matches the
/// current behaviour of the game: no time is ever charged or expired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockMode {
    /// No time measurement at all; every operation is a harmless no-op.
    Unlimited,
    /// Initial time plus per-decision increment, with optional caps.
    Timed,
}

/// Configuration for a match clock. Every duration is supplied by the caller;
/// this module embeds no preset values from any external service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchClockConfig {
    pub mode: ClockMode,
    /// Remaining time each competitor starts with, in milliseconds.
    /// Ignored in [`ClockMode::Unlimited`].
    pub initial_ms: u64,
    /// Time added once per new decision token, in milliseconds.
    /// Ignored in [`ClockMode::Unlimited`].
    pub increment_ms: u64,
    /// Upper bound on the banked remaining time after an increment.
    /// `None` means unlimited banking. Ignored in [`ClockMode::Unlimited`].
    pub max_bank_ms: Option<u64>,
    /// Upper bound charged for a single decision, however long it ran.
    /// `None` means a decision is charged for its whole duration.
    /// Ignored in [`ClockMode::Unlimited`].
    pub per_decision_cap_ms: Option<u64>,
}

impl MatchClockConfig {
    /// An unlimited clock: nothing is ever measured or charged.
    pub fn unlimited() -> Self {
        Self {
            mode: ClockMode::Unlimited,
            initial_ms: 0,
            increment_ms: 0,
            max_bank_ms: None,
            per_decision_cap_ms: None,
        }
    }
}

/// Outcome of a successful `start_decision` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartOutcome {
    /// A new decision was started; increment applied, consumption begun.
    Started,
    /// A previously paused decision was resumed; no new increment.
    Resumed,
    /// The decision was already running; no state change.
    AlreadyRunning,
    /// The decision was already committed; no-op, no state change.
    AlreadyCommitted,
}

/// Errors returned by clock operations. Every error leaves the clock state
/// unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockError {
    /// A time input is earlier than the last time this clock accepted.
    TimeReversed {
        now_ms: u128,
        accepted_up_to_ms: u128,
    },
    /// A different decision is already in progress on this clock.
    DecisionInProgress { open_token: u64 },
    /// The seat index is out of bounds.
    InvalidSeat { seat: usize, seats: usize },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClockError::TimeReversed {
                now_ms,
                accepted_up_to_ms,
            } => {
                write!(f, "clock time input {now_ms} ms went backwards; the last accepted time was {accepted_up_to_ms} ms")
            }
            ClockError::DecisionInProgress { open_token } => {
                write!(
                    f,
                    "decision {open_token} is still in progress; commit or pause it first"
                )
            }
            ClockError::InvalidSeat { seat, seats } => {
                write!(f, "invalid seat {seat}; valid range is 0..{seats}")
            }
        }
    }
}

impl std::error::Error for ClockError {}

/// Lifecycle of one decision token on a single clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecisionPhase {
    /// No decision in progress (or the last one was committed).
    Idle,
    /// The decision is consuming time since `started_at_ms`.
    Running { started_at_ms: u128, token: u64 },
    /// Consumption is suspended; elapsed time is parked in `current_elapsed_ms`.
    Paused { token: u64 },
}

/// The clock of a single competitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerClock {
    /// Remaining banked time, or `None` under [`ClockMode::Unlimited`].
    remaining_ms: Option<u64>,
    phase: DecisionPhase,
    /// Total elapsed time already accumulated for the current token, in ms.
    /// This is charged at `commit_decision`, capped by
    /// [`MatchClockConfig::per_decision_cap_ms`].
    current_elapsed_ms: u64,
    /// Tokens whose increment has already been applied, so a repeated
    /// `start_decision` cannot add a second increment.
    incremented_tokens: Vec<u64>,
    /// Tokens that have been committed (charged). A re-start of a committed
    /// token is a no-op (AlreadyCommitted).
    committed_tokens: Vec<u64>,
    /// Last time accepted by any operation, for rewind detection.
    accepted_up_to_ms: u128,
    /// Set once the remaining time reaches 0. Game-side consequences are the
    /// caller's concern; this module only records the flag.
    expired: bool,
}

impl PlayerClock {
    /// A fresh clock for one competitor with `initial_ms` of time (Timed) or
    /// no time accounting at all (Unlimited).
    pub fn new(config: &MatchClockConfig) -> Self {
        Self {
            remaining_ms: match config.mode {
                ClockMode::Unlimited => None,
                ClockMode::Timed => Some(config.initial_ms),
            },
            phase: DecisionPhase::Idle,
            current_elapsed_ms: 0,
            incremented_tokens: Vec::with_capacity(8),
            committed_tokens: Vec::with_capacity(8),
            accepted_up_to_ms: 0,
            expired: false,
        }
    }

    /// Remaining time in ms; `None` means unlimited.
    pub fn remaining_ms(&self) -> Option<u64> {
        self.remaining_ms
    }

    /// True once the remaining time has reached 0 (Timed only).
    pub fn expired(&self) -> bool {
        self.expired
    }

    /// True while a decision is started (running or paused).
    pub fn decision_in_progress(&self) -> bool {
        !matches!(self.phase, DecisionPhase::Idle)
    }

    /// Token of the decision in progress, if any.
    pub fn current_token(&self) -> Option<u64> {
        match self.phase {
            DecisionPhase::Running { token, .. } | DecisionPhase::Paused { token } => Some(token),
            DecisionPhase::Idle => None,
        }
    }

    /// True while the running decision is paused (not consuming time).
    pub fn paused(&self) -> bool {
        matches!(self.phase, DecisionPhase::Paused { .. })
    }

    /// Whether `token` has already had its increment applied.
    pub fn increment_applied(&self, token: u64) -> bool {
        self.incremented_tokens.contains(&token)
    }

    /// Whether `token` has already been committed (charged).
    pub fn committed(&self, token: u64) -> bool {
        self.committed_tokens.contains(&token)
    }

    /// Reject times that go backwards, leaving the state untouched.
    fn check_time(&self, now_ms: u128) -> Result<(), ClockError> {
        if now_ms < self.accepted_up_to_ms {
            return Err(ClockError::TimeReversed {
                now_ms,
                accepted_up_to_ms: self.accepted_up_to_ms,
            });
        }
        Ok(())
    }

    /// Internal: compute the next state for `start_decision` without mutating.
    /// Returns the planned outcome and the new phase/elapsed/increment state.
    fn plan_start_decision(
        &self,
        config: &MatchClockConfig,
        now_ms: u128,
        token: u64,
    ) -> Result<StartPlan, ClockError> {
        self.check_time(now_ms)?;

        if config.mode == ClockMode::Unlimited {
            // In Unlimited mode, no increments are ever applied, no time charged.
            return Ok(StartPlan {
                outcome: if self.current_token() == Some(token) {
                    if matches!(self.phase, DecisionPhase::Running { .. }) {
                        StartOutcome::AlreadyRunning
                    } else {
                        StartOutcome::Resumed
                    }
                } else {
                    StartOutcome::Started
                },
                new_phase: match self.phase {
                    DecisionPhase::Running { .. } if self.current_token() == Some(token) => {
                        self.phase
                    }
                    DecisionPhase::Paused { .. } if self.current_token() == Some(token) => {
                        DecisionPhase::Running {
                            started_at_ms: now_ms,
                            token,
                        }
                    }
                    _ => DecisionPhase::Running {
                        started_at_ms: now_ms,
                        token,
                    },
                },
                new_current_elapsed_ms: if self.current_token() == Some(token) {
                    self.current_elapsed_ms
                } else {
                    0
                },
                apply_increment: false, // Never apply increment in Unlimited mode
                new_accepted_up_to_ms: now_ms,
            });
        }

        // Timed mode
        match self.phase {
            DecisionPhase::Running { token: t, .. } if t == token => {
                // Already running: no-op
                Ok(StartPlan {
                    outcome: StartOutcome::AlreadyRunning,
                    new_phase: self.phase,
                    new_current_elapsed_ms: self.current_elapsed_ms,
                    apply_increment: false,
                    new_accepted_up_to_ms: now_ms,
                })
            }
            DecisionPhase::Paused { token: t } if t == token => {
                // Resume paused
                Ok(StartPlan {
                    outcome: StartOutcome::Resumed,
                    new_phase: DecisionPhase::Running {
                        started_at_ms: now_ms,
                        token,
                    },
                    new_current_elapsed_ms: self.current_elapsed_ms,
                    apply_increment: false,
                    new_accepted_up_to_ms: now_ms,
                })
            }
            DecisionPhase::Idle => {
                if self.committed(token) {
                    // Already committed: no-op
                    Ok(StartPlan {
                        outcome: StartOutcome::AlreadyCommitted,
                        new_phase: DecisionPhase::Idle,
                        new_current_elapsed_ms: 0,
                        apply_increment: false,
                        new_accepted_up_to_ms: now_ms,
                    })
                } else if self.current_token().is_some() {
                    // Different token still open
                    Err(ClockError::DecisionInProgress {
                        open_token: self.current_token().unwrap(),
                    })
                } else {
                    // Start new decision
                    Ok(StartPlan {
                        outcome: StartOutcome::Started,
                        new_phase: DecisionPhase::Running {
                            started_at_ms: now_ms,
                            token,
                        },
                        new_current_elapsed_ms: 0,
                        apply_increment: !self.increment_applied(token),
                        new_accepted_up_to_ms: now_ms,
                    })
                }
            }
            DecisionPhase::Running { token: t, .. } | DecisionPhase::Paused { token: t } => {
                // Different token open
                Err(ClockError::DecisionInProgress { open_token: t })
            }
        }
    }

    /// Apply a previously computed start plan.
    fn apply_start_plan(&mut self, config: &MatchClockConfig, plan: StartPlan, token: u64) {
        self.phase = plan.new_phase;
        self.current_elapsed_ms = plan.new_current_elapsed_ms;
        self.accepted_up_to_ms = plan.new_accepted_up_to_ms;
        if plan.apply_increment {
            let _ = self.apply_increment(config, token);
        }
        // If we're starting a committed token, ensure it's not treated as running
        if plan.outcome == StartOutcome::AlreadyCommitted {
            self.phase = DecisionPhase::Idle;
            self.current_elapsed_ms = 0;
        }
    }

    /// Start (or re-start) the decision identified by `token`.
    ///
    /// * The first `start_decision` for a token applies the per-decision
    ///   increment once, capped by `max_bank_ms`, and starts consumption.
    /// * A repeat with the same token adds no second increment; if the clock
    ///   was paused, it resumes consumption instead.
    /// * A re-start of a committed token returns `AlreadyCommitted` and is a no-op.
    /// * In [`ClockMode::Unlimited`] nothing is measured or charged.
    pub fn start_decision(
        &mut self,
        config: &MatchClockConfig,
        now_ms: u128,
        token: u64,
    ) -> Result<StartOutcome, ClockError> {
        let plan = self.plan_start_decision(config, now_ms, token)?;
        let outcome = plan.outcome;
        self.apply_start_plan(config, plan, token);
        Ok(outcome)
    }

    /// Apply the per-decision increment at most once per token, capped by
    /// `max_bank_ms`. Returns the new remaining time.
    fn apply_increment(&mut self, config: &MatchClockConfig, token: u64) -> u64 {
        if self.incremented_tokens.contains(&token) {
            return self.remaining_ms.unwrap_or(0);
        }
        self.incremented_tokens.push(token);
        let remaining = self
            .remaining_ms
            .unwrap_or(0)
            .saturating_add(config.increment_ms);
        let capped = match config.max_bank_ms {
            Some(cap) => remaining.min(cap),
            None => remaining,
        };
        self.remaining_ms = Some(capped);
        capped
    }

    /// Internal: compute the next state for `commit_decision` without mutating.
    fn plan_commit_decision(
        &self,
        config: &MatchClockConfig,
        now_ms: u128,
        token: u64,
    ) -> Result<CommitPlan, ClockError> {
        self.check_time(now_ms)?;

        if config.mode == ClockMode::Unlimited {
            return Ok(CommitPlan {
                new_phase: DecisionPhase::Idle,
                new_current_elapsed_ms: 0,
                new_current_token: None,
                charged: 0,
                new_remaining: self.remaining_ms.unwrap_or(0),
                new_expired: self.expired,
                new_accepted_up_to_ms: now_ms,
                mark_committed: false,
            });
        }

        match self.phase {
            DecisionPhase::Running {
                token: t,
                started_at_ms,
            } if t == token => {
                let until = now_ms.max(started_at_ms);
                let elapsed_this_run = (until - started_at_ms) as u64;
                let total_elapsed = self.current_elapsed_ms.saturating_add(elapsed_this_run);
                let charged = match config.per_decision_cap_ms {
                    Some(cap) => total_elapsed.min(cap),
                    None => total_elapsed,
                };
                let remaining = self.remaining_ms.unwrap_or(0);
                let new_remaining = remaining.saturating_sub(charged);
                let new_expired = new_remaining == 0;
                Ok(CommitPlan {
                    new_phase: DecisionPhase::Idle,
                    new_current_elapsed_ms: 0,
                    new_current_token: None,
                    charged,
                    new_remaining,
                    new_expired,
                    new_accepted_up_to_ms: now_ms,
                    mark_committed: !self.committed(token),
                })
            }
            DecisionPhase::Paused { token: t } if t == token => {
                // Commit a paused decision: charge the parked elapsed time
                let total_elapsed = self.current_elapsed_ms;
                let charged = match config.per_decision_cap_ms {
                    Some(cap) => total_elapsed.min(cap),
                    None => total_elapsed,
                };
                let remaining = self.remaining_ms.unwrap_or(0);
                let new_remaining = remaining.saturating_sub(charged);
                let new_expired = new_remaining == 0;
                Ok(CommitPlan {
                    new_phase: DecisionPhase::Idle,
                    new_current_elapsed_ms: 0,
                    new_current_token: None,
                    charged,
                    new_remaining,
                    new_expired,
                    new_accepted_up_to_ms: now_ms,
                    mark_committed: !self.committed(token),
                })
            }
            DecisionPhase::Idle | DecisionPhase::Running { .. } | DecisionPhase::Paused { .. } => {
                // Unknown or already-committed token: idempotent no-op
                Ok(CommitPlan {
                    new_phase: self.phase,
                    new_current_elapsed_ms: self.current_elapsed_ms,
                    new_current_token: self.current_token(),
                    charged: 0,
                    new_remaining: self.remaining_ms.unwrap_or(0),
                    new_expired: self.expired,
                    new_accepted_up_to_ms: now_ms,
                    mark_committed: false,
                })
            }
        }
    }

    /// Apply a previously computed commit plan.
    fn apply_commit_plan(&mut self, plan: CommitPlan, token: u64) {
        self.phase = plan.new_phase;
        self.current_elapsed_ms = plan.new_current_elapsed_ms;
        self.accepted_up_to_ms = plan.new_accepted_up_to_ms;
        if plan.charged > 0 {
            self.remaining_ms = Some(plan.new_remaining);
            self.expired = plan.new_expired;
        }
        if plan.mark_committed {
            self.committed_tokens.push(token);
        }
    }

    /// Commit the decision identified by `token`.
    ///
    /// The accumulated elapsed time (capped by `per_decision_cap_ms`) is
    /// charged against the remaining time, which is clamped at 0. Reaching 0
    /// sets the expired flag; no game-side behaviour is triggered here.
    ///
    /// Committing an unknown or already-committed token is an idempotent
    /// no-op: the state does not change and no time is charged twice.
    pub fn commit_decision(
        &mut self,
        config: &MatchClockConfig,
        now_ms: u128,
        token: u64,
    ) -> Result<(), ClockError> {
        let plan = self.plan_commit_decision(config, now_ms, token)?;
        self.apply_commit_plan(plan, token);
        Ok(())
    }

    /// Stop consuming time for the running decision. Pausing while no
    /// decision runs, or while already paused, is a no-op.
    pub fn pause(&mut self, config: &MatchClockConfig, now_ms: u128) -> Result<(), ClockError> {
        self.check_time(now_ms)?;
        if config.mode == ClockMode::Unlimited {
            self.accepted_up_to_ms = now_ms;
            return Ok(());
        }
        if let DecisionPhase::Running {
            started_at_ms,
            token,
        } = self.phase
        {
            let until = now_ms.max(started_at_ms);
            self.current_elapsed_ms = self
                .current_elapsed_ms
                .saturating_add((until - started_at_ms) as u64);
            self.phase = DecisionPhase::Paused { token };
        }
        self.accepted_up_to_ms = now_ms;
        Ok(())
    }

    /// Resume consumption of the paused decision. Resuming while running or
    /// idle is a no-op.
    pub fn resume(&mut self, config: &MatchClockConfig, now_ms: u128) -> Result<(), ClockError> {
        self.check_time(now_ms)?;
        if config.mode == ClockMode::Unlimited {
            self.accepted_up_to_ms = now_ms;
            return Ok(());
        }
        if let DecisionPhase::Paused { token } = self.phase {
            self.phase = DecisionPhase::Running {
                started_at_ms: now_ms,
                token,
            };
        }
        self.accepted_up_to_ms = now_ms;
        Ok(())
    }
}

/// Internal plan for start_decision (check → compute → apply separation).
struct StartPlan {
    outcome: StartOutcome,
    new_phase: DecisionPhase,
    new_current_elapsed_ms: u64,
    apply_increment: bool,
    new_accepted_up_to_ms: u128,
}

/// Internal plan for commit_decision (check → compute → apply separation).
#[allow(dead_code)]
struct CommitPlan {
    new_phase: DecisionPhase,
    new_current_elapsed_ms: u64,
    new_current_token: Option<u64>,
    charged: u64,
    new_remaining: u64,
    new_expired: bool,
    new_accepted_up_to_ms: u128,
    mark_committed: bool,
}

/// The clocks of all competitors in a match, addressed by seat index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchClock {
    config: MatchClockConfig,
    players: Vec<PlayerClock>,
}

impl MatchClock {
    /// One clock per seat, all configured identically from `config`.
    pub fn new(config: MatchClockConfig, seats: usize) -> Self {
        Self {
            players: (0..seats).map(|_| PlayerClock::new(&config)).collect(),
            config,
        }
    }

    pub fn config(&self) -> &MatchClockConfig {
        &self.config
    }

    pub fn player(&self, seat: usize) -> Option<&PlayerClock> {
        self.players.get(seat)
    }

    /// Mutable access for callers that need to inspect internals in tests;
    /// day-to-day operations should go through the seat-addressed methods.
    pub fn player_mut(&mut self, seat: usize) -> Option<&mut PlayerClock> {
        self.players.get_mut(seat)
    }

    pub fn seats(&self) -> usize {
        self.players.len()
    }

    pub fn start_decision(
        &mut self,
        seat: usize,
        now_ms: u128,
        token: u64,
    ) -> Result<StartOutcome, ClockError> {
        let config = self.config;
        let seats = self.seats();
        self.player_mut(seat)
            .ok_or(ClockError::InvalidSeat { seat, seats })?
            .start_decision(&config, now_ms, token)
    }

    pub fn commit_decision(
        &mut self,
        seat: usize,
        now_ms: u128,
        token: u64,
    ) -> Result<(), ClockError> {
        let config = self.config;
        let seats = self.seats();
        self.player_mut(seat)
            .ok_or(ClockError::InvalidSeat { seat, seats })?
            .commit_decision(&config, now_ms, token)
    }

    pub fn pause(&mut self, seat: usize, now_ms: u128) -> Result<(), ClockError> {
        let config = self.config;
        let seats = self.seats();
        self.player_mut(seat)
            .ok_or(ClockError::InvalidSeat { seat, seats })?
            .pause(&config, now_ms)
    }

    pub fn resume(&mut self, seat: usize, now_ms: u128) -> Result<(), ClockError> {
        let config = self.config;
        let seats = self.seats();
        self.player_mut(seat)
            .ok_or(ClockError::InvalidSeat { seat, seats })?
            .resume(&config, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 60 s initial, 5 s increment, bank capped at 90 s, decision capped at 30 s.
    fn timed_config() -> MatchClockConfig {
        MatchClockConfig {
            mode: ClockMode::Timed,
            initial_ms: 60_000,
            increment_ms: 5_000,
            max_bank_ms: Some(90_000),
            per_decision_cap_ms: Some(30_000),
        }
    }

    fn uncapped_config() -> MatchClockConfig {
        MatchClockConfig {
            mode: ClockMode::Timed,
            initial_ms: 60_000,
            increment_ms: 5_000,
            max_bank_ms: None,
            per_decision_cap_ms: None,
        }
    }

    #[test]
    fn consecutive_turns_charge_and_increment() {
        let cfg = timed_config();
        let mut clock = MatchClock::new(cfg, 2);
        // Seat 0, decision 1: 10 s of thinking.
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 11_000, 1).unwrap();
        // 60_000 + 5_000 increment - 10_000 elapsed = 55_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(55_000));
        // Seat 1 in the meantime is untouched.
        assert_eq!(clock.player(1).unwrap().remaining_ms(), Some(60_000));
        // Seat 0, decision 2: 3 s.
        assert_eq!(
            clock.start_decision(0, 11_000, 2).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 14_000, 2).unwrap();
        // 55_000 + 5_000 - 3_000 = 57_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(57_000));
        assert!(!clock.player(0).unwrap().expired());
    }

    #[test]
    fn double_start_does_not_increment_twice() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 7).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 4_000, 7).unwrap();
        // 60_000 + 5_000 - 3_000 = 62_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(62_000));
        // Restarting the same committed token: no second increment, no-op.
        assert_eq!(
            clock.start_decision(0, 4_000, 7).unwrap(),
            StartOutcome::AlreadyCommitted
        );
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(62_000));
        assert!(clock.player(0).unwrap().increment_applied(7));
        // Commit again: no-op (already committed)
        clock.commit_decision(0, 5_000, 7).unwrap();
        // No additional charge because token was already committed.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(62_000));
    }

    #[test]
    fn double_commit_does_not_charge_twice() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 7).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 6_000, 7).unwrap();
        let after_first = clock.player(0).unwrap().remaining_ms();
        assert_eq!(after_first, Some(60_000)); // (60_000 + 5_000) - (6_000 - 1_000)
                                               // Second commit with the same token and even a later time: no change.
        clock.commit_decision(0, 9_000, 7).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), after_first);
        // Commit for a token that never started: also a no-op.
        clock.commit_decision(0, 9_000, 99).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), after_first);
    }

    #[test]
    fn increment_is_banked_up_to_max() {
        let cfg = timed_config(); // initial 60_000, increment 5_000, bank cap 90_000
        let mut clock = MatchClock::new(cfg, 1);
        // Zero-length decisions, so only the increments change the bank:
        // 60+5, 65+5, ... up to the 90 s cap.
        let mut now = 1_000u128;
        for token in 1..=7u64 {
            assert_eq!(
                clock.start_decision(0, now, token).unwrap(),
                StartOutcome::Started
            );
            clock.commit_decision(0, now, token).unwrap();
            now += 1_000;
        }
        // The bank must be pinned at the 90_000 cap and never exceed it.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(90_000));
        assert_eq!(
            clock.start_decision(0, now, 8).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, now, 8).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(90_000));
    }

    #[test]
    fn per_decision_cap_limits_a_single_charge() {
        let cfg = timed_config(); // decision cap 30 s
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        // A 5-minute "stall" (tab hidden, dispute, ...) is charged only 30 s.
        clock.commit_decision(0, 301_000, 1).unwrap();
        // 60_000 + 5_000 - 30_000 = 35_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(35_000));
    }

    #[test]
    fn running_out_clamps_at_zero_and_sets_expired() {
        let mut cfg = timed_config();
        cfg.per_decision_cap_ms = None; // charge the full overrun
        let mut clock = MatchClock::new(cfg, 1);
        // Each round burns a full minute against a 1-minute bank plus a 5 s
        // increment: 65_000 - 60_000 = 5_000, then 10_000 - 60_000 -> 0.
        let mut now = 1_000u128;
        loop {
            let token = (now / 1_000) as u64;
            assert_eq!(
                clock.start_decision(0, now, token).unwrap(),
                StartOutcome::Started
            );
            clock.commit_decision(0, now + 60_000, token).unwrap();
            now += 61_000;
            if clock.player(0).unwrap().remaining_ms() == Some(0) {
                break;
            }
            assert!(
                !clock.player(0).unwrap().expired(),
                "expired flag must wait for exactly 0 remaining"
            );
        }
        assert!(clock.player(0).unwrap().expired());
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(0));
        // Further time does not push remaining negative.
        assert_eq!(
            clock.start_decision(0, now, 999).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, now + 70_000, 999).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(0));
    }

    #[test]
    fn unlimited_clock_is_untouched() {
        let cfg = MatchClockConfig::unlimited();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.pause(0, 2_000).unwrap();
        clock.resume(0, 3_000).unwrap();
        clock.commit_decision(0, 60_000, 1).unwrap();
        let seat = clock.player(0).unwrap();
        assert_eq!(seat.remaining_ms(), None);
        assert!(!seat.expired());
    }

    #[test]
    fn pause_stops_consumption() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        // The 5 s before the pause are parked, not lost: they are settled at
        // commit together with the time after the resume.
        clock.pause(0, 6_000).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(65_000));
        clock.resume(0, 60_000).unwrap();
        // The 54 s pause is not charged.
        clock.commit_decision(0, 61_000, 1).unwrap();
        // 60_000 + 5_000 - (5_000 + 1_000) = 59_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(59_000));
        // Pause with nothing running is a harmless no-op.
        clock.pause(0, 62_000).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(59_000));
    }

    #[test]
    fn reversed_time_is_rejected_without_state_change() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 10_000, 1).unwrap(),
            StartOutcome::Started
        );
        let before = clock.player(0).unwrap().clone();
        // Earlier than the start time.
        assert!(matches!(
            clock.commit_decision(0, 9_999, 1),
            Err(ClockError::TimeReversed { .. })
        ));
        assert_eq!(&before, clock.player(0).unwrap());
        assert!(matches!(
            clock.pause(0, 9_000),
            Err(ClockError::TimeReversed { .. })
        ));
        // Later operations with valid times still work.
        clock.commit_decision(0, 12_000, 1).unwrap();
        // 60_000 + 5_000 - 2_000 = 63_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(63_000));
    }

    #[test]
    fn second_decision_cannot_start_while_first_is_open() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        assert!(matches!(
            clock.start_decision(0, 2_000, 2),
            Err(ClockError::DecisionInProgress { .. })
        ));
        // The first decision is still the one running and commits cleanly.
        clock.commit_decision(0, 3_000, 1).unwrap();
        // 60_000 + 5_000 - 2_000 = 63_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(63_000));
    }

    #[test]
    fn seats_are_independent() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 3);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 20_000, 1).unwrap();
        // 60_000 + 5_000 - 19_000 = 46_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(46_000));
        for seat in 1..3 {
            assert_eq!(clock.player(seat).unwrap().remaining_ms(), Some(60_000));
            assert!(!clock.player(seat).unwrap().expired());
        }
    }

    // --- Reproduction tests for contract violations (pre-fix) ---
    // These tests are expected to FAIL before the fix and PASS after.

    /// Contract violation (1): different-token start advances accepted_up_to_ms
    /// before returning Err, causing a subsequent commit at a valid intermediate
    /// time to be incorrectly rejected.
    #[test]
    fn rejected_start_advances_accepted_time_causing_commit_rejection() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        // accepted_up_to_ms should be 1000 now.
        // Try to start token 2 at 2000 - this fails but incorrectly advances
        // accepted_up_to_ms to 2000 (bug).
        let err = clock.start_decision(0, 2_000, 2);
        assert!(matches!(err, Err(ClockError::DecisionInProgress { .. })));
        // Now try to commit token 1 at 1500 (valid: 1500 > 1000, but < 2000).
        // With the bug, this fails because accepted_up_to_ms was advanced to 2000.
        let commit_result = clock.commit_decision(0, 1_500, 1);
        // This SHOULD succeed (1500 > 1000, the time when token 1 started)
        // but with the bug it fails with TimeReversed { accepted_up_to_ms: 2000 }
        assert!(
            commit_result.is_ok(),
            "commit at 1500 should succeed (original accepted_up_to_ms was 1000), \
             but bug incorrectly advanced it to 2000 causing rejection"
        );
    }

    /// Contract violation (2): committed token restart causes double charging.
    #[test]
    fn committed_token_restart_causes_double_charge() {
        let cfg = uncapped_config(); // initial 60000, increment 5000, no caps
        let mut clock = MatchClock::new(cfg, 1);
        // Start and commit token 1: 1000ms elapsed
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 2_000, 1).unwrap();
        // Remaining: 60000 + 5000 - 1000 = 64000
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(64_000));
        // Now RESTART the committed token 1
        assert_eq!(
            clock.start_decision(0, 3_000, 1).unwrap(),
            StartOutcome::AlreadyCommitted
        );
        // Remaining should still be 64000 (no second increment)
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(64_000));
        // Commit again with 1000ms more elapsed
        clock.commit_decision(0, 4_000, 1).unwrap();
        // BUG: remaining becomes 63000 (charged another 1000)
        // CORRECT: remaining should stay 64000 (no-op commit for already-committed token)
        // The bug report says: "残り 900 → 300" (remaining 900 -> 300)
        // which means 600ms was charged twice.
        assert_eq!(
            clock.player(0).unwrap().remaining_ms(),
            Some(64_000),
            "re-committing an already-committed token should be a no-op, \
             not charge time again (double-charge bug)"
        );
    }

    // --- Additional regression tests for the fix ---

    #[test]
    fn rejected_start_err_for_different_token_uses_decision_in_progress() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        let err = clock.start_decision(0, 2_000, 2);
        assert!(matches!(
            err,
            Err(ClockError::DecisionInProgress { open_token: 1 })
        ));
    }

    #[test]
    fn committed_token_restart_is_noop_and_does_not_advance_time() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 2_000, 1).unwrap();
        // Restart committed token
        assert_eq!(
            clock.start_decision(0, 3_000, 1).unwrap(),
            StartOutcome::AlreadyCommitted
        );
        // accepted_up_to_ms should advance to 3000 (time check passes)
        // but state should be unchanged (no increment, no phase change)
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(64_000));
        assert!(!clock.player(0).unwrap().decision_in_progress());
        // Subsequent commit at 4000 should be no-op
        clock.commit_decision(0, 4_000, 1).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(64_000));
    }

    #[test]
    fn no_op_start_does_not_apply_second_increment() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.commit_decision(0, 2_000, 1).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(64_000));
        // Restart committed token multiple times
        assert_eq!(
            clock.start_decision(0, 3_000, 1).unwrap(),
            StartOutcome::AlreadyCommitted
        );
        assert_eq!(
            clock.start_decision(0, 4_000, 1).unwrap(),
            StartOutcome::AlreadyCommitted
        );
        assert_eq!(
            clock.start_decision(0, 5_000, 1).unwrap(),
            StartOutcome::AlreadyCommitted
        );
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(64_000));
        assert_eq!(clock.player(0).unwrap().incremented_tokens.len(), 1);
    }

    #[test]
    fn overflow_in_increment_does_not_panic() {
        let cfg = MatchClockConfig {
            mode: ClockMode::Timed,
            initial_ms: u64::MAX - 100,
            increment_ms: 1000,
            max_bank_ms: None,
            per_decision_cap_ms: None,
        };
        let mut clock = MatchClock::new(cfg, 1);
        // This should not panic; saturating_add will clamp at u64::MAX
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(u64::MAX));
    }

    #[test]
    fn invalid_seat_returns_invalid_seat_error() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 2);
        let err = clock.start_decision(5, 1_000, 1);
        assert!(matches!(
            err,
            Err(ClockError::InvalidSeat { seat: 5, seats: 2 })
        ));
        let err = clock.commit_decision(5, 1_000, 1);
        assert!(matches!(
            err,
            Err(ClockError::InvalidSeat { seat: 5, seats: 2 })
        ));
        let err = clock.pause(5, 1_000);
        assert!(matches!(
            err,
            Err(ClockError::InvalidSeat { seat: 5, seats: 2 })
        ));
        let err = clock.resume(5, 1_000);
        assert!(matches!(
            err,
            Err(ClockError::InvalidSeat { seat: 5, seats: 2 })
        ));
    }

    #[test]
    fn time_reversed_includes_both_times() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 10_000, 1).unwrap(),
            StartOutcome::Started
        );
        let err = clock.commit_decision(0, 9_999, 1);
        assert!(matches!(
            err,
            Err(ClockError::TimeReversed {
                now_ms: 9_999,
                accepted_up_to_ms: 10_000
            })
        ));
    }

    #[test]
    fn pause_resume_idempotent() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.pause(0, 2_000).unwrap();
        // Pause again: no-op
        clock.pause(0, 3_000).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(65_000));
        clock.resume(0, 4_000).unwrap();
        // Resume again: no-op
        clock.resume(0, 5_000).unwrap();
        clock.commit_decision(0, 6_000, 1).unwrap();
        // 60_000 + 5_000 - (1_000 + 2_000) = 62_000
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(62_000));
    }

    #[test]
    fn start_outcome_resumed_for_paused_token() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        clock.pause(0, 2_000).unwrap();
        assert_eq!(
            clock.start_decision(0, 3_000, 1).unwrap(),
            StartOutcome::Resumed
        );
        assert!(clock.player(0).unwrap().decision_in_progress());
        assert!(!clock.player(0).unwrap().paused());
    }

    #[test]
    fn start_outcome_already_running_for_running_token() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        assert_eq!(
            clock.start_decision(0, 1_000, 1).unwrap(),
            StartOutcome::Started
        );
        assert_eq!(
            clock.start_decision(0, 2_000, 1).unwrap(),
            StartOutcome::AlreadyRunning
        );
        assert!(clock.player(0).unwrap().decision_in_progress());
        assert!(!clock.player(0).unwrap().paused());
    }
}

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

/// Errors returned by clock operations. Every error leaves the clock state
/// unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    /// A time input is earlier than the last time this clock accepted.
    TimeReversed { accepted_up_to_ms: u128 },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClockError::TimeReversed { accepted_up_to_ms } => {
                write!(f, "clock time input went backwards; the last accepted time was {accepted_up_to_ms} ms")
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
    Running { started_at_ms: u128 },
    /// Consumption is suspended; elapsed time is parked in `elapsed_ms`.
    Paused,
}

/// The clock of a single competitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerClock {
    /// Remaining banked time, or `None` under [`ClockMode::Unlimited`].
    remaining_ms: Option<u64>,
    phase: DecisionPhase,
    /// Token of the decision currently started (running or paused).
    current_token: Option<u64>,
    /// Total elapsed time already accumulated for `current_token`, in ms.
    /// This is charged at `commit_decision`, capped by
    /// [`MatchClockConfig::per_decision_cap_ms`].
    current_elapsed_ms: u64,
    /// Tokens whose increment has already been applied, so a repeated
    /// `start_decision` cannot add a second increment.
    incremented_tokens: Vec<u64>,
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
            current_token: None,
            current_elapsed_ms: 0,
            incremented_tokens: Vec::with_capacity(8),
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
        self.current_token.is_some()
    }

    /// Token of the decision in progress, if any.
    pub fn current_token(&self) -> Option<u64> {
        self.current_token
    }

    /// True while the running decision is paused (not consuming time).
    pub fn paused(&self) -> bool {
        self.phase == DecisionPhase::Paused
    }

    /// Whether `token` has already had its increment applied.
    pub fn increment_applied(&self, token: u64) -> bool {
        self.incremented_tokens.contains(&token)
    }

    /// Reject times that go backwards, leaving the state untouched.
    fn check_time(&self, now_ms: u128) -> Result<(), ClockError> {
        if now_ms < self.accepted_up_to_ms {
            return Err(ClockError::TimeReversed {
                accepted_up_to_ms: self.accepted_up_to_ms,
            });
        }
        Ok(())
    }

    /// Start (or re-start) the decision identified by `token`.
    ///
    /// * The first `start_decision` for a token applies the per-decision
    ///   increment once, capped by `max_bank_ms`, and starts consumption.
    /// * A repeat with the same token adds no second increment; if the clock
    ///   was paused, it resumes consumption instead.
    /// * In [`ClockMode::Unlimited`] nothing is measured or charged.
    pub fn start_decision(
        &mut self,
        config: &MatchClockConfig,
        now_ms: u128,
        token: u64,
    ) -> Result<(), ClockError> {
        self.check_time(now_ms)?;
        self.accepted_up_to_ms = now_ms;
        if config.mode == ClockMode::Unlimited {
            // Unlimited: nothing to measure, only record the token so the
            // commit stays idempotent.
            if self.current_token != Some(token) {
                self.current_token = Some(token);
                self.current_elapsed_ms = 0;
                self.phase = DecisionPhase::Idle;
            }
            return Ok(());
        }
        if self.current_token == Some(token) {
            // Same token again: never add a second increment. Restart
            // consumption if it was paused or previously committed.
            if !matches!(self.phase, DecisionPhase::Running { .. }) {
                self.phase = DecisionPhase::Running {
                    started_at_ms: now_ms,
                };
            }
            return Ok(());
        }
        if self.current_token.is_some() {
            // A different decision is still open. Refuse rather than lose the
            // unaccounted time of the open decision.
            return Err(ClockError::TimeReversed {
                accepted_up_to_ms: self.accepted_up_to_ms,
            });
        }
        let _ = self.apply_increment(config, token);
        self.current_token = Some(token);
        self.current_elapsed_ms = 0;
        self.phase = DecisionPhase::Running {
            started_at_ms: now_ms,
        };
        Ok(())
    }

    /// Apply the per-decision increment at most once per token, capped by
    /// `max_bank_ms`. Returns the new remaining time.
    fn apply_increment(&mut self, config: &MatchClockConfig, token: u64) -> u64 {
        if self.incremented_tokens.contains(&token) {
            return self.remaining_ms.unwrap_or(0);
        }
        self.incremented_tokens.push(token);
        let remaining = self.remaining_ms.unwrap_or(0) + config.increment_ms;
        let capped = match config.max_bank_ms {
            Some(cap) => remaining.min(cap),
            None => remaining,
        };
        self.remaining_ms = Some(capped);
        capped
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
        self.check_time(now_ms)?;
        if config.mode == ClockMode::Unlimited {
            // Unlimited: nothing charged; only close the token if it matches.
            if self.current_token == Some(token) {
                self.current_token = None;
                self.current_elapsed_ms = 0;
            }
            self.accepted_up_to_ms = now_ms;
            return Ok(());
        }
        if self.current_token != Some(token) {
            // Old or duplicated commit: idempotent no-op.
            return Ok(());
        }
        if let DecisionPhase::Running { started_at_ms } = self.phase {
            let until = now_ms.max(started_at_ms);
            self.current_elapsed_ms += (until - started_at_ms) as u64;
        }
        let charged = match config.per_decision_cap_ms {
            Some(cap) => self.current_elapsed_ms.min(cap),
            None => self.current_elapsed_ms,
        };
        let remaining = self.remaining_ms.unwrap_or(0);
        let new_remaining = remaining.saturating_sub(charged);
        if new_remaining == 0 {
            self.expired = true;
        }
        self.remaining_ms = Some(new_remaining);
        self.current_token = None;
        self.current_elapsed_ms = 0;
        self.phase = DecisionPhase::Idle;
        self.accepted_up_to_ms = now_ms;
        Ok(())
    }

    /// Stop consuming time for the running decision. Pausing while no
    /// decision runs, or while already paused, is a no-op.
    pub fn pause(&mut self, config: &MatchClockConfig, now_ms: u128) -> Result<(), ClockError> {
        self.check_time(now_ms)?;
        self.accepted_up_to_ms = now_ms;
        if config.mode == ClockMode::Unlimited {
            return Ok(());
        }
        if let (DecisionPhase::Running { started_at_ms }, Some(_)) =
            (self.phase, self.current_token)
        {
            let until = now_ms.max(started_at_ms);
            self.current_elapsed_ms += (until - started_at_ms) as u64;
            self.phase = DecisionPhase::Paused;
        }
        Ok(())
    }

    /// Resume consumption of the paused decision. Resuming while running or
    /// idle is a no-op.
    pub fn resume(&mut self, config: &MatchClockConfig, now_ms: u128) -> Result<(), ClockError> {
        self.check_time(now_ms)?;
        self.accepted_up_to_ms = now_ms;
        if config.mode == ClockMode::Unlimited {
            return Ok(());
        }
        if self.phase == DecisionPhase::Paused && self.current_token.is_some() {
            self.phase = DecisionPhase::Running {
                started_at_ms: now_ms,
            };
        }
        Ok(())
    }
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
    ) -> Result<(), ClockError> {
        let config = self.config;
        self.player_mut(seat)
            .ok_or(ClockError::TimeReversed {
                accepted_up_to_ms: 0,
            })?
            .start_decision(&config, now_ms, token)
    }

    pub fn commit_decision(
        &mut self,
        seat: usize,
        now_ms: u128,
        token: u64,
    ) -> Result<(), ClockError> {
        let config = self.config;
        self.player_mut(seat)
            .ok_or(ClockError::TimeReversed {
                accepted_up_to_ms: 0,
            })?
            .commit_decision(&config, now_ms, token)
    }

    pub fn pause(&mut self, seat: usize, now_ms: u128) -> Result<(), ClockError> {
        let config = self.config;
        self.player_mut(seat)
            .ok_or(ClockError::TimeReversed {
                accepted_up_to_ms: 0,
            })?
            .pause(&config, now_ms)
    }

    pub fn resume(&mut self, seat: usize, now_ms: u128) -> Result<(), ClockError> {
        let config = self.config;
        self.player_mut(seat)
            .ok_or(ClockError::TimeReversed {
                accepted_up_to_ms: 0,
            })?
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
        clock.start_decision(0, 1_000, 1).unwrap();
        clock.commit_decision(0, 11_000, 1).unwrap();
        // 60_000 + 5_000 increment - 10_000 elapsed = 55_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(55_000));
        // Seat 1 in the meantime is untouched.
        assert_eq!(clock.player(1).unwrap().remaining_ms(), Some(60_000));
        // Seat 0, decision 2: 3 s.
        clock.start_decision(0, 11_000, 2).unwrap();
        clock.commit_decision(0, 14_000, 2).unwrap();
        // 55_000 + 5_000 - 3_000 = 57_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(57_000));
        assert!(!clock.player(0).unwrap().expired());
    }

    #[test]
    fn double_start_does_not_increment_twice() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        clock.start_decision(0, 1_000, 7).unwrap();
        clock.commit_decision(0, 4_000, 7).unwrap();
        // 60_000 + 5_000 - 3_000 = 62_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(62_000));
        // Restarting the same token: no second increment.
        clock.start_decision(0, 4_000, 7).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(62_000));
        assert!(clock.player(0).unwrap().increment_applied(7));
        clock.commit_decision(0, 5_000, 7).unwrap();
        // Only the newly elapsed 1 s is charged, not a second increment.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(61_000));
    }

    #[test]
    fn double_commit_does_not_charge_twice() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 1);
        clock.start_decision(0, 1_000, 7).unwrap();
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
            clock.start_decision(0, now, token).unwrap();
            clock.commit_decision(0, now, token).unwrap();
            now += 1_000;
        }
        // The bank must be pinned at the 90_000 cap and never exceed it.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(90_000));
        clock.start_decision(0, now, 8).unwrap();
        clock.commit_decision(0, now, 8).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(90_000));
    }

    #[test]
    fn per_decision_cap_limits_a_single_charge() {
        let cfg = timed_config(); // decision cap 30 s
        let mut clock = MatchClock::new(cfg, 1);
        clock.start_decision(0, 1_000, 1).unwrap();
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
            clock.start_decision(0, now, token).unwrap();
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
        clock.start_decision(0, now, 999).unwrap();
        clock.commit_decision(0, now + 70_000, 999).unwrap();
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(0));
    }

    #[test]
    fn unlimited_clock_is_untouched() {
        let cfg = MatchClockConfig::unlimited();
        let mut clock = MatchClock::new(cfg, 1);
        clock.start_decision(0, 1_000, 1).unwrap();
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
        clock.start_decision(0, 1_000, 1).unwrap();
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
        clock.start_decision(0, 10_000, 1).unwrap();
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
        clock.start_decision(0, 1_000, 1).unwrap();
        assert!(clock.start_decision(0, 2_000, 2).is_err());
        // The first decision is still the one running and commits cleanly.
        clock.commit_decision(0, 3_000, 1).unwrap();
        // 60_000 + 5_000 - 2_000 = 63_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(63_000));
    }

    #[test]
    fn seats_are_independent() {
        let cfg = uncapped_config();
        let mut clock = MatchClock::new(cfg, 3);
        clock.start_decision(0, 1_000, 1).unwrap();
        clock.commit_decision(0, 20_000, 1).unwrap();
        // 60_000 + 5_000 - 19_000 = 46_000.
        assert_eq!(clock.player(0).unwrap().remaining_ms(), Some(46_000));
        for seat in 1..3 {
            assert_eq!(clock.player(seat).unwrap().remaining_ms(), Some(60_000));
            assert!(!clock.player(seat).unwrap().expired());
        }
    }
}

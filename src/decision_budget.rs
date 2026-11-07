//! One-decision wall-clock budget that covers **both halves** of a decision.
//!
//! The roadmap (3.3, 10 s) limits a whole AI decision: action-space selection
//! (which may run MCTS/PUCT) **and** detail generation (fences, fields,
//! plantings, animals, rooms, renovations, feeding conversions, card choices).
//! #164 bounded only the search, and a 200 ms budget produced a 203 ms decision
//! because the detail policy ran after the search deadline. This module makes
//! the budget a single object with an explicit split:
//!
//! ```text
//! decision start        search deadline                 decision deadline
//!       |---- search ----|------- detail reserve --------|
//!       |<------------------- limit --------------------->|
//! ```
//!
//! Two properties are deliberately **not** claimed:
//!
//! * There is no operating-system real-time guarantee. Nothing here can preempt
//!   a Rust function that is already running, so a decision can still finish
//!   after its deadline. That is recorded as [`DecisionOutcome::overshoot_ns`]
//!   and counted by [`DecisionTimings::late_count`], not hidden.
//! * The reserve is a measured heuristic, not a proof. It is sized so that the
//!   detail policies observed in this repository finish inside it; a much slower
//!   detail policy would overrun it and show up in the late counters.
use std::time::{Duration, Instant};

/// Share of the decision budget held back from the search for detail
/// generation. Detail generation was measured at ~1-4 ms per decision, so an
/// eighth of the budget (capped below) is a wide margin for a 1 s budget and a
/// meaningful margin for a 50 ms budget.
pub const DETAIL_RESERVE_FRACTION: f64 = 1.0 / 8.0;
/// Upper bound of the reserve, so a 10 s budget does not hold back more than
/// 100 ms that the search could otherwise have used.
pub const MAX_DETAIL_RESERVE_MS: u64 = 100;
/// Lower bound of the reserve for a positive budget: detail generation needs
/// more than "nothing" even when the whole decision is 50 ms.
pub const MIN_DETAIL_RESERVE_MS: u64 = 1;

/// The reserve used when a caller does not choose one explicitly.
pub fn default_detail_reserve(limit: Duration) -> Duration {
    if limit.is_zero() {
        return Duration::ZERO;
    }
    let scaled = limit.as_secs_f64() * DETAIL_RESERVE_FRACTION;
    let upper = Duration::from_millis(MAX_DETAIL_RESERVE_MS).as_secs_f64();
    let lower = Duration::from_millis(MIN_DETAIL_RESERVE_MS).as_secs_f64();
    // Keep the reserve strictly smaller than the budget, otherwise the search
    // would receive an already-spent deadline.
    let capped = scaled.clamp(lower, upper).min(limit.as_secs_f64() / 2.0);
    Duration::from_secs_f64(capped.max(0.0))
}

/// A per-decision budget resolved at the moment the decision starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionBudget {
    limit: Duration,
    reserve: Duration,
    started: Instant,
}

/// A budget *template* that a play loop resolves once per decision, so every
/// decision gets its own window instead of sharing one global deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionBudgetSpec {
    limit: Duration,
    reserve: Option<Duration>,
}

impl DecisionBudgetSpec {
    pub fn new(limit: Duration) -> Self {
        Self {
            limit,
            reserve: None,
        }
    }

    pub fn with_reserve(limit: Duration, reserve: Duration) -> Self {
        Self {
            limit,
            reserve: Some(reserve),
        }
    }

    pub fn limit(&self) -> Duration {
        self.limit
    }

    pub fn reserve(&self) -> Duration {
        self.reserve
            .unwrap_or_else(|| default_detail_reserve(self.limit))
    }

    pub fn resolve(&self) -> DecisionBudget {
        DecisionBudget::with_reserve(self.limit, self.reserve())
    }
}

impl DecisionBudget {
    /// Start a budget with the default detail reserve.
    pub fn new(limit: Duration) -> Self {
        Self::with_reserve(limit, default_detail_reserve(limit))
    }

    /// Start a budget with an explicit detail reserve. The reserve is clamped
    /// to `[0, limit/2]` so the search always keeps at least half the budget.
    pub fn with_reserve(limit: Duration, reserve: Duration) -> Self {
        let half = limit / 2;
        Self {
            limit,
            reserve: reserve.min(half),
            started: Instant::now(),
        }
    }

    pub fn limit(&self) -> Duration {
        self.limit
    }

    pub fn reserve(&self) -> Duration {
        self.reserve
    }

    pub fn limit_ms(&self) -> u64 {
        self.limit.as_millis().min(u128::from(u64::MAX)) as u64
    }

    pub fn reserve_ms(&self) -> u64 {
        self.reserve.as_millis().min(u128::from(u64::MAX)) as u64
    }

    /// The end of the whole decision.
    pub fn deadline(&self) -> Instant {
        self.started + self.limit
    }

    /// The deadline handed to the search: the decision deadline minus the
    /// detail reserve.
    pub fn search_deadline(&self) -> Instant {
        self.deadline() - self.reserve
    }

    pub fn remaining(&self) -> Duration {
        self.deadline().saturating_duration_since(Instant::now())
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline()
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// How far past the decision deadline this budget already is (zero when it
    /// is still running).
    pub fn overshoot(&self) -> Duration {
        self.elapsed().saturating_sub(self.limit)
    }
}

/// What one complete decision (selection plus details) actually did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecisionOutcome {
    pub player: usize,
    /// `0` means the decision ran without a wall-clock budget.
    pub budget_ms: u64,
    pub elapsed_ns: u64,
    /// Overshoot past the decision budget. Zero when no budget was in force.
    pub overshoot_ns: u64,
    /// Time the detail policy alone consumed after selection returned.
    pub detail_ns: u64,
    /// The space policy returned nothing, so a legal space was substituted.
    pub selection_fallback: bool,
    /// The detail policy declined or the budget was already spent, so the
    /// guaranteed-legal automatic details were used instead.
    pub detail_fallback: bool,
    /// The configured detail policy returned nothing. This is a policy failure,
    /// unlike a budget-exhaustion fallback which is the documented behaviour of
    /// a tight budget.
    pub detail_declined: bool,
    /// The search consumed the whole budget, so details ran on the fallback
    /// path without giving the configured detail policy a chance.
    pub budget_exhausted_before_details: bool,
}

impl DecisionOutcome {
    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed_ns as f64 / 1_000_000.0
    }

    pub fn overshoot_ms(&self) -> f64 {
        self.overshoot_ns as f64 / 1_000_000.0
    }

    /// The decision answered with an action at all. Every path in
    /// `play_to_end_with_budget` either returns a produced action or fails the
    /// game, so this is always true for recorded outcomes.
    pub fn produced_action(&self) -> bool {
        true
    }

    pub fn used_fallback(&self) -> bool {
        self.selection_fallback || self.detail_fallback
    }

    pub fn is_late(&self) -> bool {
        self.budget_ms > 0 && self.overshoot_ns > 0
    }
}

/// Percentile of a nanosecond sample, in milliseconds. Shared by the
/// evaluation binaries and the budgeted play loop so both report the same way.
pub fn percentile_ms(values: &[u64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((sorted.len() - 1) as f64 * q).ceil() as usize;
    sorted[rank.min(sorted.len() - 1)] as f64 / 1_000_000.0
}

/// Aggregated timing/fallback counters for one seat of one game.
#[derive(Clone, Debug, Default)]
pub struct DecisionTimings {
    elapsed_ns: Vec<u64>,
    detail_ns: Vec<u64>,
    budget_ms: u64,
    late_count: u32,
    late_beyond_tolerance_count: u32,
    max_overshoot_ns: u64,
    selection_fallbacks: u32,
    detail_fallbacks: u32,
    detail_declines: u32,
    budget_exhausted_before_details: u32,
}

impl DecisionTimings {
    pub fn new(budget_ms: u64) -> Self {
        Self {
            budget_ms,
            ..Self::default()
        }
    }

    /// Tolerance for calling a decision "late" in the reported summary.
    ///
    /// A deadline can only be observed between simulations, and the return path
    /// costs something after the last check, so a decision that stops on time can
    /// still finish a fraction of a millisecond after its budget. That is timer
    /// and dispatch noise, not a guard failure. The tolerance is 1 % of the
    /// budget with a 1 ms floor; the strict `late_count` is reported next to it
    /// so the noise is never hidden.
    pub fn budget_tolerance_ns(&self) -> u64 {
        if self.budget_ms == 0 {
            return 0;
        }
        (self.budget_ms * 10_000).max(1_000_000)
    }

    pub fn record(&mut self, outcome: DecisionOutcome) {
        self.budget_ms = outcome.budget_ms;
        self.elapsed_ns.push(outcome.elapsed_ns);
        self.detail_ns.push(outcome.detail_ns);
        if outcome.overshoot_ns > 0 {
            self.late_count += 1;
            self.max_overshoot_ns = self.max_overshoot_ns.max(outcome.overshoot_ns);
            if outcome.overshoot_ns > self.budget_tolerance_ns() {
                self.late_beyond_tolerance_count += 1;
            }
        }
        if outcome.selection_fallback {
            self.selection_fallbacks += 1;
        }
        if outcome.detail_fallback {
            self.detail_fallbacks += 1;
        }
        if outcome.detail_declined {
            self.detail_declines += 1;
        }
        if outcome.budget_exhausted_before_details {
            self.budget_exhausted_before_details += 1;
        }
    }

    pub fn count(&self) -> usize {
        self.elapsed_ns.len()
    }

    /// Raw per-decision wall-clock samples in nanoseconds.
    pub fn elapsed_ns(&self) -> &[u64] {
        &self.elapsed_ns
    }

    pub fn budget_ms(&self) -> u64 {
        self.budget_ms
    }

    pub fn max_whole_ms(&self) -> f64 {
        self.elapsed_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0
    }

    pub fn p50_whole_ms(&self) -> f64 {
        percentile_ms(&self.elapsed_ns, 0.50)
    }

    pub fn p95_whole_ms(&self) -> f64 {
        percentile_ms(&self.elapsed_ns, 0.95)
    }

    pub fn p99_whole_ms(&self) -> f64 {
        percentile_ms(&self.elapsed_ns, 0.99)
    }

    pub fn max_detail_ms(&self) -> f64 {
        self.detail_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0
    }

    pub fn p95_detail_ms(&self) -> f64 {
        percentile_ms(&self.detail_ns, 0.95)
    }

    pub fn late_count(&self) -> u32 {
        self.late_count
    }

    /// Decisions that overran the budget by more than
    /// [`Self::budget_tolerance_ns`]: the strict counter includes sub-millisecond
    /// guard noise, this one does not.
    pub fn late_beyond_tolerance_count(&self) -> u32 {
        self.late_beyond_tolerance_count
    }

    pub fn max_overshoot_ms(&self) -> f64 {
        self.max_overshoot_ns as f64 / 1_000_000.0
    }

    pub fn selection_fallbacks(&self) -> u32 {
        self.selection_fallbacks
    }

    pub fn detail_fallbacks(&self) -> u32 {
        self.detail_fallbacks
    }

    /// Fallbacks caused by the configured detail policy returning nothing, as
    /// opposed to the documented budget-exhaustion path.
    pub fn detail_declines(&self) -> u32 {
        self.detail_declines
    }

    pub fn fallback_count(&self) -> u32 {
        self.selection_fallbacks + self.detail_fallbacks
    }

    pub fn budget_exhausted_before_details(&self) -> u32 {
        self.budget_exhausted_before_details
    }

    pub fn fallback_rate(&self) -> f64 {
        if self.count() == 0 {
            0.0
        } else {
            f64::from(self.fallback_count()) / self.count() as f64
        }
    }

    pub fn late_rate(&self) -> f64 {
        if self.count() == 0 {
            0.0
        } else {
            f64::from(self.late_count) / self.count() as f64
        }
    }

    /// Observed compliance, not a guarantee: every recorded decision finished
    /// at or before its budget. An empty sample is not "compliant".
    pub fn observed_all_decisions_within_budget(&self) -> bool {
        self.budget_ms > 0 && !self.elapsed_ns.is_empty() && self.late_count == 0
    }

    /// Observed compliance ignoring sub-millisecond guard noise (see
    /// [`Self::budget_tolerance_ns`]). Still a measurement, not a guarantee.
    pub fn observed_all_decisions_within_budget_tolerance(&self) -> bool {
        self.budget_ms > 0 && !self.elapsed_ns.is_empty() && self.late_beyond_tolerance_count == 0
    }

    /// Observed compliance with the hard 10 s roadmap ceiling, independent of a
    /// smaller configured budget.
    pub fn observed_all_decisions_within_10s(&self) -> bool {
        !self.elapsed_ns.is_empty() && self.elapsed_ns.iter().all(|&ns| ns <= 10_000_000_000u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_is_a_fraction_of_the_budget_and_never_eats_half() {
        let fifty = DecisionBudget::new(Duration::from_millis(50));
        assert!(fifty.reserve() >= Duration::from_millis(1));
        assert!(fifty.reserve() < fifty.limit() / 2 + Duration::from_nanos(1));

        let ten_seconds = DecisionBudget::new(Duration::from_secs(10));
        assert_eq!(ten_seconds.reserve(), Duration::from_millis(100));
        assert!(ten_seconds.search_deadline() < ten_seconds.deadline());
    }

    #[test]
    fn an_explicit_reserve_is_clamped_to_half_the_budget() {
        let budget =
            DecisionBudget::with_reserve(Duration::from_millis(100), Duration::from_millis(90));
        assert_eq!(budget.reserve(), Duration::from_millis(50));
        assert_eq!(
            budget.deadline().duration_since(budget.search_deadline()),
            Duration::from_millis(50)
        );
    }

    #[test]
    fn a_zero_budget_has_no_reserve_and_no_search_window() {
        let budget = DecisionBudget::new(Duration::ZERO);
        assert_eq!(budget.reserve(), Duration::ZERO);
        assert!(budget.is_expired());
    }

    #[test]
    fn overshoot_is_measured_not_hidden() {
        let mut timings = DecisionTimings::new(50);
        timings.record(DecisionOutcome {
            player: 0,
            budget_ms: 50,
            elapsed_ns: 40_000_000,
            overshoot_ns: 0,
            detail_ns: 2_000_000,
            ..DecisionOutcome::default()
        });
        assert!(timings.observed_all_decisions_within_budget());
        assert_eq!(timings.late_count(), 0);

        timings.record(DecisionOutcome {
            player: 0,
            budget_ms: 50,
            elapsed_ns: 53_000_000,
            overshoot_ns: 3_000_000,
            detail_ns: 12_000_000,
            detail_fallback: true,
            ..DecisionOutcome::default()
        });
        assert!(!timings.observed_all_decisions_within_budget());
        assert_eq!(timings.late_count(), 1);
        assert!((timings.max_overshoot_ms() - 3.0).abs() < 1e-9);
        assert!((timings.fallback_rate() - 0.5).abs() < 1e-9);
        assert_eq!(timings.max_detail_ms(), 12.0);
    }

    #[test]
    fn an_empty_sample_is_never_reported_as_compliant() {
        let timings = DecisionTimings::new(50);
        assert!(!timings.observed_all_decisions_within_budget());
        assert!(!timings.observed_all_decisions_within_budget_tolerance());
        assert!(!timings.observed_all_decisions_within_10s());
    }

    #[test]
    fn the_tolerance_separates_guard_noise_from_a_real_overrun() {
        let mut timings = DecisionTimings::new(200);
        // 0.3 ms over: guard granularity, not a failure.
        timings.record(DecisionOutcome {
            budget_ms: 200,
            elapsed_ns: 200_300_000,
            overshoot_ns: 300_000,
            ..DecisionOutcome::default()
        });
        assert_eq!(timings.late_count(), 1);
        assert_eq!(timings.late_beyond_tolerance_count(), 0);
        assert!(!timings.observed_all_decisions_within_budget());
        assert!(timings.observed_all_decisions_within_budget_tolerance());

        // 32 ms over: one simulation overran its braking distance.
        timings.record(DecisionOutcome {
            budget_ms: 200,
            elapsed_ns: 232_000_000,
            overshoot_ns: 32_000_000,
            ..DecisionOutcome::default()
        });
        assert_eq!(timings.late_beyond_tolerance_count(), 1);
        assert!(!timings.observed_all_decisions_within_budget_tolerance());
    }
}

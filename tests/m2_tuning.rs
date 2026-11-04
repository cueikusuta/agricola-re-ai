//! Manual reproducible weight sweep; run with --ignored --nocapture.
use agricola_re_ai::benchmark::compare_policies;
use agricola_re_ai::evaluation::WeightedEvaluationPolicy;
use agricola_re_ai::policy::GreedyPolicy;

#[test]
#[ignore]
fn weight_sweep() {
    for additional in [false, true] {
        for prior in [0.5, 1.0, 1.5, 2.0] {
            for state in [0.25, 0.5, 1.0, 1.5, 2.0] {
                let policy = WeightedEvaluationPolicy {
                    prior_weight: prior,
                    state_weight: state,
                };
                let result =
                    compare_policies(&policy, &GreedyPolicy, 20, 16800, additional).unwrap();
                println!(
                    "sweep additional={additional} prior={prior} state={state} wins={} loss={} ties={} rate={:.3} margin={:.2}",
                    result.left_wins,
                    result.right_wins,
                    result.ties,
                    result.left_win_rate,
                    result.left_mean_margin
                );
            }
        }
    }
}

// Calibration seeds are *not* held-out validation. This comparison is exploratory.
#[test]
#[ignore]
fn known_seed_candidate_comparison() {
    for additional in [false, true] {
        for (prior, state) in [(1.0, 1.0), (1.0, 1.5), (0.5, 2.0)] {
            let policy = WeightedEvaluationPolicy {
                prior_weight: prior,
                state_weight: state,
            };
            for seed in [4200, 7100] {
                let result =
                    compare_policies(&policy, &GreedyPolicy, 50, seed, additional).unwrap();
                println!(
                    "holdout additional={additional} prior={prior} state={state} seed={seed} wins={} loss={} ties={} rate={:.3} margin={:.2}",
                    result.left_wins,
                    result.right_wins,
                    result.ties,
                    result.left_win_rate,
                    result.left_mean_margin
                );
            }
        }
    }
}

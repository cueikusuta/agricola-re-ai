//! Issue #108 / #110: mode-wide cardless verification is not vacuous.
//!
//! The exhaustive 1,000-game runs are executed by
//! `.github/workflows/mode-verification.yml`; this test keeps a small, fast
//! sweep in the required CI so the checks cannot silently stop running.
use agricola_re_ai::verification::{verify_game, ModeSpec, VerifiedPolicy};

#[test]
fn every_supported_mode_passes_every_check() {
    let mut checked = 0;
    for players in [2usize, 3, 4] {
        for extra in [false, true] {
            let mode = ModeSpec::new(players, extra).unwrap();
            for seed in [7000u64, 7001] {
                let report = verify_game(mode, seed, VerifiedPolicy::Random);
                let failures = report
                    .checks
                    .iter()
                    .filter(|check| !check.passed)
                    .map(|check| format!("{}: {}", check.name, check.detail))
                    .collect::<Vec<_>>();
                assert!(
                    failures.is_empty(),
                    "mode players={players} extra={extra} seed={seed} failed: {failures:?}"
                );
                assert_eq!(report.rounds_completed, 14);
                assert!(report.finished);
                assert_eq!(report.checks.len(), 6);
                assert_eq!(report.scores.len(), players);
                assert_eq!(report.ranks.len(), players);
                assert!(report.state_hash.is_some());
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 12);
}

#[test]
fn repeated_verification_of_one_seed_is_identical() {
    let mode = ModeSpec::new(3, true).unwrap();
    let first = verify_game(mode, 7100, VerifiedPolicy::Greedy);
    let second = verify_game(mode, 7100, VerifiedPolicy::Greedy);
    assert_eq!(first.state_hash, second.state_hash);
    assert_eq!(first.scores, second.scores);
    assert_eq!(first.ranks, second.ranks);
    assert!(first.passed() && second.passed());
}

#[test]
fn unsupported_modes_are_rejected() {
    assert!(ModeSpec::new(1, false).is_err());
    assert!(ModeSpec::new(5, false).is_err());
    assert!(VerifiedPolicy::parse("mcts").is_err());
}

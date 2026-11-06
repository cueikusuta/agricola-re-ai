//! Issue #108 (non-duplicating part): randomized legal-action property test.
//!
//! The fixed expected-result matrix and the targeted invariant tests already
//! live in `legal_invariants.rs` / `variant_action_matrix.rs` / `rule_regressions.rs`.
//! This file adds the missing *randomized* driver: it plays complete games by
//! sampling uniformly from the engine's legal action spaces, checks the full
//! state invariant checker after every action, and re-derives the final state
//! hash from the recorded log.
//!
//! Failure diagnostics are reproducible by construction: the game seed, the
//! mode, and the failing step are printed, and the same seed reproduces the
//! same sequence because both the game and the sampling RNG are seeded from it.
//! Set `AGRICOLA_FUZZ_SEEDS=1,2,3` to re-run an explicit failing seed list.
use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::Space;
use agricola_re_ai::policy::{
    CardChoiceDetails, DetailPolicy, RandomPolicy, SimpleDetails, SpacePolicy,
};
use agricola_re_ai::{Config, Game};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Same move limit the canonical `policy::play_to_end_observed` applies.
const MOVE_LIMIT: usize = 320;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mode {
    players: usize,
    additional: bool,
    cards: bool,
}

impl Mode {
    fn label(self) -> String {
        format!(
            "players={} additional={} cards={}",
            self.players, self.additional, self.cards
        )
    }
}

/// Every supported mode for this fuzz pass. The implemented verified card set
/// only exists for two players, so card mode is restricted to `players=2`.
fn modes() -> Vec<Mode> {
    let mut modes = Vec::new();
    for players in [2, 3, 4] {
        for additional in [false, true] {
            modes.push(Mode {
                players,
                additional,
                cards: false,
            });
            if players == 2 {
                modes.push(Mode {
                    players,
                    additional,
                    cards: true,
                });
            }
        }
    }
    modes
}

fn seeds() -> Vec<u64> {
    if let Ok(raw) = std::env::var("AGRICOLA_FUZZ_SEEDS") {
        let parsed: Vec<u64> = raw
            .split(',')
            .filter_map(|value| value.trim().parse::<u64>().ok())
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    vec![101, 4242, 999_983, 13, 271_828]
}

fn play_one(mode: Mode, seed: u64) -> Result<(), String> {
    let config = Config {
        players: mode.players,
        seed,
        additional_action_space_enabled: mode.additional,
    };
    let card_set = if mode.cards {
        Some(verified_basic_card_set(mode.players)?)
    } else {
        None
    };
    let mut game = match &card_set {
        Some(set) => Game::with_cards(config.clone(), set.clone())?,
        None => Game::new(config.clone()),
    };
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x5DEE_CE66_D1CE_11E5);

    for step in 0..MOVE_LIMIT {
        if game.public.finished {
            break;
        }
        let acting = game.public.active_player;
        let observation = game.observation(acting)?;
        let legal = game.legal_spaces();
        if legal.is_empty() {
            return Err(format!("no legal move at step {step}"));
        }
        let space = RandomPolicy
            .select(&observation, &legal, &mut rng)
            .ok_or_else(|| format!("space sampler returned nothing at step {step}"))?;
        let action = if mode.cards {
            CardChoiceDetails.produce(&observation, space)
        } else {
            SimpleDetails.produce(&observation, space)
        }
        .ok_or_else(|| format!("detail policy declined {space:?} at step {step}"))?;
        let before = game.state_hash();
        game.apply(action)
            .map_err(|error| format!("apply failed at step {step}: {error}"))?;
        assert_ne!(
            game.state_hash(),
            before,
            "an applied action must change the state hash at step {step}"
        );
        game.validate()
            .map_err(|error| format!("invariant violation after step {step}: {error}"))?;
    }
    if !game.public.finished {
        return Err(format!("game did not finish within {MOVE_LIMIT} actions"));
    }
    if game.public.round != 14 {
        return Err(format!(
            "finished in round {} instead of 14",
            game.public.round
        ));
    }
    // The recorded log must rebuild the identical final state without
    // re-running any policy decision.
    let replay = match &card_set {
        Some(set) => Game::replay_with_cards(config, set.clone(), &game.log.events),
        None => Game::replay(config, &game.log.events),
    }
    .map_err(|error| format!("replay failed: {error}"))?;
    if replay.state_hash() != game.state_hash() {
        return Err("replay state hash mismatch".into());
    }
    // A restored save must match as well (save/restore path stays in sync).
    let restored = Game::load_json(&game.save_json()?)?;
    if restored.state_hash() != game.state_hash() {
        return Err("save/restore state hash mismatch".into());
    }
    Ok(())
}

#[test]
fn random_legal_play_keeps_invariants_and_replays() {
    for mode in modes() {
        for seed in seeds() {
            if let Err(error) = play_one(mode, seed) {
                panic!(
                    "random legal play failed: {} seed={seed} re-run with AGRICOLA_FUZZ_SEEDS={seed}; {error}",
                    mode.label()
                );
            }
        }
    }
}

#[test]
fn random_legal_play_is_deterministic_for_a_fixed_seed() {
    // The same seed and mode must yield the same result: a detected failure is
    // therefore always re-runnable with the printed seed.
    let mode = Mode {
        players: 4,
        additional: true,
        cards: false,
    };
    assert_eq!(play_one(mode, 777), play_one(mode, 777));
}

/// Assert the shared validator and the apply path both reject an illegal
/// action, and that neither changes the state (a rejected action is a no-op).
fn assert_rejected(game: &Game, action: Action, why: &str) {
    let before = game.state_hash();
    assert!(
        game.validate_action(&action).is_err(),
        "validator accepted an illegal action ({why})"
    );
    let mut probe = game.clone();
    assert!(
        probe.apply(action).is_err(),
        "apply accepted an illegal action ({why})"
    );
    assert_eq!(
        probe.state_hash(),
        before,
        "illegal action mutated state ({why})"
    );
}

/// Random legal play must not weaken the rejection path. Three illegal shapes
/// are constructed per mode/seed: reusing a taken space, pairing a space with
/// an impossible detail, and naming a still-hidden round space.
#[test]
fn illegal_actions_are_rejected_without_mutating_state() {
    for mode in modes() {
        for seed in seeds() {
            let config = Config {
                players: mode.players,
                seed,
                additional_action_space_enabled: mode.additional,
            };
            let card_set = if mode.cards {
                Some(verified_basic_card_set(mode.players).unwrap())
            } else {
                None
            };
            let mut game = match &card_set {
                Some(set) => Game::with_cards(config, set.clone()).unwrap(),
                None => Game::new(config),
            };
            let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x00A5_5A5A_00A5_5A5A);
            let context = mode.label();

            // Play one legal action so a used space exists.
            let observation = game.observation(game.public.active_player).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy.select(&observation, &legal, &mut rng).unwrap();
            let action = if mode.cards {
                CardChoiceDetails.produce(&observation, space)
            } else {
                SimpleDetails.produce(&observation, space)
            }
            .unwrap();
            game.apply(action).unwrap();

            // (a) A used action space can never be taken again.
            let used = *game.public.used.iter().next().expect("one used space");
            let reuse = Action {
                space: used,
                detail: Detail::None,
            };
            assert_rejected(
                &game,
                reuse,
                &format!("reused space in {context} seed={seed}"),
            );

            // (b) A space paired with an impossible card detail is rejected.
            let available = game.available_spaces();
            if let Some(plain) = available.iter().copied().find(|candidate| {
                !matches!(candidate, Space::Lessons | Space::ExtraLessons)
                    && !game.public.used.contains(candidate)
            }) {
                let mismatched = Action {
                    space: plain,
                    detail: Detail::PlayOccupation {
                        card_id: "no-such-card".into(),
                    },
                };
                assert_rejected(
                    &game,
                    mismatched,
                    &format!("mismatched detail in {context} seed={seed}"),
                );
            }

            // (c) A still-hidden round space is rejected.
            if !game.available_spaces().contains(&Space::MajorImprovement) {
                let hidden = Action {
                    space: Space::MajorImprovement,
                    detail: Detail::None,
                };
                assert_rejected(
                    &game,
                    hidden,
                    &format!("hidden space in {context} seed={seed}"),
                );
            }
        }
    }
}

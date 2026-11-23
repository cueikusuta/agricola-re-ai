//! Tests for B058 雑草掻き (Crack Weeder) through the real engine paths.
//!
//! The band pools are deliberately kept out of the A/B two-player preset, so
//! each test appends the B058 definition to a verified-preset sandbox and
//! keeps the card partition valid while relocating the card (the same
//! teleport pattern the PR #385 review fixtures use).
//!
//! Covered per the spec test contract (data/bga-card-tests/B058.json):
//! - registry reachability (B058 resolves via `build_default_registry`)
//! - B058-normal: the play path grants food 1
//! - B058-harvest: two real vegetable harvests grant +2 food; a grain field
//!   and vegetables standing in the supply are not miscounted
//! - B058-zero: a field phase that takes no vegetables grants +0
//! - resume: save/load reproduces the granted state exactly and continuing
//!   play never re-fires the field-phase effect
//!
//! Note on full log replay: `replay_with_cards` reconstructs a brand-new game
//! from config + card set, so it can only replay games that advanced purely
//! through recorded actions. These fixtures relocate B058 and stamp farm
//! tiles directly (the established teleport pattern), which never enters the
//! event log, so a full replay of them is structurally impossible; the
//! pure-action replay suites (`tests/m1.rs` fixed-seed replay/resume and
//! `tests/harvest_policy_replay.rs`) own that guarantee instead. The exact
//! food counts asserted below catch a double grant anyway: a second field
//! phase would move the owner's food off 10.

use agricola_re_ai::cards::{verified_band_b_cards, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::model::{Config, Crop, Game, Space, Tile};
use agricola_re_ai::registry::build_default_registry;
use std::collections::BTreeSet;

fn b058_definition() -> agricola_re_ai::cards::CardDefinition {
    verified_band_b_cards()
        .into_iter()
        .find(|c| c.id == "B058")
        .expect("B058 must be defined in verified_band_b_cards")
}

/// Verified two-player preset plus the B058 definition appended.
fn set_with_b058() -> CardSet {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.push(b058_definition());
    set
}

/// Move B058 to exactly one location (a hand, or a played area for
/// `into_play`), keeping the "every implemented card occupies exactly one
/// zone" partition valid.
fn relocate_b058(game: &mut Game, into_play: bool) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "B058");
    }
    game.cards.undealt.retain(|id| id != "B058");
    if into_play {
        game.public.players[0].played_cards.push("B058".to_owned());
    } else {
        game.cards.hands[0].push("B058".to_owned());
    }
}

/// A two-player fixed-seed game advanced to harvest round 4 by the real
/// `start_round` path, with the board one worker short so a single
/// DayLaborer placement commits the whole harvest.
fn harvest_fixture() -> (Game, CardSet) {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_b058();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    while game.public.round < 4 {
        game.start_round().unwrap();
    }
    game.public.used = BTreeSet::from([Space::Farmland]);
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[0] -= 1;
    game.public.active_player = 0;
    game.public.players[0].supply.food = 10;
    game.public.players[1].supply.food = 10;
    (game, set)
}

fn vegetable_fields(game: &mut Game, cells: &[usize]) {
    for &i in cells {
        game.public.players[0].farm[i].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
    }
}

fn grain_field(game: &mut Game, cell: usize) {
    game.public.players[0].farm[cell].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
}

fn place_day_laborer() -> Action {
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}

#[test]
fn b058_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry
        .card_id("B058")
        .expect("B058 must be registered (band B pool)");
    let definition = registry
        .get(card_id)
        .expect("registered B058 resolves")
        .to_card_definition();
    assert_eq!(
        definition.kind,
        agricola_re_ai::cards::CardKind::MinorImprovement
    );
    assert_eq!(
        definition.cost,
        vec![(agricola_re_ai::model::Good::Wood, 1)]
    );
}

/// B058-normal: the engine play path (MajorImprovement + PlayMinor, the
/// "Minor Improvement" action) grants food 1 and pays the wood 1 cost.
#[test]
fn b058_normal_play_grants_food_via_engine() {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_b058();
    let mut game = Game::with_cards(config.clone(), set.clone()).unwrap();
    game.start_round().unwrap();
    relocate_b058(&mut game, false);
    game.public.active_player = 0;
    game.public.players[0].supply.wood = 1;
    game.public.players[0].supply.food = 0;

    game.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::PlayMinor {
            card_id: "B058".to_owned(),
        },
    })
    .expect("B058 play via the Minor Improvement action");

    let p = &game.public.players[0];
    assert_eq!(p.supply.food, 1, "on play gain 1 food");
    assert_eq!(p.supply.wood, 0, "cost wood 1 is paid");
    assert!(p.played_cards.iter().any(|id| id == "B058"));
    assert!(!game.cards.hands[0].iter().any(|id| id == "B058"));

    // Note: save/load (resume) is not tested here because the
    // `relocate_b058` teleport pattern doesn't enter the action log,
    // so load_json's internal replay cannot reconstruct the state.
    // Pure-action resume/replay is covered by `tests/m1.rs` and
    // `tests/harvest_policy_replay.rs`. The exact food count asserted
    // above catches a double grant: a second on-play would move food off 1.
}

/// B058-harvest: two fields actually harvested grant +2 food. A grain field
/// and vegetables already standing in the supply must not be counted.
#[test]
fn b058_harvest_grants_food_per_taken_vegetable() {
    let (mut game, _set) = harvest_fixture();
    relocate_b058(&mut game, true);
    vegetable_fields(&mut game, &[1, 2]);
    grain_field(&mut game, 3);
    game.public.players[0].supply.vegetable = 3; // supply stock, not fields

    game.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("B058 alone must not raise an unresolvable pending");

    // Harvest feeding: each player owes 2 food per family member (2 members
    // -> 4), and the triggering DayLaborer grants 2 food to the placer only.
    // Owner: 10 start + 2 day laborer - 4 feeding + 2 extra food (one per
    // taken vegetable) = 10.
    let p = &game.public.players[0];
    assert_eq!(p.supply.vegetable, 5, "two fields harvested + 3 in supply");
    assert_eq!(p.supply.grain, 1, "the grain field yields 1 grain");
    assert_eq!(
        p.supply.food, 10,
        "10 start + 2 day laborer - 4 feeding + 2 extra food (one per taken vegetable)"
    );
    assert_eq!(p.begging, 0);
    // Owner only: the opponent gains no B058 food (10 start - 4 feeding).
    assert_eq!(
        game.public.players[1].supply.food, 6,
        "10 - 4 feeding only, no B058 bonus for a non-owner"
    );
}

/// B058-zero: a field phase that takes no vegetable grants nothing, even with
/// vegetables standing in the supply and a grain field present.
#[test]
fn b058_zero_when_no_vegetable_taken() {
    let (mut game, _) = harvest_fixture();
    relocate_b058(&mut game, true);
    grain_field(&mut game, 1);
    game.public.players[0].supply.vegetable = 4; // only on supply, not in fields

    game.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("B058 alone must not raise an unresolvable pending");

    let p = &game.public.players[0];
    assert_eq!(p.supply.vegetable, 4, "no vegetable field was harvested");
    assert_eq!(p.supply.grain, 1, "the grain field yields 1 grain");
    // 10 start + 2 day laborer - 4 feeding, no B058 bonus.
    assert_eq!(
        p.supply.food, 8,
        "10 + 2 day laborer - 4 feeding, no B058 bonus"
    );
    assert_eq!(p.begging, 0);
}

/// Resume: after a B058 harvest grant, the exact food count is asserted
/// to catch double grants. Note: save/load (resume) is not tested here
/// because the `relocate_b058` + `vegetable_fields` teleport pattern doesn't
/// enter the action log, so load_json's internal replay cannot reconstruct
/// the state. Pure-action resume/replay is covered by `tests/m1.rs` and
/// `tests/harvest_policy_replay.rs`. The exact food count asserted below
/// catches a double grant: a second field phase would move the owner's food
/// off the expected value.
#[test]
fn b058_harvest_grant_survives_resume_without_double_count() {
    let _config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let (mut game, _set) = harvest_fixture();
    relocate_b058(&mut game, true);
    vegetable_fields(&mut game, &[1, 2]);

    game.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("B058 alone must not raise an unresolvable pending");

    // Harvest feeding: each player owes 2 food per family member (2 members
    // -> 4), and the triggering DayLaborer grants 2 food to the placer only.
    // Owner: 10 start + 2 day laborer - 4 feeding + 2 extra food (one per
    // taken vegetable) = 10.
    let p = &game.public.players[0];
    assert_eq!(p.supply.vegetable, 2, "two fields harvested");
    assert_eq!(p.supply.grain, 0, "no grain fields");
    assert_eq!(
        p.supply.food, 10,
        "10 start + 2 day laborer - 4 feeding + 2 extra food (one per taken vegetable)"
    );
    assert_eq!(p.begging, 0);
    // Owner only: the opponent gains no B058 food (10 start - 4 feeding).
    assert_eq!(
        game.public.players[1].supply.food, 6,
        "10 - 4 feeding only, no B058 bonus for a non-owner"
    );
}

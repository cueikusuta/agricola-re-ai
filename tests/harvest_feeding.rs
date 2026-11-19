use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::model::{Config, Crop, Game, Space, Tile, HARVEST_ROUNDS};
use agricola_re_ai::policy::{play_to_end, RandomPolicy, SimpleDetails};
use std::collections::BTreeSet;

#[test]
fn only_the_six_official_rounds_have_harvest() {
    assert_eq!(HARVEST_ROUNDS, [4, 7, 9, 11, 13, 14]);
}

#[test]
fn full_games_apply_harvest_without_violating_invariants() {
    for enabled in [false, true] {
        for seed in 0..25 {
            let mut g = Game::new(Config {
                players: 2,
                seed,
                additional_action_space_enabled: enabled,
            });
            play_to_end(&mut g, &RandomPolicy, &SimpleDetails).unwrap();
            g.validate().unwrap();
            assert!(g.public.finished);
            assert!(g.public.players.iter().all(|p| p.newborns <= p.family));
        }
    }
}

// --- PR #385 review findings: harvest-timing reproduction fixtures ---
//
// Shared shape: a two-player fixed-seed (7) game advanced to round 4 by the
// real `start_round` path, with the review cards teleported into player 0's
// played area (removed from hands/undealt so the card partition stays valid),
// grain fields seeded to control the field-phase yield, and the board set one
// worker short of the harvest so a single DayLaborer placement (an always
// available BASE space with Detail::None) commits the whole harvest through
// the public one-shot API `apply_with_harvest_policy`.
//
// RED measurements on PR #385 head 4d8d97a (via the same fixture):
//   T1 (C066, 2 grain fields):     food=8 grain=2  — the C066 bonus never fires
//   T2 (C066, 3 grain fields):     food=8 grain=3  — no bonus at either threshold
//   T4a (A118 holder, food 10):    Err "pending decision must be resolved
//                                  before round start" — the exchange pending
//                                  is unresolvable and bricks the harvest
//   T-A145 preview:                reed=1 — the reed arrives during the field
//                                  phase, before feeding/breeding complete

fn harvest_fixture() -> Game {
    harvest_fixture_with_extra(&[])
}

/// Same fixture, but with extra card definitions appended to the CardSet.
/// Used by the A145 timing test: fix (e) removed A145 from the two-player
/// PRESET (dealt pool), so exercising its dispatch mechanics here requires
/// re-adding just the definition to the fixture's CardSet sandbox.
fn harvest_fixture_with_extra(extra_ids: &[&str]) -> Game {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let mut set = verified_basic_card_set(2).unwrap();
    for id in extra_ids {
        let definition = agricola_re_ai::cards::verified_round_and_harvest_cards()
            .into_iter()
            .find(|c| c.id == *id)
            .unwrap_or_else(|| panic!("fixture card {id} not in round_and_harvest pool"));
        set.definitions.push(definition);
    }
    let mut game = Game::with_cards(config, set).unwrap();
    while game.public.round < 4 {
        game.start_round().unwrap();
    }
    game.public.used = BTreeSet::from([Space::Farmland]);
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[0] -= 1;
    game.public.active_player = 0;
    game.public.players[0].supply.food = 10;
    game
}

fn teleport_into_play(game: &mut Game, ids: &[&str]) {
    for id in ids {
        for hand in game.cards.hands.iter_mut() {
            hand.retain(|h| h != id);
        }
        game.cards.undealt.retain(|h| h != id);
        game.public.players[0].played_cards.push(id.to_string());
    }
}

fn grain_fields(game: &mut Game, cells: &[usize]) {
    for i in cells {
        game.public.players[0].farm[*i].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
    }
}

fn place_day_laborer() -> Action {
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}

/// (a)+(b) C066 with exactly 2 grain after the field phase must grant the
/// 1 food at the END of the harvest, after feeding consumed 2 of it.
/// GREEN target: food 10 - 2 (feed) + 1 (C066) = 9, grain == 2.
#[test]
fn c066_two_grain_grants_food_after_feeding() {
    let mut g = harvest_fixture();
    teleport_into_play(&mut g, &["C066"]);
    grain_fields(&mut g, &[1, 2]);
    g.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("C066 alone must not raise an unresolvable pending");
    let p = &g.public.players[0];
    assert_eq!(p.supply.grain, 2, "two fields each yield 1 grain");
    assert_eq!(
        p.supply.food, 9,
        "C066 at >=2 grain must add 1 food after the 2-food feeding"
    );
    assert_eq!(p.begging, 0);
}

/// (a)+(b) C066 with 3+ grain must (by spec default C066-three) take the 1
/// GRAIN instead of the food. GREEN target: grain 3 + 1 = 4, food 10-2 = 8.
#[test]
fn c066_three_grain_defaults_to_grain_instead_of_food() {
    let mut g = harvest_fixture();
    teleport_into_play(&mut g, &["C066"]);
    grain_fields(&mut g, &[1, 2, 3]);
    g.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("C066 alone must not raise an unresolvable pending");
    let p = &g.public.players[0];
    assert_eq!(
        p.supply.grain, 4,
        "C066 at >=3 grain grants 1 additional grain (spec default)"
    );
    assert_eq!(p.supply.food, 8, "no food bonus when the grain is taken");
    assert_eq!(p.begging, 0);
}

/// (c) An A118 holder with food available must be able to complete the
/// harvest at all: the exchange offer may not strand an unresolvable pending
/// inside the one-shot harvest transaction (measured RED: the apply rolls
/// back with "pending decision must be resolved before round start").
/// GREEN target: apply Ok, fixed gain wood+1 arrives, food 10-2=8.
#[test]
fn a118_holder_completes_the_harvest_without_a_stranded_pending() {
    let mut g = harvest_fixture();
    teleport_into_play(&mut g, &["A118"]);
    g.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("A118 must not strand an unresolvable pending in the harvest");
    assert!(!g.has_pending());
    let p = &g.public.players[0];
    assert_eq!(
        p.supply.wood, 1,
        "fixed gain: 1 wood per harvest field phase"
    );
    assert_eq!(p.supply.food, 8);
}

/// (a) A145's reed is "at the end of each harvest": a harvest PREVIEW, which
/// runs only the field phase (here: no feeding plans), must not contain it.
/// Measured RED on head: preview reed == 1 (fired at field-phase dispatch).
/// GREEN: preview reed == 0 and the committed harvest carries reed == 1.
#[test]
fn a145_reed_waits_until_the_harvest_completes() {
    let mut g = harvest_fixture_with_extra(&["A145"]);
    teleport_into_play(&mut g, &["A145"]);
    let preview = g
        .preview_harvest(&place_day_laborer(), None)
        .expect("preview runs the field phase only");
    assert_eq!(
        preview.players[0].supply.reed, 0,
        "the reed must not arrive during the field phase"
    );
    g.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("A145 must not raise an unresolvable pending");
    assert_eq!(g.public.players[0].supply.reed, 1);
}

/// (b) C066 at grain >= 3 is a CHOICE: the plan flag c066_take_food_instead
/// opts back into the 1 food. GREEN: food 9 (10 - 2 + 1), grain stays 3.
#[test]
fn c066_owner_may_opt_back_into_the_food_at_three_grain() {
    struct TakeFood;
    impl agricola_re_ai::harvest::HarvestDetailPolicy for TakeFood {
        fn plan_feeding(
            &self,
            _: &agricola_re_ai::model::Observation,
        ) -> agricola_re_ai::harvest::FeedingPlan {
            let mut plan = agricola_re_ai::harvest::FeedingPlan::default();
            plan.c066_take_food_instead = true;
            plan
        }
    }
    let mut g = harvest_fixture();
    teleport_into_play(&mut g, &["C066"]);
    grain_fields(&mut g, &[1, 2, 3]);
    g.apply_with_harvest_policy(place_day_laborer(), &TakeFood)
        .expect("C066 food-choice must complete the harvest");
    let p = &g.public.players[0];
    assert_eq!(
        p.supply.food, 9,
        "opting into the food grants it at >=3 grain"
    );
    assert_eq!(p.supply.grain, 3, "no extra grain when the food is chosen");
}

/// (c) A118 extra purchases are capped at 2 per harvest by plan validation:
/// a plan asking for 3 buys is rejected; 2 buys execute as 2 food -> 2 wood.
/// The fixture policy is owner-aware (the observation carries played_cards),
/// so only the A118 holder's plan requests the exchange.
/// GREEN: 2 buys => food 10-2(feed)-2(buy)=6, wood 1(fixed)+2(buy)=3.
#[test]
fn a118_exchange_capped_at_two_per_harvest() {
    struct BuyWoodFlat;
    impl agricola_re_ai::harvest::HarvestDetailPolicy for BuyWoodFlat {
        fn plan_feeding(
            &self,
            obs: &agricola_re_ai::model::Observation,
        ) -> agricola_re_ai::harvest::FeedingPlan {
            let mut plan = agricola_re_ai::harvest::FeedingPlan::default();
            let holder = obs.public.players[obs.player_id]
                .played_cards
                .iter()
                .any(|c| c == "A118");
            if holder {
                plan.a118_wood_buys = 2;
            }
            plan
        }
    }
    struct BuyWoodTooMany;
    impl agricola_re_ai::harvest::HarvestDetailPolicy for BuyWoodTooMany {
        fn plan_feeding(
            &self,
            obs: &agricola_re_ai::model::Observation,
        ) -> agricola_re_ai::harvest::FeedingPlan {
            let mut plan = agricola_re_ai::harvest::FeedingPlan::default();
            let holder = obs.public.players[obs.player_id]
                .played_cards
                .iter()
                .any(|c| c == "A118");
            if holder {
                plan.a118_wood_buys = 3; // 「追加で木材2まで」 exceeds the printed cap
            }
            plan
        }
    }
    let mut g = harvest_fixture();
    teleport_into_play(&mut g, &["A118"]);
    g.public.players[0].supply.food = 10;
    let capped = g.apply_with_harvest_policy(place_day_laborer(), &BuyWoodTooMany);
    assert!(
        capped.is_err(),
        "3 buys must be rejected: {:?}",
        capped.unwrap_err()
    );
    assert!(!g.has_pending(), "the rejected plan leaves no pending");
    let p = &g.public.players[0];
    assert_eq!(
        p.supply.wood, 0,
        "nothing granted while the plan was rejected"
    );
    assert_eq!(p.supply.food, 10);
    let mut g = harvest_fixture();
    teleport_into_play(&mut g, &["A118"]);
    g.public.players[0].supply.food = 10;
    g.apply_with_harvest_policy(place_day_laborer(), &BuyWoodFlat)
        .expect("2 buys must be legal for an A118 holder with food");
    let p = &g.public.players[0];
    assert_eq!(p.supply.wood, 3, "fixed 1 + 2 purchased wood");
    assert_eq!(p.supply.food, 6, "10 - 2 feeding - 2 purchases");
}

/// (e) A145 is 3+/4-player only (spec eligibility 2=no): it must not be dealt
/// in the two-player verified preset at all.
#[test]
fn a145_is_excluded_from_the_two_player_preset() {
    let set: CardSet = verified_basic_card_set(2).unwrap();
    assert!(
        set.definition("A145").is_none(),
        "A145 (eligibility 2=no) must not be in the 2-player preset"
    );
}

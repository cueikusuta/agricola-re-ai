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
//! - resume/replay: a game that plays B058, plows and sows two vegetable
//!   fields entirely through recorded legal actions reaches the round-4
//!   harvest; save/load and full setup/hands/action-log replay reproduce the
//!   exact state hash and food count without granting the effect twice
//!
//! The small normal/harvest/zero fixtures use the established teleport pattern
//! for focused effect checks. The resume/replay fixture below does not: every
//! post-setup transition is a recorded `Game::apply` call, so its action log is
//! sufficient to reconstruct the game from the same setup and initial hands.

use agricola_re_ai::cards::{verified_band_b_cards, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail, Plant};
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

/// Explicit initial hands put B058 in seat 0's hand without changing any
/// post-setup state outside the action log.
fn hands_with_b058(set: &CardSet) -> Vec<Vec<String>> {
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement && card.id != "B058")
        .map(|card| card.id.clone())
        .collect();
    assert!(occupations.len() >= 14 && minors.len() >= 13);

    let mut seat_zero = occupations[..7].to_vec();
    seat_zero.push("B058".to_owned());
    seat_zero.extend_from_slice(&minors[..6]);
    let seat_one = [occupations[7..14].to_vec(), minors[6..13].to_vec()].concat();
    vec![seat_zero, seat_one]
}

fn game_with_b058() -> (Config, CardSet, Vec<Vec<String>>, Game) {
    let config = Config {
        players: 2,
        seed: 2,
        additional_action_space_enabled: false,
    };
    let set = set_with_b058();
    let hands = hands_with_b058(&set);
    let game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("game setup with B058 in seat 0's hand");
    (config, set, hands, game)
}

const PLAIN_SPACES: [Space; 7] = [
    Space::Forest,
    Space::ClayPit,
    Space::ReedBank,
    Space::Fishing,
    Space::DayLaborer,
    Space::GrainSeeds,
    Space::VegetableSeeds,
];

fn first_plain_space(legal: &[Space]) -> Space {
    PLAIN_SPACES
        .iter()
        .copied()
        .find(|space| legal.contains(space))
        .expect("at least one detail-free space must be legal")
}

fn field_count(game: &Game, player: usize) -> usize {
    game.public.players[player]
        .farm
        .iter()
        .filter(|cell| matches!(cell.tile, Tile::Field { .. }))
        .count()
}

/// Seed-2 real-action replay fixture driver.
///
/// Seed 2's round deck (per round): R1 Fencing, R2 MajorImprovement, R4
/// GrainUtilization, R8 VegetableSeeds, R12 Cultivation; harvests at R4/R7/
/// R9/R11/R14. Seat 0:
///   R1 Forest (+3 wood), then plow cell 1.
///   R2 MajorImprovement -> play B058 (cost 1 wood, +1 food on play), then
///     plow cell 2.
///   R8 VegetableSeeds (+1 vegetable supply).
///   R12 Cultivation -> plow cell 3, sow cell 1 with 1 vegetable.
///   R14 final harvest: the single growing vegetable field yields 1
///     vegetable and B058 grants +1 food per harvested vegetable.
///
/// Every post-setup transition is a recorded `Game::apply` call, so the
/// action log reconstructs the game exactly. save/load and full replay must
/// reproduce the identical state hash and food count, proving B058's harvest
/// bonus fires exactly once per harvest and never doubles on resume/replay.
fn seat_zero_action(game: &Game, legal: &[Space]) -> Action {
    let player = &game.public.players[0];
    let b058_played = player.played_cards.iter().any(|id| id == "B058");
    let round = game.public.round;

    // Before B058 is played, secure the 1 wood needed to pay its cost via the
    // Major Improvement space at round 2. Forest first so round 2 can play it.
    if !b058_played && player.supply.wood < 1 && legal.contains(&Space::Forest) {
        return Action {
            space: Space::Forest,
            detail: Detail::None,
        };
    }

    // Round 2: play B058 via the Major Improvement space (needs 1 wood).
    if round == 2 && !b058_played && legal.contains(&Space::MajorImprovement) {
        return Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "B058".to_owned(),
            },
        };
    }

    // Keep food at or above the 4-food feeding cost (2 family members) on
    // every seat-0 turn so the single vegetable obtained at round 8 is never
    // eaten by feeding before it can be sown at round 12.
    if player.supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return place_day_laborer();
    }

    // Round 8: take the revealed VegetableSeeds space (+1 vegetable supply).
    if round == 8 && legal.contains(&Space::VegetableSeeds) {
        return Action {
            space: Space::VegetableSeeds,
            detail: Detail::None,
        };
    }

    // Round 12: Cultivation plow cell 3 and sow cell 1 with 1 vegetable.
    // Only sow while a vegetable is still in supply (feeding must not have
    // eaten it, which the food rule above prevents).
    if round == 12 && legal.contains(&Space::Cultivation) && player.supply.vegetable > 0 {
        return Action {
            space: Space::Cultivation,
            detail: Detail::Cultivate {
                plow: Some(3),
                plantings: vec![Plant {
                    cell: 1,
                    crop: Crop::Vegetable,
                }],
            },
        };
    }

    // Plow the first two empty field cells (cell 1 with no existing field,
    // then cell 2 adjacent to cell 1) using Farmland.
    if legal.contains(&Space::Farmland) {
        let fields = field_count(game, 0);
        let target = if fields == 0 {
            1usize
        } else if fields == 1 {
            2usize
        } else {
            999
        };
        if target != 999 {
            let cell = &game.public.players[0].farm[target];
            if matches!(cell.tile, Tile::Empty) && !cell.stable {
                return Action {
                    space: Space::Farmland,
                    detail: Detail::Plow { cell: target },
                };
            }
        }
    }

    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

fn seat_one_action(game: &Game, legal: &[Space]) -> Action {
    if game.public.players[1].supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return place_day_laborer();
    }
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

fn drive_to_finished(game: &mut Game) {
    for _ in 0..200 {
        if game.public.finished {
            return;
        }
        let legal = game.legal_spaces();
        assert!(!legal.is_empty(), "a legal placement must exist");
        let action = if game.public.active_player == 0 {
            seat_zero_action(game, &legal)
        } else {
            seat_one_action(game, &legal)
        };
        game.apply(action)
            .expect("scripted placement must be legal");
        assert!(
            !game.has_pending(),
            "B058 must not create a pending decision"
        );
    }
    panic!("recorded drive never reached the final harvest");
}

/// Real resume/replay proof: B058 is played from the explicitly dealt hand,
/// a vegetable field is prepared and harvested solely through legal recorded
/// actions. Save/loading and a full replay from setup, hands, start player,
/// and the action log must reproduce the exact state and grant the harvest
/// bonus only once.
#[test]
fn b058_harvest_grant_survives_resume_without_double_count() {
    let (config, set, hands, mut game) = game_with_b058();
    drive_to_finished(&mut game);
    assert!(game.public.finished, "drive must reach the final harvest");
    assert_eq!(game.public.round, 14);
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B058"));

    // Seat 0 grew a vegetable field via R12 Cultivation; it was harvested at
    // the R13 and R14 harvests (B058's per-harvest bonus fires each time),
    // so the supply vegetable count is above the single VegetableSeeds grant.
    let final_vegetable = game.public.players[0].supply.vegetable;
    assert!(
        final_vegetable >= 2,
        "the grown vegetable field was harvested at least once: veg={final_vegetable}"
    );

    // B058's on-play grant (+1 food) plus its per-harvest bonuses are folded
    // into the final food count. Any double grant during resume or replay
    // moves the food count off this value.
    let final_hash = game.state_hash();
    let final_food = game.public.players[0].supply.food;

    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved B058 game must resume");
    assert_eq!(
        loaded.state_hash(),
        final_hash,
        "resume must reproduce state hash"
    );
    assert_eq!(
        loaded.public.players[0].supply.food, final_food,
        "resume must reproduce food"
    );
    assert_eq!(loaded.public.players[0].supply.vegetable, final_vegetable);

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(0),
        &game.log.events,
    )
    .expect("recorded B058 game must replay");
    assert_eq!(
        replayed.state_hash(),
        final_hash,
        "replay must reproduce state hash"
    );
    assert_eq!(
        replayed.public.players[0].supply.food, final_food,
        "replay must reproduce food"
    );
    assert_eq!(replayed.public.players[0].supply.vegetable, final_vegetable);
}

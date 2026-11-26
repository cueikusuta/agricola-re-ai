//! A068 アスパラガスの贈り物 (Asparagus Gift): after the owner uses Fencing,
//! if the number of fences standing AFTER the action resolved is at least the
//! current round, gain 1 vegetable.
//!
//! Dispatcher cases mirror `data/bga-card-tests/A068.json`
//! (A068-normal / A068-boundary / A068-not-triggered). The engine case drives
//! `Game::apply`, captures the pre-action fence count (0), fences the same
//! action (0 -> 4), and pins save/load/replay so the grant is never doubled on
//! replay.

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{
    can_play, verified_band_a_cards, verified_basic_card_set, CardDefinition, CardKind, CardSet,
};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Crop, Game, Player, RoundCard, Space, Tile};
use std::collections::{BTreeMap, BTreeSet};

fn set_with_a068() -> CardSet {
    CardSet {
        definitions: verified_band_a_cards()
            .into_iter()
            .filter(|card| card.id == "A068")
            .collect(),
    }
}

fn a068_definition() -> CardDefinition {
    set_with_a068()
        .definitions
        .into_iter()
        .next()
        .expect("A068 must exist in band A")
}

fn player_with_fences(fences: usize) -> Player {
    let mut player = Player::new(0, 0);
    player.fences.extend((0..fences).map(|i| (i as u8, 0u8)));
    player
}

fn dispatch(
    set: &CardSet,
    played_by: &[usize],
    actor: usize,
    space: Space,
    round: u8,
    players: Vec<Player>,
) -> Vec<Player> {
    let mut cards = Default::default();
    let mut players = players;
    for &owner in played_by {
        players[owner].played_cards.push("A068".to_string());
    }
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 100;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    let used = BTreeSet::new();
    let accumulations = BTreeMap::new();
    let animal_accumulations = BTreeMap::new();
    dispatch_after_worker(
        set,
        actor,
        space,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        round,
        &used,
        &accumulations,
        &animal_accumulations,
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        false,
    )
    .expect("dispatch_after_worker must succeed");
    players
}

fn field_cell(tile: Tile) -> Player {
    let mut player = Player::new(0, 0);
    player.farm[1].tile = tile;
    player
}

#[test]
fn a068_normal_round4_with_4_fences_grants_one_vegetable() {
    // A068-normal: round 4, fences >= round -> vegetable +1.
    let set = set_with_a068();
    let players = vec![player_with_fences(4), Player::new(1, 0)];
    let players = dispatch(&set, &[0], 0, Space::Fencing, 4, players);
    assert_eq!(
        players[0].supply.vegetable, 1,
        "round 4 with 4 fences must grant 1 vegetable"
    );
    assert_eq!(players[1].supply.vegetable, 0, "non-owner receives nothing");
}

#[test]
fn a068_boundary_round5_with_5_fences_grants_one_vegetable() {
    // A068-boundary: round 5, fences == round -> vegetable +1.
    let set = set_with_a068();
    let players = vec![player_with_fences(5)];
    let players = dispatch(&set, &[0], 0, Space::Fencing, 5, players);
    assert_eq!(
        players[0].supply.vegetable, 1,
        "round == fences is the printed >= boundary"
    );
}

#[test]
fn a068_not_triggered_round5_with_4_fences_grants_nothing() {
    // A068-not-triggered: round 5, fences < round -> vegetable +0.
    let set = set_with_a068();
    let players = vec![player_with_fences(4)];
    let players = dispatch(&set, &[0], 0, Space::Fencing, 5, players);
    assert_eq!(
        players[0].supply.vegetable, 0,
        "round > fences must not fire"
    );
}

#[test]
fn a068_does_not_fire_on_non_fencing_spaces() {
    // Fencing is the only trigger space.
    let set = set_with_a068();
    for space in [Space::Forest, Space::DayLaborer, Space::GrainSeeds] {
        let players = vec![player_with_fences(4)];
        let players = dispatch(&set, &[0], 0, space, 4, players);
        assert_eq!(
            players[0].supply.vegetable, 0,
            "{space:?} must not trigger A068"
        );
    }
}

#[test]
fn a068_only_the_actor_owner_benefits() {
    // Both players own A068, but only the actor's played cards are scanned.
    let set = set_with_a068();
    let players = vec![player_with_fences(4), player_with_fences(0)];
    let players = dispatch(&set, &[0, 1], 1, Space::Fencing, 4, players);
    assert_eq!(
        players[0].supply.vegetable, 0,
        "non-actor owner gets nothing"
    );
    assert_eq!(
        players[1].supply.vegetable, 0,
        "actor without the qualifying fences gets nothing"
    );
}

#[test]
fn a068_play_precondition_requires_truly_empty_fields() {
    let set = set_with_a068();
    let card = a068_definition();

    // No field: cannot play.
    let bare = Player::new(0, 0);
    assert!(
        !can_play(&set, &card, &bare, 0),
        "no empty field must block play"
    );

    // Empty field: playable.
    let empty = field_cell(Tile::Field {
        crop: None,
        remaining: 0,
    });
    assert!(
        can_play(&set, &card, &empty, 0),
        "one empty field is the printed precondition"
    );

    // C006 stone field (field_stones > 0): not empty.
    let stone = field_cell(Tile::Field {
        crop: None,
        remaining: 0,
    });
    let mut stone = stone;
    stone.farm[1].field_stones = 1;
    assert!(
        !can_play(&set, &card, &stone, 0),
        "a stone field is not an empty field"
    );

    // Sowon field: not empty.
    let sown = field_cell(Tile::Field {
        crop: Some(Crop::Vegetable),
        remaining: 1,
    });
    assert!(
        !can_play(&set, &card, &sown, 0),
        "a sown field is not an empty field"
    );
}

fn game_with_a068() -> Game {
    let mut defs: Vec<CardDefinition> = verified_basic_card_set(2)
        .unwrap()
        .definitions
        .into_iter()
        .filter(|c| c.id != "A066")
        .collect();
    let occs: Vec<String> = defs
        .iter()
        .filter(|c| matches!(c.kind, CardKind::Occupation))
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = defs
        .iter()
        .filter(|c| matches!(c.kind, CardKind::MinorImprovement) && c.id != "A066")
        .map(|c| c.id.clone())
        .collect();
    defs.push(a068_definition());
    let card_set = CardSet { definitions: defs };
    let mut hands = vec![occs[..7].to_vec(), occs[7..14].to_vec()];
    hands[0].push("A068".to_string());
    hands[0].extend(minors[0..6].iter().cloned());
    hands[1].extend(minors[7..14].iter().cloned());
    let config = Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: false,
    };
    Game::with_cards_and_hands_with_start_player(config, card_set, hands, 0)
        .expect("a068 engine setup")
}

fn apply0(game: &mut Game, space: Space, detail: Detail) {
    let player = game.public.active_player;
    let round = game.public.round;
    game.apply(Action { space, detail })
        .unwrap_or_else(|e| panic!("round {round} player {player} {space:?} must be legal: {e}"));
}

#[test]
fn a068_engine_post_action_fence_count_triggers_and_replays_once() {
    // Round 1: seat0 gathers the wood needed for a one-cell pasture and plows
    // the empty field required to play A068. Round 2: play A068 and gather the
    // remaining wood. Round 3: Fencing resolves and the standing fence count
    // (0 -> 4) meets round 3.
    let mut game = game_with_a068();

    // Round 1: players alternate seats, so each seat takes two actions.
    apply0(&mut game, Space::Forest, Detail::None); // seat0
    apply0(&mut game, Space::ClayPit, Detail::None); // seat1
    apply0(&mut game, Space::Farmland, Detail::Plow { cell: 1 }); // seat0
    apply0(&mut game, Space::ReedBank, Detail::None); // seat1
    assert_eq!(game.public.round, 2);

    // Round 2: seed 0 reveals Fencing as the second round card, so the space
    // is on the board from here on.
    assert!(game.public.revealed.contains(&RoundCard::Fencing));
    apply0(
        &mut game,
        Space::MeetingPlace,
        Detail::PlayMinor {
            card_id: "A068".to_string(),
        },
    );
    apply0(&mut game, Space::DayLaborer, Detail::None);
    apply0(&mut game, Space::Forest, Detail::None);
    apply0(&mut game, Space::ReedBank, Detail::None); // seat1
    assert_eq!(game.public.round, 3);
    assert_eq!(game.public.players[0].played_cards, &["A068"]);
    assert_eq!(
        game.public.players[0].supply.vegetable, 0,
        "playing A068 itself must not grant the vegetable"
    );

    // The pre-action fence count would not meet round 3.
    let pre_fences = game.public.players[0].fences.len();
    let pre_vegetable = game.public.players[0].supply.vegetable;
    assert_eq!(pre_fences, 0, "fixture must start without fences");
    apply0(
        &mut game,
        Space::Fencing,
        Detail::Fence {
            pastures: vec![vec![14]],
        },
    );
    assert_eq!(
        game.public.players[0].fences.len(),
        4,
        "a single-cell pasture adds four fence segments"
    );
    assert_eq!(
        game.public.players[0].supply.vegetable,
        pre_vegetable + 1,
        "the standing post-action fence count must trigger A068"
    );
    assert_eq!(
        game.public.players[1].supply.vegetable, 0,
        "only the card owner benefits"
    );

    let final_hash = game.state_hash();
    let final_vegetable = game.public.players[0].supply.vegetable;
    let loaded = Game::load_json(&game.save_json().expect("save")).expect("load");
    assert_eq!(loaded.state_hash(), final_hash);
    assert_eq!(loaded.public.players[0].supply.vegetable, final_vegetable);

    let replayed = Game::replay_with_setup_and_start_player(
        loaded.config.clone(),
        (*loaded.card_set).clone(),
        loaded.log.initial_hands.clone(),
        loaded.log.initial_start_player_override,
        &loaded.log.events,
    )
    .expect("logged actions replay");
    assert_eq!(replayed.state_hash(), final_hash);
    assert_eq!(
        replayed.public.players[0].supply.vegetable, final_vegetable,
        "replay must not double-grant A068"
    );
    assert_eq!(replayed.public.players[1].supply.vegetable, 0);
}

#[test]
fn a068_round_card_is_fencing_on_seed0_round2() {
    // seed 0 のラウンド1 revealed は SheepMarket、ラウンド2 に Fencing。
    let game = Game::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: false,
    });
    assert_eq!(game.public.revealed, vec![RoundCard::SheepMarket]);
    let next = game
        .hidden_round_deck
        .last()
        .copied()
        .expect("the stage deck must remain");
    assert_eq!(next, RoundCard::Fencing);
}

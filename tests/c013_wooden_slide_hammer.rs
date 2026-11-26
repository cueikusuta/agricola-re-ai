//! C013 木製スライドハンマー (Wood Slide Hammer): initial-renovation stone
//! discount and direct wooden -> stone house.
//!
//! Fixed seam (see the kanban card body): `PassiveRule::FirstRenovationStoneDiscount`
//! fires only while `player.house == Wood && player.rooms() >= 5` at
//! renovation time; after the renovation the condition no longer holds, so
//! no use-count ledger is kept. `direct_to_stone` permits Wood -> Stone and
//! does not implicitly execute Wood -> Clay -> Stone.

use agricola_re_ai::cards::{play, verified_passive_cards, CardKind};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, House, Player, RoundCard, Space, Tile};

const C013: &str = "C013";

fn new_game() -> Game {
    Game::new(Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    })
}

/// Reveal `HouseRedevelopment` as the current round card so the renovation
/// action space is legal (mirrors `tests/renovation_details.rs`).
fn redevelopment_game() -> Game {
    let mut game = new_game();
    if game.public.revealed[0] != RoundCard::HouseRedevelopment {
        let index = game
            .hidden_round_deck
            .iter()
            .position(|&card| card == RoundCard::HouseRedevelopment)
            .unwrap();
        let original = game.public.revealed[0];
        game.public.revealed[0] = RoundCard::HouseRedevelopment;
        game.hidden_round_deck[index] = original;
    }
    game
}

fn actor(game: &Game) -> usize {
    game.public.active_player
}

fn c013_set() -> agricola_re_ai::cards::CardSet {
    agricola_re_ai::cards::CardSet {
        definitions: verified_passive_cards(),
    }
}

fn play_c013(game: &mut Game) {
    let a = actor(game);
    let p = &mut game.public.players[a];
    p.supply.wood = 1;
    play(
        &c013_set(),
        &mut vec![C013.to_string()],
        p,
        C013,
        CardKind::MinorImprovement,
        1,
        0,
    )
    .unwrap();
    assert_eq!(p.supply.wood, 0, "C013 costs exactly 1 wood");
}

/// Rebuild the farm with `total` rooms (incl. the 2 starting rooms).
fn set_rooms(p: &mut Player, total: usize) {
    assert!((2..=15).contains(&total));
    for cell in &mut p.farm {
        cell.tile = Tile::Empty;
    }
    for cell in p.farm.iter_mut().take(total) {
        cell.tile = Tile::Room;
    }
}

fn renovate_action() -> Action {
    Action {
        space: Space::HouseRedevelopment,
        detail: Detail::Renovate {
            bonus_improvement: None,
            instant_bake: 0,
            pastures: vec![],
        },
    }
}

#[test]
fn c013_cost_is_one_wood() {
    let mut game = redevelopment_game();
    play_c013(&mut game);
}

#[test]
fn c013_five_rooms_first_renovation_goes_stone_with_discount() {
    let mut game = redevelopment_game();
    let a = actor(&game);
    play_c013(&mut game);
    let p = &mut game.public.players[a];
    set_rooms(p, 5);
    p.supply.stone = 10;
    p.supply.reed = 1;
    game.apply(renovate_action()).unwrap();
    let p = &game.public.players[a];
    assert_eq!(p.house, House::Stone, "direct wooden -> stone");
    assert_eq!(p.supply.stone, 10 - 3, "5 rooms - 2 discount = 3 stone");
    assert_eq!(p.supply.reed, 0);
}

#[test]
fn c013_four_rooms_renovates_to_clay_at_full_price() {
    let mut game = redevelopment_game();
    let a = actor(&game);
    play_c013(&mut game);
    let p = &mut game.public.players[a];
    set_rooms(p, 4);
    p.supply.clay = 10;
    p.supply.reed = 1;
    game.apply(renovate_action()).unwrap();
    let p = &game.public.players[a];
    assert_eq!(p.house, House::Clay, "below 5 rooms: no direct-to-stone");
    assert_eq!(p.supply.clay, 10 - 4, "full clay price, no discount");
    assert_eq!(p.supply.reed, 0);
}

#[test]
fn c013_discount_fires_only_for_first_wooden_renovation() {
    let mut game = redevelopment_game();
    let a = actor(&game);
    play_c013(&mut game);
    let p = &mut game.public.players[a];
    set_rooms(p, 5);
    p.supply.stone = 10;
    p.supply.reed = 1;
    game.apply(renovate_action()).unwrap();
    let p = &game.public.players[a];
    assert_eq!(p.house, House::Stone);
    // House is stone now: the wooden-house condition no longer holds, and no
    // further renovation is possible at all.
    assert!(
        game.apply(renovate_action()).is_err(),
        "stone house is final"
    );
}

#[test]
fn c013_clay_house_renovation_keeps_normal_cost() {
    // House already clay (as after a first renovation to clay): with 5 rooms,
    // clay -> stone at full price; C013 does not fire because house is not
    // wood, so the stone discount does not apply.
    let mut game = redevelopment_game();
    let a = actor(&game);
    play_c013(&mut game);
    let p = &mut game.public.players[a];
    p.house = House::Clay;
    set_rooms(p, 5);
    p.supply.stone = 10;
    p.supply.reed = 1;
    game.apply(renovate_action()).unwrap();
    let p = &game.public.players[a];
    assert_eq!(p.house, House::Stone);
    assert_eq!(p.supply.stone, 10 - 5, "full stone price, no C013 discount");
}

#[test]
fn c013_save_load_replay_is_stable() {
    let mut game = redevelopment_game();
    let a = actor(&game);
    play_c013(&mut game);
    let p = &mut game.public.players[a];
    set_rooms(p, 5);
    p.supply.stone = 10;
    p.supply.reed = 1;
    let before = serde_json::to_string(&game).unwrap();
    game.apply(renovate_action()).unwrap();
    let after = serde_json::to_string(&game).unwrap();
    assert_ne!(before, after);

    // Restore from save and re-apply: identical result.
    let mut restored: Game = serde_json::from_str(&before).unwrap();
    restored.apply(renovate_action()).unwrap();
    assert_eq!(
        serde_json::to_string(&game.public).unwrap(),
        serde_json::to_string(&restored.public).unwrap(),
        "save/load/replay must reproduce the engine result"
    );
}

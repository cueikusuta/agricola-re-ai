use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, House, RoundCard, Space};

fn game() -> Game {
    Game::new(Config {
        seed: 51,
        additional_action_space_enabled: false,
    })
}

fn reveal(g: &mut Game, card: RoundCard) {
    if g.public.revealed[0] == card {
        return;
    }
    let at = g.hidden_round_deck.iter().position(|&x| x == card).unwrap();
    let old = g.public.revealed[0];
    g.public.revealed[0] = card;
    g.hidden_round_deck[at] = old;
}

#[test]
fn build_multiple_rooms_uses_correct_material_and_reed() {
    let mut g = game();
    let id = g.public.active_player;
    g.public.players[id].supply.wood = 10;
    g.public.players[id].supply.reed = 4;
    g.apply(Action {
        space: Space::FarmExpansion,
        detail: Detail::Build {
            rooms: vec![1, 2],
            stables: vec![],
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].rooms(), 4);
    assert_eq!(g.public.players[id].supply.wood, 0);
    assert_eq!(g.public.players[id].supply.reed, 0);
}

#[test]
fn exceeding_four_stables_does_not_change_state() {
    let mut g = game();
    let id = g.public.active_player;
    g.public.players[id].supply.wood = 12;
    let original = g.state_hash();
    let result = g.apply(Action {
        space: Space::FarmExpansion,
        detail: Detail::Build {
            rooms: vec![],
            stables: vec![14, 13, 12, 11, 10],
        },
    });
    assert!(result.is_err());
    assert_eq!(original, g.state_hash());
}

#[test]
fn renovation_upgrades_all_rooms_and_costs_one_reed() {
    let mut g = game();
    reveal(&mut g, RoundCard::HouseRedevelopment);
    let id = g.public.active_player;
    g.public.players[id].supply.clay = 2;
    g.public.players[id].supply.reed = 1;
    g.apply(Action {
        space: Space::HouseRedevelopment,
        detail: Detail::Renovate {
            bonus_improvement: None,
            instant_bake: 0,
            pastures: vec![],
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].house, House::Clay);
    assert_eq!(g.public.players[id].supply.clay, 0);
    assert_eq!(g.public.players[id].supply.reed, 0);
}

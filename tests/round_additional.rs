use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Space, Tile};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails};

fn game(enabled: bool) -> Game {
    Game::new(Config {
        seed: 123,
        additional_action_space_enabled: enabled,
    })
}

#[test]
fn copse_accumulation_survives_use_of_other_additional_space() {
    let mut g = game(true);
    assert_eq!(g.public.accumulations.get(&Space::Copse), Some(&1));
    g.apply(Action {
        space: Space::ResourceMarket,
        detail: Detail::None,
    })
    .unwrap();
    assert!(!g.legal_spaces().contains(&Space::Copse));
    while g.public.round == 1 {
        let active = g.public.active_player;
        let view = g.observation(active).unwrap();
        let space = g.legal_spaces()[0];
        g.apply(SimpleDetails.produce(&view, space).unwrap()).unwrap();
    }
    assert_eq!(g.public.round, 2);
    assert_eq!(g.public.accumulations.get(&Space::Copse), Some(&2));
    g.apply(Action {
        space: Space::Copse,
        detail: Detail::None,
    })
    .unwrap();
    assert_eq!(g.public.accumulations.get(&Space::Copse), None);
    assert!(!g.legal_spaces().contains(&Space::ResourceMarket));
}

#[test]
fn additional_mode_is_fixed_and_disabled_by_default() {
    let a = Game::new(Config::default());
    assert!(!a.config.additional_action_space_enabled);
    assert!(!a.legal_spaces().iter().any(|s| s.additional()));
    let mut b = game(true);
    b.public.additional_action_space_enabled = false;
    assert!(b.validate().is_err());
    assert!(b.public.players[0].farm.iter().any(|c| matches!(c.tile, Tile::Room)));
}

#[test]
fn modest_family_growth_requires_round_five_and_a_free_room() {
    let mut g = game(true);
    assert!(!g.legal_spaces().contains(&Space::ModestFamilyGrowth));
    g.public.round = 5;
    assert!(!g.legal_spaces().contains(&Space::ModestFamilyGrowth));
    let active = g.public.active_player;
    g.public.players[active].farm[1].tile = Tile::Room;
    assert!(g.legal_spaces().contains(&Space::ModestFamilyGrowth));
}

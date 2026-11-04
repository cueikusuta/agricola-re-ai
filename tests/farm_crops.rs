use agricola_re_ai::engine::{Action, Detail, Plant};
use agricola_re_ai::model::{Config, Crop, Game, RoundCard, Space, Tile};

fn game() -> Game {
    Game::new(Config {
        seed: 42,
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
fn sow_grain_and_vegetable_exact_crop_stacks() {
    let mut g = game();
    reveal(&mut g, RoundCard::GrainUtilization);
    let id = g.public.active_player;
    let p = &mut g.public.players[id];
    p.farm[13].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    p.farm[14].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    p.supply.grain = 1;
    p.supply.vegetable = 1;
    g.apply(Action {
        space: Space::GrainUtilization,
        detail: Detail::Sow {
            plantings: vec![
                Plant {
                    cell: 13,
                    crop: Crop::Grain,
                },
                Plant {
                    cell: 14,
                    crop: Crop::Vegetable,
                },
            ],
            bake: 0,
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].supply.grain, 0);
    assert_eq!(g.public.players[id].supply.vegetable, 0);
    assert_eq!(
        g.public.players[id].farm[13].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3,
        }
    );
    assert_eq!(
        g.public.players[id].farm[14].tile,
        Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        }
    );
}

#[test]
fn sow_same_field_twice_rejected_atomically() {
    let mut g = game();
    reveal(&mut g, RoundCard::GrainUtilization);
    let id = g.public.active_player;
    g.public.players[id].farm[14].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    g.public.players[id].supply.grain = 2;
    let before = g.state_hash();
    assert!(g
        .apply(Action {
            space: Space::GrainUtilization,
            detail: Detail::Sow {
                plantings: vec![
                    Plant {
                        cell: 14,
                        crop: Crop::Grain
                    },
                    Plant {
                        cell: 14,
                        crop: Crop::Grain
                    },
                ],
                bake: 0,
            },
        })
        .is_err());
    assert_eq!(g.state_hash(), before);
}

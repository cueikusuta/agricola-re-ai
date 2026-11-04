use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, RoundCard, Space};

fn game() -> Game {
    Game::new(Config { players: 2,
        seed: 31,
        additional_action_space_enabled: true,
    })
}

fn reveal_fencing(g: &mut Game) {
    if g.public.revealed.contains(&RoundCard::Fencing) {
        return;
    }
    let at = g
        .hidden_round_deck
        .iter()
        .position(|&x| x == RoundCard::Fencing)
        .unwrap();
    let old = g.public.revealed[0];
    g.public.revealed[0] = RoundCard::Fencing;
    g.hidden_round_deck[at] = old;
}

#[test]
fn invariants_reject_mutually_exclusive_actions_and_unrevealed_spaces() {
    let mut g = game();
    g.public.used.insert(Space::ResourceMarket);
    g.public.used.insert(Space::Copse);
    assert!(g.validate().unwrap_err().contains("mutually-exclusive"));
    g.public.used.clear();
    g.public.used.insert(Space::CattleMarket);
    assert!(g.validate().unwrap_err().contains("non-revealed"));
}

#[test]
fn invariants_check_pasture_boundaries_and_connectedness() {
    let mut g = game();
    reveal_fencing(&mut g);
    let id = g.public.active_player;
    g.public.players[id].supply.wood = 15;
    g.apply(Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![13, 14]],
        },
    })
    .unwrap();
    g.validate().unwrap();
    let edge = *g.public.players[id].fences.iter().next().unwrap();
    g.public.players[id].fences.remove(&edge);
    assert!(g.validate().unwrap_err().contains("boundaries"));
    g.public.players[id].fences.insert(edge);
    g.public.players[id].pastures[0].cells = vec![1, 14];
    assert!(g.validate().unwrap_err().contains("connected"));
}

#[test]
fn invariants_check_duplicate_pasture_ids() {
    let mut g = game();
    reveal_fencing(&mut g);
    let id = g.public.active_player;
    g.public.players[id].supply.wood = 15;
    g.apply(Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![13], vec![14]],
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].pastures.len(), 2);
    g.public.players[id].pastures[1].id = g.public.players[id].pastures[0].id;
    assert!(g.validate().unwrap_err().contains("duplicate pasture"));
}

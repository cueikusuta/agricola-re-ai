use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Improvement, RoundCard, Space};
use std::collections::BTreeSet;

fn game() -> Game {
    let mut g = Game::new(Config {
        seed: 412,
        additional_action_space_enabled: false,
    });
    if g.public.revealed[0] != RoundCard::MajorImprovement {
        let index = g
            .hidden_round_deck
            .iter()
            .position(|&x| x == RoundCard::MajorImprovement)
            .unwrap();
        let old = g.public.revealed[0];
        g.public.revealed[0] = RoundCard::MajorImprovement;
        g.hidden_round_deck[index] = old;
    }
    g
}

#[test]
fn improvement_market_contains_ten_unique_scored_cards() {
    let g = game();
    assert_eq!(g.public.improvement_market.len(), 10);
    let unique: BTreeSet<_> = g.public.improvement_market.iter().copied().collect();
    assert_eq!(unique.len(), 10);
    for improvement in g.public.improvement_market {
        assert!(improvement.points() >= 1);
        assert!(!improvement.cost().is_empty());
    }
}

#[test]
fn buying_a_fireplace_spends_clay_and_depletes_the_market() {
    let mut g = game();
    let id = g.public.active_player;
    g.public.players[id].supply.clay = 2;
    g.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::Buy {
            improvement: Improvement::Fireplace2,
            instant_bake: 0,
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].supply.clay, 0);
    assert!(g.public.players[id]
        .improvements
        .contains(&Improvement::Fireplace2));
    assert!(!g
        .public
        .improvement_market
        .contains(&Improvement::Fireplace2));
}

#[test]
fn exchanging_a_fireplace_for_a_hearth_is_free() {
    let mut g = game();
    let id = g.public.active_player;
    g.public.players[id]
        .improvements
        .push(Improvement::Fireplace2);
    g.public
        .improvement_market
        .retain(|&x| x != Improvement::Fireplace2);
    let initial_clay = g.public.players[id].supply.clay;
    g.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::Buy {
            improvement: Improvement::Hearth4,
            instant_bake: 0,
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].supply.clay, initial_clay);
    assert!(g.public.players[id]
        .improvements
        .contains(&Improvement::Hearth4));
    assert!(!g.public.players[id]
        .improvements
        .contains(&Improvement::Fireplace2));
    assert!(g
        .public
        .improvement_market
        .contains(&Improvement::Fireplace2));
}

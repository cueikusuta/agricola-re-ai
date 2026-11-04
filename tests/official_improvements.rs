//! Official revised-edition appendix p.2: all ten major improvements.
use agricola_re_ai::engine::{Action, Detail, PurchaseMode};
use agricola_re_ai::model::{Config, Game, Good, Improvement, Player, RoundCard, Space};
use agricola_re_ai::score::score;

fn major_game() -> Game {
    let mut g = Game::new(Config {
        seed: 7731,
        additional_action_space_enabled: false,
    });
    if g.public.revealed[0] != RoundCard::MajorImprovement {
        let index = g
            .hidden_round_deck
            .iter()
            .position(|&x| x == RoundCard::MajorImprovement)
            .unwrap();
        let previous = g.public.revealed[0];
        g.public.revealed[0] = RoundCard::MajorImprovement;
        g.hidden_round_deck[index] = previous;
    }
    g
}

#[test]
fn each_major_improvement_uses_its_official_cost_and_score() {
    type MajorCase = (Improvement, &'static [(Good, u32)], i32);
    let cases: [MajorCase; 10] = [
        (Improvement::Fireplace2, &[(Good::Clay, 2)], 1),
        (Improvement::Fireplace3, &[(Good::Clay, 3)], 1),
        (Improvement::Hearth4, &[(Good::Clay, 4)], 1),
        (Improvement::Hearth5, &[(Good::Clay, 5)], 1),
        (
            Improvement::ClayOven,
            &[(Good::Clay, 3), (Good::Stone, 1)],
            2,
        ),
        (
            Improvement::StoneOven,
            &[(Good::Clay, 1), (Good::Stone, 3)],
            3,
        ),
        (Improvement::Well, &[(Good::Wood, 1), (Good::Stone, 3)], 4),
        (
            Improvement::Joinery,
            &[(Good::Wood, 2), (Good::Stone, 2)],
            2,
        ),
        (
            Improvement::Pottery,
            &[(Good::Clay, 2), (Good::Stone, 2)],
            2,
        ),
        (
            Improvement::Basketry,
            &[(Good::Reed, 2), (Good::Stone, 2)],
            2,
        ),
    ];
    for (card, expected_cost, expected_points) in cases {
        assert_eq!(
            card.cost(),
            expected_cost,
            "printed cost mismatch for {card:?}"
        );
        assert_eq!(
            card.points(),
            expected_points,
            "printed points mismatch for {card:?}"
        );
        let mut g = major_game();
        let id = g.public.active_player;
        let p = &mut g.public.players[id];
        p.supply.wood = 12;
        p.supply.clay = 12;
        p.supply.reed = 12;
        p.supply.stone = 12;
        let before = p.supply.clone();
        g.apply(Action {
            space: Space::MajorImprovement,
            detail: Detail::Buy {
                improvement: card,
                instant_bake: 0,
            },
        })
        .unwrap();
        let after = &g.public.players[id];
        assert!(after.improvements.contains(&card));
        assert!(!g.public.improvement_market.contains(&card));
        for good in [Good::Wood, Good::Clay, Good::Reed, Good::Stone] {
            let amount: u32 = expected_cost
                .iter()
                .filter(|(resource, _)| *resource == good)
                .map(|(_, n)| *n)
                .sum();
            assert_eq!(
                after.supply.get(good),
                before.get(good) - amount,
                "incorrect resource spent for {card:?}, {good:?}"
            );
        }
        if card == Improvement::Well {
            assert_eq!(g.delayed_well.len(), 5);
            for round in 2..=6 {
                assert_eq!(g.delayed_well.get(&round), Some(&vec![id]));
            }
        }
    }
}

#[test]
fn buying_hearth_by_paying_cost_keeps_owned_fireplace() {
    let mut g = major_game();
    let id = g.public.active_player;
    g.public.players[id]
        .improvements
        .push(Improvement::Fireplace2);
    g.public
        .improvement_market
        .retain(|&c| c != Improvement::Fireplace2);
    g.public.players[id].supply.clay = 4;
    g.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::BuyWithMode {
            improvement: Improvement::Hearth4,
            instant_baking: vec![],
            mode: PurchaseMode::Pay,
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].supply.clay, 0);
    assert!(g.public.players[id]
        .improvements
        .contains(&Improvement::Fireplace2));
    assert!(g.public.players[id]
        .improvements
        .contains(&Improvement::Hearth4));
    assert!(!g
        .public
        .improvement_market
        .contains(&Improvement::Fireplace2));
}

#[test]
fn selecting_which_fireplace_to_exchange_is_explicit() {
    let mut g = major_game();
    let id = g.public.active_player;
    g.public.players[id]
        .improvements
        .extend([Improvement::Fireplace2, Improvement::Fireplace3]);
    g.public
        .improvement_market
        .retain(|&c| c != Improvement::Fireplace2 && c != Improvement::Fireplace3);
    g.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::BuyWithMode {
            improvement: Improvement::Hearth5,
            instant_baking: vec![],
            mode: PurchaseMode::Exchange(Improvement::Fireplace3),
        },
    })
    .unwrap();
    assert!(g.public.players[id]
        .improvements
        .contains(&Improvement::Fireplace2));
    assert!(!g.public.players[id]
        .improvements
        .contains(&Improvement::Fireplace3));
    assert!(g.public.players[id]
        .improvements
        .contains(&Improvement::Hearth5));
    assert!(g
        .public
        .improvement_market
        .contains(&Improvement::Fireplace3));
    assert!(!g
        .public
        .improvement_market
        .contains(&Improvement::Fireplace2));
}

#[test]
fn illegal_card_exchange_is_atomic() {
    let mut g = major_game();
    let id = g.public.active_player;
    g.public.players[id]
        .improvements
        .push(Improvement::Fireplace2);
    g.public
        .improvement_market
        .retain(|&c| c != Improvement::Fireplace2);
    let hash = g.state_hash();
    assert!(g
        .apply(Action {
            space: Space::MajorImprovement,
            detail: Detail::BuyWithMode {
                improvement: Improvement::Well,
                instant_baking: vec![],
                mode: PurchaseMode::Exchange(Improvement::Fireplace2),
            },
        })
        .is_err());
    assert_eq!(g.state_hash(), hash);
}

#[test]
fn all_three_workshops_award_official_leftover_resource_bonus_tiers() {
    let cases = [
        (
            Improvement::Joinery,
            Good::Wood,
            [(2, 0), (3, 1), (4, 1), (5, 2), (7, 3)],
        ),
        (
            Improvement::Pottery,
            Good::Clay,
            [(2, 0), (3, 1), (4, 1), (5, 2), (7, 3)],
        ),
        (
            Improvement::Basketry,
            Good::Reed,
            [(1, 0), (2, 1), (3, 1), (4, 2), (5, 3)],
        ),
    ];
    for (card, good, tiers) in cases {
        for (amount, expected_bonus) in tiers {
            let mut p = Player::new(0, 2);
            p.improvements.push(card);
            p.supply.set(good, amount);
            assert_eq!(
                score(&p).categories["craft_bonus"],
                expected_bonus,
                "wrong bonus for {card:?} with {amount} {good:?}"
            );
        }
    }
}

//! Official Revised 2016 Appendix regression tests for M1.
use agricola_re_ai::engine::{Action, AnimalPlan, Detail, FreeAction};
use agricola_re_ai::model::{
    Animal, Config, Crop, Game, Good, Improvement, RoundCard, Space, Tile,
};
use agricola_re_ai::score::score;

fn game(enabled: bool) -> Game {
    Game::new(Config {
        players: 2,
        seed: 17,
        additional_action_space_enabled: enabled,
    })
}
fn expose(g: &mut Game, card: RoundCard) {
    if g.public.revealed.contains(&card) {
        return;
    }
    let index = g
        .hidden_round_deck
        .iter()
        .position(|x| *x == card)
        .expect("test card");
    let old = g.public.revealed[0];
    g.hidden_round_deck[index] = old;
    g.public.revealed[0] = card;
}
#[test]
fn field_count_scoring_follows_2016_appendix() {
    let mut p = game(false).public.players.remove(0);
    assert_eq!(score(&p).categories["fields"], -1);
    p.farm[14].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 3,
    };
    assert_eq!(score(&p).categories["fields"], -1);
    p.farm[13].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    assert_eq!(score(&p).categories["fields"], 1);
    p.farm[12].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    assert_eq!(score(&p).categories["fields"], 2);
}
#[test]
fn clay_oven_bakes_one_grain_per_action() {
    let mut g = game(false);
    expose(&mut g, RoundCard::GrainUtilization);
    let id = g.public.active_player;
    g.public.players[id]
        .improvements
        .push(Improvement::ClayOven);
    g.public
        .improvement_market
        .retain(|&card| card != Improvement::ClayOven);
    g.public.players[id].supply.grain = 3;
    let before = g.state_hash();
    assert!(g
        .apply(Action {
            space: Space::GrainUtilization,
            detail: Detail::Sow {
                plantings: vec![],
                bake: 3
            }
        })
        .is_err());
    assert_eq!(g.state_hash(), before);
    g.apply(Action {
        space: Space::GrainUtilization,
        detail: Detail::Sow {
            plantings: vec![],
            bake: 1,
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].supply.grain, 2);
    assert_eq!(
        g.public.players[id].supply.food,
        if id == g.public.start_player {
            2 + 5
        } else {
            3 + 5
        }
    );
}
#[test]
fn stone_oven_bakes_at_most_two_grain() {
    let mut g = game(false);
    expose(&mut g, RoundCard::GrainUtilization);
    let id = g.public.active_player;
    g.public.players[id]
        .improvements
        .push(Improvement::StoneOven);
    g.public
        .improvement_market
        .retain(|&card| card != Improvement::StoneOven);
    g.public.players[id].supply.grain = 3;
    let before = g.public.players[id].supply.food;
    assert!(g
        .apply(Action {
            space: Space::GrainUtilization,
            detail: Detail::Sow {
                plantings: vec![],
                bake: 3
            }
        })
        .is_err());
    g.apply(Action {
        space: Space::GrainUtilization,
        detail: Detail::Sow {
            plantings: vec![],
            bake: 2,
        },
    })
    .unwrap();
    assert_eq!(g.public.players[id].supply.food, before + 8);
}
#[test]
fn vegetable_conversion_uses_cooking_hearth_rate() {
    let mut g = game(false);
    let id = g.public.active_player;
    g.public.players[id].supply.vegetable = 2;
    g.public.players[id].improvements.push(Improvement::Hearth4);
    g.public
        .improvement_market
        .retain(|&card| card != Improvement::Hearth4);
    let before = g.public.players[id].supply.food;
    g.apply_free(
        id,
        FreeAction::ConvertCrop {
            good: Good::Vegetable,
            quantity: 2,
        },
    )
    .unwrap();
    assert_eq!(g.public.players[id].supply.food, before + 6);
    assert_eq!(g.public.players[id].supply.vegetable, 0);
}
#[test]
fn animal_market_cattle_requires_food_and_is_atomic() {
    let mut g = game(true);
    let id = g.public.active_player;
    g.public.players[id].supply.food = 0;
    let plan = AnimalPlan::discard_all(Animal::Cattle, 1);
    let action = Action {
        space: Space::AnimalMarket,
        detail: Detail::Animals(plan.clone()),
    };
    let old = g.state_hash();
    assert!(g.apply(action.clone()).is_err());
    assert_eq!(g.state_hash(), old);
    g.public.players[id].supply.food = 1;
    g.apply(action).unwrap();
    assert_eq!(g.public.players[id].supply.food, 0);
}

#[test]
fn delayed_round_goods_are_collected_at_round_start_and_only_once() {
    use agricola_re_ai::model::DelayedGood;
    let mut g = game(false);
    let wood = g.public.players[0].supply.wood;
    let food = g.public.players[1].supply.food;
    // The next start_round opens round 4.
    g.public.round = 3;
    g.delayed_goods.insert(
        4,
        vec![
            DelayedGood {
                player: 0,
                good: Good::Wood,
                amount: 2,
            },
            DelayedGood {
                player: 1,
                good: Good::Food,
                amount: 1,
            },
        ],
    );

    g.start_round();
    assert_eq!(g.public.round, 4);
    assert_eq!(g.public.players[0].supply.wood, wood + 2);
    assert_eq!(g.public.players[1].supply.food, food + 1);
    assert!(
        g.delayed_goods.is_empty(),
        "a collected reservation must be removed"
    );

    let wood_after = g.public.players[0].supply.wood;
    g.start_round();
    assert_eq!(g.public.round, 5);
    assert_eq!(
        g.public.players[0].supply.wood, wood_after,
        "the reserved good is granted once, at its own round start"
    );
}

#[test]
fn empty_delayed_goods_ledger_is_skipped_in_serialization() {
    let g = game(false);
    let json = serde_json::to_string(&g).unwrap();
    assert!(
        !json.contains("delayed_goods"),
        "pre-existing saves must keep their exact bytes"
    );
}

#[test]
fn empty_delayed_goods_ledger_keeps_the_pre_ch8_integrity_stamp() {
    // Saves written before C074/C077/C078/C079/C118 existed must keep loading:
    // while the ledger is empty the integrity tuple is byte-for-byte the old one.
    use sha2::{Digest, Sha256};
    let g = game(false);
    let legacy = serde_json::to_vec(&(
        g.state_hash(),
        &g.well_food_schedule,
        &g.pending,
        g.event_budget,
        &g.card_uses,
        &g.card_use_limits,
        g.next_decision_id,
    ))
    .unwrap();
    assert_eq!(
        g.integrity_hash(),
        format!("{:x}", Sha256::digest(&legacy)),
        "an empty ledger must not change the legacy integrity stamp"
    );
}

use agricola_re_ai::cards::{verified_band_b_cards, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::model::{Config, Crop, Game, Space, Tile};
use std::collections::BTreeSet;

fn set_with_b058() -> CardSet {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.push(
        verified_band_b_cards()
            .into_iter()
            .find(|c| c.id == "B058")
            .unwrap(),
    );
    set
}

#[test]
fn debug_post_harvest_state() {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_b058();
    let mut game = Game::with_cards(config.clone(), set.clone()).unwrap();
    while game.public.round < 4 {
        game.start_round().unwrap();
    }
    game.public.used = BTreeSet::from([Space::Farmland]);
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[0] -= 1;
    game.public.active_player = 0;
    game.public.players[0].supply.food = 10;
    game.public.players[1].supply.food = 10;
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "B058");
    }
    game.cards.undealt.retain(|id| id != "B058");
    game.public.players[0].played_cards.push("B058".to_owned());
    for &i in &[1usize, 2] {
        game.public.players[0].farm[i].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
    }
    game.public.players[0].supply.vegetable = 3;
    eprintln!("start_active={}", game.public.active_player);
    eprintln!(
        "round_before={} quota={:?} placed={:?}",
        game.public.round, game.public.work_quota, game.public.workers_placed
    );
    let res = game.apply_with_harvest_policy(
        Action {
            space: Space::DayLaborer,
            detail: Detail::None,
        },
        &SimpleHarvestDetails,
    );
    eprintln!("apply={:?}", res);
    eprintln!(
        "round_after={} finished={}",
        game.public.round, game.public.finished
    );
    eprintln!(
        "p0 food={} veg={} grain={} beg={}",
        game.public.players[0].supply.food,
        game.public.players[0].supply.vegetable,
        game.public.players[0].supply.grain,
        game.public.players[0].begging
    );
    eprintln!("p1 food={}", game.public.players[1].supply.food);
    eprintln!("active_after={}", game.public.active_player);
    let legal = game.legal_spaces();
    eprintln!("legal_after={:?}", legal);
    eprintln!(
        "log_len={} kinds={:?}",
        game.log.events.len(),
        game.log
            .events
            .iter()
            .map(|e| e.kind.clone())
            .collect::<Vec<_>>()
    );
}

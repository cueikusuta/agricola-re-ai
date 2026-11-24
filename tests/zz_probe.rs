use agricola_re_ai::cards::verified_band_b_cards;
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Space};

#[test]
fn probe_initial_supply_and_forest() {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.extend(
        verified_band_b_cards()
            .into_iter()
            .filter(|c| c.id == "B059"),
    );
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let mut game = Game::with_cards(config, set).unwrap();
    game.start_round().unwrap();
    let p = &game.public.players[0];
    println!(
        "PROBE initial: wood={} food={} clay={} reed={} grain={} veg={}",
        p.supply.wood,
        p.supply.food,
        p.supply.clay,
        p.supply.reed,
        p.supply.grain,
        p.supply.vegetable
    );
    println!("PROBE legal: {:?}", game.legal_spaces());
    game.apply(Action {
        space: Space::Forest,
        detail: Detail::None,
    })
    .unwrap();
    let p = &game.public.players[0];
    println!(
        "PROBE after forest: wood={} food={}",
        p.supply.wood, p.supply.food
    );
}

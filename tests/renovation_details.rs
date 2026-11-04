use agricola_re_ai::engine::{Action, Detail, PastureOccupancy, PurchaseMode};
use agricola_re_ai::model::{
    Animal, Config, Game, Good, House, Improvement, Pasture, RoundCard, Space, Tile,
};
use std::collections::BTreeSet;

fn revealed(target: RoundCard, additional: bool) -> Game {
    let mut game = Game::new(Config { players: 2,
        seed: 8301,
        additional_action_space_enabled: additional,
    });
    if game.public.revealed[0] != target {
        let index = game
            .hidden_round_deck
            .iter()
            .position(|&card| card == target)
            .unwrap();
        let original = game.public.revealed[0];
        game.public.revealed[0] = target;
        game.hidden_round_deck[index] = original;
    }
    game
}

#[test]
fn house_renovation_can_pay_for_hearth_instead_of_exchanging_owned_fireplace() {
    for additional in [false, true] {
        let mut game = revealed(RoundCard::HouseRedevelopment, additional);
        let actor = game.public.active_player;
        let player = &mut game.public.players[actor];
        player.supply.clay = 7;
        player.supply.reed = 1;
        player.improvements.push(Improvement::Fireplace2);
        game.public
            .improvement_market
            .retain(|&i| i != Improvement::Fireplace2);
        game.apply(Action {
            space: Space::HouseRedevelopment,
            detail: Detail::RenovateDetailed {
                bonus_improvement: Some(Improvement::Hearth4),
                instant_baking: vec![],
                purchase_mode: PurchaseMode::Pay,
                pastures: vec![],
                occupancy: None,
            },
        })
        .unwrap();
        let player = &game.public.players[actor];
        assert_eq!(player.house, House::Clay);
        assert_eq!(player.supply.clay, 1);
        assert!(player.improvements.contains(&Improvement::Fireplace2));
        assert!(player.improvements.contains(&Improvement::Hearth4));
    }
}

#[test]
fn farm_renovation_can_split_an_occupied_pasture_with_explicit_conserving_allocation() {
    for additional in [false, true] {
        let mut game = revealed(RoundCard::FarmRedevelopment, additional);
        let actor = game.public.active_player;
        let player = &mut game.public.players[actor];
        player.supply.clay = 2;
        player.supply.reed = 1;
        player.supply.wood = 1;
        player.farm[13].tile = Tile::Pasture { id: 0 };
        player.farm[14].tile = Tile::Pasture { id: 0 };
        player.pastures = vec![Pasture {
            id: 0,
            cells: vec![13, 14],
            animal: Some(Animal::Sheep),
            quantity: 3,
        }];
        player.fences =
            BTreeSet::from([(15, 16), (16, 17), (15, 21), (17, 23), (21, 22), (22, 23)]);
        game.validate().unwrap();
        game.apply(Action {
            space: Space::FarmRedevelopment,
            detail: Detail::RenovateDetailed {
                bonus_improvement: None,
                instant_baking: vec![],
                purchase_mode: PurchaseMode::Auto,
                pastures: vec![vec![13], vec![14]],
                occupancy: Some(vec![
                    PastureOccupancy {
                        animal: Some(Animal::Sheep),
                        quantity: 2,
                    },
                    PastureOccupancy {
                        animal: Some(Animal::Sheep),
                        quantity: 1,
                    },
                ]),
            },
        })
        .unwrap();
        let player = &game.public.players[actor];
        assert_eq!(player.house, House::Clay);
        assert_eq!(player.supply.get(Good::Clay), 0);
        assert_eq!(player.supply.wood, 0);
        assert_eq!(player.pastures.len(), 2);
        assert_eq!(player.animal_count(Animal::Sheep), 3);
        assert_eq!(player.fences.len(), 7);
    }
}

#[test]
fn invalid_redevelopment_followup_does_not_spend_renovation_resources() {
    let mut game = revealed(RoundCard::FarmRedevelopment, false);
    let actor = game.public.active_player;
    game.public.players[actor].supply.clay = 8;
    game.public.players[actor].supply.reed = 1;
    let before = game.state_hash();
    assert!(game
        .apply(Action {
            space: Space::FarmRedevelopment,
            detail: Detail::RenovateDetailed {
                bonus_improvement: Some(Improvement::Hearth4),
                instant_baking: vec![],
                purchase_mode: PurchaseMode::Pay,
                pastures: vec![],
                occupancy: None,
            },
        })
        .is_err());
    assert_eq!(game.state_hash(), before);

    let mut game = revealed(RoundCard::HouseRedevelopment, false);
    let actor = game.public.active_player;
    game.public.players[actor].supply.clay = 2;
    game.public.players[actor].supply.reed = 1;
    let before = game.state_hash();
    assert!(game
        .apply(Action {
            space: Space::HouseRedevelopment,
            detail: Detail::RenovateDetailed {
                bonus_improvement: None,
                instant_baking: vec![],
                purchase_mode: PurchaseMode::Auto,
                pastures: vec![vec![13]],
                occupancy: Some(vec![PastureOccupancy {
                    animal: None,
                    quantity: 0
                }]),
            },
        })
        .is_err());
    assert_eq!(game.state_hash(), before);
}

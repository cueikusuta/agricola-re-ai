use agricola_re_ai::engine::{Action, AnimalPlan, Detail, FreeAction};
use agricola_re_ai::model::{Animal, Config, Game, RoundCard, Space};

fn game(enabled: bool) -> Game {
    Game::new(Config { players: 2,
        seed: 111,
        additional_action_space_enabled: enabled,
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
fn livestock_in_an_unfenced_stable_moves_into_the_pasture() {
    let mut g = game(false);
    reveal_fencing(&mut g);
    let id = g.public.active_player;
    g.public.players[id].supply.wood = 15;
    g.public.players[id].farm[14].stable = true;
    g.public.players[id].farm[14].stable_animal = Some(Animal::Sheep);
    g.apply(Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![14]],
        },
    })
    .unwrap();
    let p = &g.public.players[id];
    assert_eq!(p.animal_count(Animal::Sheep), 1);
    assert_eq!(p.pasture_capacity(&p.pastures[0]), 4);
    assert_eq!(p.pastures[0].quantity, 1);
    assert_eq!(p.farm[14].stable_animal, None);
    g.validate().unwrap();
}

#[test]
fn animal_market_sheep_and_food_can_be_held_as_pet() {
    let mut g = game(true);
    let id = g.public.active_player;
    let original_food = g.public.players[id].supply.food;
    g.apply(Action {
        space: Space::AnimalMarket,
        detail: Detail::Animals(AnimalPlan {
            animal: Animal::Sheep,
            to_pastures: vec![],
            to_stables: vec![],
            pet: true,
            cook: 0,
            discard: 0,
        }),
    })
    .unwrap();
    assert_eq!(g.public.players[id].pet, Some(Animal::Sheep));
    assert_eq!(g.public.players[id].supply.food, original_food + 1);
}

#[test]
fn rearranging_livestock_cannot_create_new_animals() {
    let mut g = game(false);
    let id = g.public.active_player;
    g.public.players[id].pet = Some(Animal::Sheep);
    let before = g.state_hash();
    assert!(g
        .apply_free(
            id,
            FreeAction::RearrangeAnimals {
                pasture: vec![],
                stables: vec![],
                pet: None,
            }
        )
        .is_err());
    assert_eq!(g.state_hash(), before);
}

#[test]
fn fencing_rejects_a_no_op_without_spending_a_worker() {
    for enabled in [false, true] {
        let mut g = game(enabled);
        reveal_fencing(&mut g);
        let id = g.public.active_player;
        g.public.players[id].supply.wood = 8;
        let pasture = vec![vec![14]];
        g.apply(Action {
            space: Space::Fencing,
            detail: Detail::Fence {
                pastures: pasture.clone(),
            },
        })
        .unwrap();

        // Allow another fencing turn in this fixed test without advancing rounds.
        g.public.used.remove(&Space::Fencing);
        g.public.workers_placed[id] -= 1;
        g.public.active_player = id;
        g.validate().unwrap();

        let before = g.state_hash();
        let err = g
            .apply(Action {
                space: Space::Fencing,
                detail: Detail::Fence { pastures: pasture },
            })
            .unwrap_err();
        assert!(err.contains("at least one new fence"), "{err}");
        assert_eq!(g.state_hash(), before, "a rejected action must be atomic");
    }
}

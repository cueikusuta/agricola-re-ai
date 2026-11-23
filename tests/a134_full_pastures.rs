//! A134 満載畜産家 (Full Farmer) — single-card chapter tests.
//!
//! Printed effect: on play gain 1 wood and 1 clay; at end of game score
//! 1 point per pasture filled to capacity. Eligible for 3/4 players only
//! (spec eligibility "2": "no"); printed points 0. The end-game bonus is
//! read from the live final state and is never added to the printed
//! `card_points`, so a resume/replay cannot double-count it.
use agricola_re_ai::cards::{
    end_game_bonus, play as play_card, verified_band_a_cards, CardKind, CardSet,
};
use agricola_re_ai::model::{Animal, Good, Pasture, Player};
use agricola_re_ai::registry::{build_default_registry, CardPool};

/// A card set carrying A134's real definition and nothing else that scores,
/// so `end_game_bonus` reflects only A134.
fn a134_set() -> CardSet {
    CardSet {
        definitions: verified_band_a_cards()
            .into_iter()
            .filter(|card| card.id == "A134")
            .collect(),
    }
}

/// A player who has played A134, ready for an end-game bonus computation.
fn a134_owner(edit: impl FnOnce(&mut Player)) -> (CardSet, Player) {
    let set = a134_set();
    let mut player = Player::new(0, 0);
    player.played_cards.push("A134".to_owned());
    edit(&mut player);
    (set, player)
}

/// Build a pasture over `cells` (no stables → capacity 2×cells) holding
/// exactly its capacity, i.e. a full pasture.
fn full_pasture(id: u8, cells: Vec<usize>) -> Pasture {
    let quantity = 2 * cells.len() as u32;
    Pasture {
        id,
        cells,
        animal: Some(Animal::Sheep),
        quantity,
    }
}

#[test]
fn a134_normal_grants_wood_and_clay_on_play() {
    let set = a134_set();
    let mut player = Player::new(0, 0);
    let mut hand = vec!["A134".to_owned()];
    let wood_before = player.supply.get(Good::Wood);
    let clay_before = player.supply.get(Good::Clay);
    play_card(
        &set,
        &mut hand,
        &mut player,
        "A134",
        CardKind::Occupation,
        0,
    )
    .expect("A134 must be playable");
    assert_eq!(player.supply.get(Good::Wood), wood_before + 1);
    assert_eq!(player.supply.get(Good::Clay), clay_before + 1);
    assert_eq!(hand, Vec::<String>::new(), "card moved to played");
    assert_eq!(player.played_cards, vec!["A134".to_owned()]);
    assert_eq!(player.card_points, 0, "printed points stay 0");
}

#[test]
fn a134_scores_one_point_per_full_pasture() {
    let (set, one) = a134_owner(|p| {
        p.pastures.push(full_pasture(0, vec![0, 1]));
    });
    assert_eq!(end_game_bonus(&set, &one), 1);

    let (set, two) = a134_owner(|p| {
        p.pastures.push(full_pasture(0, vec![0, 1]));
        p.pastures.push(full_pasture(1, vec![2, 3]));
    });
    assert_eq!(end_game_bonus(&set, &two), 2);
}

#[test]
fn a134_zero_when_no_pasture_is_full() {
    // Empty pasture (quantity 0).
    let (set, empty) = a134_owner(|p| {
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![0, 1],
            animal: None,
            quantity: 0,
        });
    });
    assert_eq!(end_game_bonus(&set, &empty), 0);

    // Under-filled pasture (capacity 4, holding 3).
    let (set, under) = a134_owner(|p| {
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![0, 1],
            animal: Some(Animal::Sheep),
            quantity: 3,
        });
    });
    assert_eq!(end_game_bonus(&set, &under), 0);

    // Pet + lone stable, but no pasture: neither is a pasture.
    let (set, pet_stable) = a134_owner(|p| {
        p.pet = Some(Animal::Sheep);
        p.farm[4].stable = true;
    });
    assert_eq!(end_game_bonus(&set, &pet_stable), 0);
}

#[test]
fn a134_full_judgment_reuses_pasture_capacity_with_stables() {
    // Two fenced cells + one stable → pasture_capacity doubles to 8. A farm
    // holding 8 is full (bonus 1); holding 7 is not. This proves the rule
    // reads `Player::pasture_capacity` (stable-aware) rather than any
    // hardcoded cell formula.
    let (set, full) = a134_owner(|p| {
        p.farm[0].stable = true;
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![0, 1],
            animal: Some(Animal::Sheep),
            quantity: 8,
        });
    });
    assert_eq!(full.pasture_capacity(&full.pastures[0]), 8);
    assert_eq!(end_game_bonus(&set, &full), 1);

    let (set, near) = a134_owner(|p| {
        p.farm[0].stable = true;
        p.pastures.push(Pasture {
            id: 0,
            cells: vec![0, 1],
            animal: Some(Animal::Sheep),
            quantity: 7,
        });
    });
    assert_eq!(near.pasture_capacity(&near.pastures[0]), 8);
    assert_eq!(end_game_bonus(&set, &near), 0);
}

#[test]
fn a134_requires_three_or_four_players() {
    let registry = build_default_registry().unwrap();
    let a134 = registry.card_id("A134").expect("A134 must be registered");
    let pool2 = CardPool::new(&registry, 2, true);
    let pool3 = CardPool::new(&registry, 3, true);
    let pool4 = CardPool::new(&registry, 4, true);
    assert!(!pool2.contains(a134), "A134 is not eligible at 2 players");
    assert!(pool3.contains(a134), "A134 is eligible at 3 players");
    assert!(pool4.contains(a134), "A134 is eligible at 4 players");
}

#[test]
fn a134_bonus_is_read_time_and_never_cached() {
    // The end-game bonus is a pure function of the live pastures: recomputing
    // it (e.g. after a save/load that restores the same public state) returns
    // the same value, and playing A134 never moves it into card_points.
    let (set, mut player) = a134_owner(|p| {
        p.pastures.push(full_pasture(0, vec![0, 1]));
        p.pastures.push(full_pasture(1, vec![2, 3]));
    });
    let first = end_game_bonus(&set, &player);
    assert_eq!(first, 2);
    // A cloned public state stands in for a resume/replay round-trip.
    let restored = player.clone();
    assert_eq!(
        end_game_bonus(&set, &restored),
        first,
        "recompute must match"
    );
    assert_eq!(
        restored.card_points, 0,
        "printed points never absorb the bonus"
    );
    // Adding a third full pasture changes the live-state read, with no stale cache.
    player.pastures.push(full_pasture(2, vec![4, 5]));
    assert_eq!(end_game_bonus(&set, &player), 3);
}

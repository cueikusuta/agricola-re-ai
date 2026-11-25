//! Freebuff on_play gain chapter: C band (C026, C086).
//!
//! Only the printed on-play gain half is wired; the remaining triggers are
//! follow-ups documented in `PROGRESS.md`.

use agricola_re_ai::cards::{play, verified_band_c_cards, CardKind, CardSet};
use agricola_re_ai::model::{Good, Player};

fn set() -> CardSet {
    CardSet {
        definitions: verified_band_c_cards(),
    }
}

#[test]
fn c026_costs_one_wood_and_gains_two_food() {
    let set = set();
    let card = set.definition("C026").unwrap();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Wood, 1)]);
    assert_eq!(card.points, 0);

    let mut player = Player::new(0, 0);
    player.supply.wood = 1;
    play(
        &set,
        &mut vec!["C026".to_string()],
        &mut player,
        "C026",
        CardKind::MinorImprovement,
        0,
    )
    .unwrap();
    assert_eq!(player.supply.food, 2);
    assert_eq!(player.supply.wood, 0);
}

#[test]
fn c086_gains_one_grain() {
    let set = set();
    let card = set.definition("C086").unwrap();
    assert_eq!(card.kind, CardKind::Occupation);
    assert_eq!(card.cost, vec![]);
    assert_eq!(card.points, 0);

    let mut player = Player::new(0, 0);
    play(
        &set,
        &mut vec!["C086".to_string()],
        &mut player,
        "C086",
        CardKind::Occupation,
        0,
    )
    .unwrap();
    assert_eq!(player.supply.grain, 1);
}

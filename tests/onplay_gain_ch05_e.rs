//! Freebuff on_play gain chapter: E band (E082).
//!
//! Only the printed on-play gain half is wired; the remaining trigger is a
//! follow-up documented in `PROGRESS.md`.

use agricola_re_ai::cards::{play, verified_band_e_cards, CardKind, CardSet};
use agricola_re_ai::model::Player;

#[test]
fn e082_gains_one_food() {
    let set = CardSet {
        definitions: verified_band_e_cards(),
    };
    let card = set.definition("E082").unwrap();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![]);
    assert_eq!(card.points, 0);

    let mut player = Player::new(0, 0);
    play(
        &set,
        &mut vec!["E082".to_string()],
        &mut player,
        "E082",
        CardKind::MinorImprovement,
        0,
    )
    .unwrap();
    assert_eq!(player.supply.food, 1);
}

//! Freebuff on_play gain chapter: B band (B025, B103, B116, B155).
//!
//! Only the printed on-play gain half is wired; the remaining triggers are
//! follow-ups documented in `PROGRESS.md`.

use agricola_re_ai::cards::{
    play, verified_band_b_cards, CardEffect, CardKind, CardRequirement, CardSet,
};
use agricola_re_ai::model::{Good, Player};

fn set() -> CardSet {
    CardSet {
        definitions: verified_band_b_cards(),
    }
}

fn play_card(id: &str, kind: CardKind, wood: u32) -> Player {
    let set = set();
    let mut player = Player::new(0, 0);
    player.supply.wood = wood;
    play(&set, &mut vec![id.to_string()], &mut player, id, kind, 0)
        .unwrap_or_else(|e| panic!("play {id}: {e}"));
    player
}

#[test]
fn b025_costs_one_wood_and_gains_one_food() {
    let set = set();
    let card = set.definition("B025").unwrap();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Wood, 1)]);
    assert_eq!(card.points, 0);
    let player = play_card("B025", CardKind::MinorImprovement, 1);
    assert_eq!(player.supply.food, 1);
    assert_eq!(player.supply.wood, 0);
}

#[test]
fn b103_gains_one_wood_and_one_reed() {
    let set = set();
    let card = set.definition("B103").unwrap();
    assert_eq!(card.kind, CardKind::Occupation);
    assert_eq!(
        card.effects,
        vec![
            CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 1,
            },
            CardEffect::OnPlayGain {
                good: Good::Reed,
                amount: 1,
            },
        ]
    );
    let player = play_card("B103", CardKind::Occupation, 0);
    assert_eq!(player.supply.wood, 1);
    assert_eq!(player.supply.reed, 1);
}

#[test]
fn b116_gains_one_wood() {
    let player = play_card("B116", CardKind::Occupation, 0);
    assert_eq!(player.supply.wood, 1);
}

#[test]
fn b155_is_four_player_only_and_gains_wood_and_reed() {
    let set = set();
    let card = set.definition("B155").unwrap();
    assert_eq!(card.kind, CardKind::Occupation);
    assert_eq!(
        card.effects,
        vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(4),
            },
            CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 1,
            },
            CardEffect::OnPlayGain {
                good: Good::Reed,
                amount: 1,
            },
        ]
    );
    let player = play_card("B155", CardKind::Occupation, 0);
    assert_eq!(player.supply.wood, 1);
    assert_eq!(player.supply.reed, 1);
}

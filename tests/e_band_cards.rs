//! Tests for E-band 001-069 simple OnPlayGain cards (E007/E008).
//!
//! These tests verify:
//! - Normal activation (on-play effect triggers correctly)
//! - Cost payment verification (cannot play without required resources)
//! - Absence from the two-player preset (verified_basic_card_set)

use agricola_re_ai::cards::{
    play, verified_band_e_cards, verified_basic_card_set, CardEffect, CardKind,
    CardSet,
};
use agricola_re_ai::model::{Good, Player};

fn play_card(id: &str, player: &mut Player) -> Result<(), String> {
    let set = CardSet {
        definitions: verified_band_e_cards(),
    };
    let mut hand = vec![id.to_string()];
    let kind = set.definition(id).unwrap().kind;
    play(&set, &mut hand, player, id, kind, 0)
}

#[test]
fn e007_on_play_gains_four_food() {
    let mut player = Player::new(0, 0);
    player.supply.grain = 1;
    play_card("E007", &mut player).unwrap();
    assert_eq!(player.supply.food, 4);
    assert_eq!(player.supply.grain, 0);
    assert_eq!(player.played_cards, vec!["E007"]);
}

#[test]
fn e007_cannot_play_without_grain() {
    let mut player = Player::new(0, 0);
    let old = player.clone();
    assert!(play_card("E007", &mut player).is_err());
    assert_eq!(player, old);
}

#[test]
fn e008_on_play_gains_one_vegetable() {
    let mut player = Player::new(0, 0);
    player.supply.food = 2;
    play_card("E008", &mut player).unwrap();
    assert_eq!(player.supply.vegetable, 1);
    assert_eq!(player.supply.food, 0);
    assert_eq!(player.played_cards, vec!["E008"]);
}

#[test]
fn e008_cannot_play_without_food() {
    let mut player = Player::new(0, 0);
    let old = player.clone();
    assert!(play_card("E008", &mut player).is_err());
    assert_eq!(player, old);
}

#[test]
fn band_e_cards_are_absent_from_the_two_player_preset() {
    let preset = verified_basic_card_set(2).unwrap();
    for id in ["E007", "E008"] {
        assert!(
            preset.definition(id).is_none(),
            "{id} must stay out of the A/B two-player preset"
        );
    }
}

#[test]
fn e007_and_e008_are_in_verified_band_e_pool() {
    let set = CardSet {
        definitions: verified_band_e_cards(),
    };

    // E007
    let card = set.definition("E007").unwrap();
    assert_eq!(card.id, "E007");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Grain, 1)]);
    assert_eq!(card.points, 0);
    assert_eq!(
        card.effects,
        vec![CardEffect::OnPlayGain {
            good: Good::Food,
            amount: 4,
        }]
    );

    // E008
    let card = set.definition("E008").unwrap();
    assert_eq!(card.id, "E008");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Food, 2)]);
    assert_eq!(card.points, 0);
    assert_eq!(
        card.effects,
        vec![CardEffect::OnPlayGain {
            good: Good::Vegetable,
            amount: 1,
        }]
    );
}

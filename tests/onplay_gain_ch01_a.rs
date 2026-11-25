//! Freebuff on_play gain chapter: A band (A054, A165).
//!
//! Only the printed on-play gain half is wired; the remaining triggers are
//! follow-ups documented in `PROGRESS.md`. These tests pin the card metadata,
//! the existing prerequisite gates, and the supply delta through the canonical
//! `cards::play` path.

use agricola_re_ai::cards::{
    can_play, play, verified_band_a_cards, CardDefinition, CardEffect, CardKind, CardRequirement,
    CardSet,
};
use agricola_re_ai::model::{Good, Player};

fn set() -> CardSet {
    CardSet {
        definitions: verified_band_a_cards(),
    }
}

fn occ(id: &str) -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: id.to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![],
    }
}

#[test]
fn a054_has_max_three_occupations_and_gains_five_food() {
    let mut set = set();
    set.definitions
        .extend([occ("occ1"), occ("occ2"), occ("occ3"), occ("occ4")]);
    let card = set.definition("A054").unwrap();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![]);
    assert_eq!(card.points, 0);
    assert_eq!(
        card.effects,
        vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MaximumOccupations(3),
            },
            CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 5,
            },
        ]
    );

    let mut player = Player::new(0, 0);
    player.played_cards = vec!["occ1".into(), "occ2".into(), "occ3".into()];
    assert!(can_play(&set, card, &player, 0));
    play(
        &set,
        &mut vec!["A054".to_string()],
        &mut player,
        "A054",
        CardKind::MinorImprovement,
        0,
    )
    .unwrap();
    assert_eq!(player.supply.food, 5);

    let mut too_many = Player::new(0, 0);
    too_many.played_cards = vec!["occ1".into(), "occ2".into(), "occ3".into(), "occ4".into()];
    assert!(!can_play(&set, card, &too_many, 0));
}

#[test]
fn a165_is_four_player_only_and_gains_one_food() {
    let set = set();
    let card = set.definition("A165").unwrap();
    assert_eq!(card.kind, CardKind::Occupation);
    assert_eq!(card.cost, vec![]);
    assert_eq!(card.points, 0);
    assert_eq!(
        card.effects,
        vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(4),
            },
            CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 1,
            },
        ]
    );

    let mut player = Player::new(0, 0);
    play(
        &set,
        &mut vec!["A165".to_string()],
        &mut player,
        "A165",
        CardKind::Occupation,
        0,
    )
    .unwrap();
    assert_eq!(player.supply.food, 1);
    assert_eq!(player.played_cards, vec!["A165"]);
}

//! Tests for the end-game scoring cards E124 市長候補者 (Mayor Candidate) and
//! B031 製陶所の敷地 (Pottery Yard), through the public card API.
//!
//! - E124: occupation, no cost; on play gain 2 wood and 2 stone; at final
//!   scoring -1 point per wood and per stone in the supply.
//! - B031: minor, no cost, 1 printed point; prerequisite: the major Pottery
//!   (製陶所) in front; at final scoring +2 when two orthogonally adjacent
//!   farmyard spaces are unused (empty and without a stable).

use agricola_re_ai::cards::{
    can_play, end_game_bonus, verified_band_b_cards, verified_band_e_cards, CardEffect,
    CardRequirement, CardSet, ScoreBonusRule,
};
use agricola_re_ai::model::{Improvement, Player, Tile};

fn definition(
    cards: Vec<agricola_re_ai::cards::CardDefinition>,
    id: &str,
) -> agricola_re_ai::cards::CardDefinition {
    cards
        .into_iter()
        .find(|card| card.id == id)
        .unwrap_or_else(|| panic!("{id} must be defined"))
}

fn b031() -> agricola_re_ai::cards::CardDefinition {
    definition(verified_band_b_cards(), "B031")
}

fn e124() -> agricola_re_ai::cards::CardDefinition {
    definition(verified_band_e_cards(), "E124")
}

fn set_of(card: &agricola_re_ai::cards::CardDefinition) -> CardSet {
    CardSet {
        definitions: vec![card.clone()],
    }
}

/// Owner with B031 in play and the given farm edits applied.
fn b031_bonus_with(edit: impl FnOnce(&mut Player)) -> i32 {
    let card = b031();
    let set = set_of(&card);
    let mut player = Player::new(0, 0);
    player.played_cards.push("B031".into());
    edit(&mut player);
    end_game_bonus(&set, &player)
}

#[test]
fn b031_definition_has_prerequisite_points_and_scoring_rule() {
    let card = b031();
    assert_eq!(card.points, 1);
    assert!(card.cost.is_empty());
    assert!(card.effects.iter().any(|effect| matches!(
        effect,
        CardEffect::Prerequisite {
            rule: CardRequirement::OwnsMajorImprovement(Improvement::Pottery)
        }
    )));
    assert!(card.effects.iter().any(|effect| matches!(
        effect,
        CardEffect::EndGameBonus {
            rule: ScoreBonusRule::UnusedFarmAdjacentPair
        }
    )));
}

#[test]
fn b031_prerequisite_needs_the_major_pottery() {
    let card = b031();
    let set = set_of(&card);
    let without = Player::new(0, 0);
    assert!(
        !can_play(&set, &card, &without, 0),
        "no Pottery -> not playable"
    );

    let mut with_pottery = Player::new(0, 0);
    with_pottery.improvements.push(Improvement::Pottery);
    assert!(
        can_play(&set, &card, &with_pottery, 0),
        "Pottery owned -> playable"
    );

    let mut other_major = Player::new(0, 0);
    other_major.improvements.push(Improvement::Well);
    assert!(
        !can_play(&set, &card, &other_major, 0),
        "other majors do not count"
    );
}

#[test]
fn b031_scores_two_when_an_adjacent_unused_pair_exists() {
    // Fresh farmyard: every space is unused, so adjacent unused pairs exist.
    assert_eq!(b031_bonus_with(|_| {}), 2);
}

#[test]
fn b031_scores_zero_when_no_two_unused_spaces_touch() {
    // Unused spaces 0, 2, 4, 6, 8, 10, 12, 14 form a checkerboard on the
    // 5x3 grid: no two of them are orthogonally adjacent. Rooms fill the rest.
    let unused = [0usize, 2, 4, 6, 8, 10, 12, 14];
    let bonus = b031_bonus_with(|player| {
        for index in 0..player.farm.len() {
            player.farm[index].tile = if unused.contains(&index) {
                Tile::Empty
            } else {
                Tile::Room
            };
        }
    });
    assert_eq!(bonus, 0);
}

#[test]
fn b031_stable_on_an_empty_space_makes_it_used() {
    // Only the orthogonal pair (0, 1) is unused in this layout; a stable on
    // cell 0 uses that space, so no adjacent unused pair remains.
    let unused = [0usize, 1];
    let layout = |player: &mut Player, stable_on_zero: bool| {
        for index in 0..player.farm.len() {
            // Fresh players start with rooms on cells 0 and 1, so reset both
            // the unused cells (Empty) and every other cell (Room).
            player.farm[index].tile = if unused.contains(&index) {
                Tile::Empty
            } else {
                Tile::Room
            };
        }
        if stable_on_zero {
            player.farm[0].stable = true;
        }
    };
    assert_eq!(b031_bonus_with(|p| layout(p, false)), 2, "pair 0-1 unused");
    assert_eq!(
        b031_bonus_with(|p| layout(p, true)),
        0,
        "stable on 0 breaks the pair"
    );
}

#[test]
fn e124_definition_matches_the_spec() {
    let card = e124();
    assert_eq!(card.points, 0);
    assert!(card.cost.is_empty());
    let gains: Vec<_> = card
        .effects
        .iter()
        .filter_map(|effect| match effect {
            CardEffect::OnPlayGain { good, amount } => Some((*good, *amount)),
            _ => None,
        })
        .collect();
    assert_eq!(
        gains,
        vec![
            (agricola_re_ai::model::Good::Wood, 2),
            (agricola_re_ai::model::Good::Stone, 2)
        ]
    );
    assert!(card.effects.iter().any(|effect| matches!(
        effect,
        CardEffect::EndGameBonus {
            rule: ScoreBonusRule::WoodStoneSupplyPenalty
        }
    )));
}

#[test]
fn e124_penalizes_one_point_per_wood_and_stone_in_supply() {
    let card = e124();
    let set = set_of(&card);
    let mut player = Player::new(0, 0);
    player.played_cards.push("E124".into());
    assert_eq!(
        end_game_bonus(&set, &player),
        0,
        "empty supply -> no penalty"
    );

    player.supply.wood = 3;
    player.supply.stone = 2;
    player.supply.reed = 4; // reed is not penalized
    assert_eq!(end_game_bonus(&set, &player), -5);
}

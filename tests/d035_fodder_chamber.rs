//! D035 飼料部屋 (Fodder Chamber): end-game 1 bonus point per threshold
//! number of own livestock (2p: 7 animals/point, 3p: 5, 4p: 4).
//!
//! The card carries all three player-count runs guarded by
//! Minimum/MaximumPlayers; `CardSet::projected_for_players` keeps exactly
//! one run per game, so each test projects the band-D set for its player
//! count before scoring.

use agricola_re_ai::cards::{end_game_bonus, verified_band_d_cards, CardSet};
use agricola_re_ai::model::{Animal, Pasture, Player};

fn d035_set(players: usize) -> CardSet {
    CardSet {
        definitions: verified_band_d_cards(),
    }
    .projected_for_players(players)
    .unwrap_or_else(|e| panic!("D035 must project for {players} players: {e}"))
}

/// A player with only D035 in the played area and the given livestock.
/// All species count toward the total (sheep, boar, cattle).
fn owner(sheep: u32, boar: u32, cattle: u32) -> Player {
    let mut p = Player::new(0, 0);
    p.played_cards.push("D035".to_string());
    let mut add = |kind: Animal, quantity: u32, id: u64| {
        if quantity > 0 {
            p.pastures.push(Pasture {
                id: id as u8,
                cells: vec![0, 1],
                animal: Some(kind),
                quantity,
            });
        }
    };
    add(Animal::Sheep, sheep, 0);
    add(Animal::Boar, boar, 1);
    add(Animal::Cattle, cattle, 2);
    p
}

#[test]
fn d035_two_player_14_livestock_is_2_points() {
    // Spec case D035-two-player: 2p, 14 livestock -> floor(14 / 7) = 2.
    let set = d035_set(2);
    assert_eq!(end_game_bonus(&set, &owner(14, 0, 0)), 2);
    // Boundaries around the 7-animal threshold.
    assert_eq!(end_game_bonus(&set, &owner(13, 0, 0)), 1);
    assert_eq!(end_game_bonus(&set, &owner(7, 0, 0)), 1);
    assert_eq!(end_game_bonus(&set, &owner(6, 0, 0)), 0);
    // All species count toward the total.
    assert_eq!(end_game_bonus(&set, &owner(5, 5, 4)), 2);
    assert_eq!(end_game_bonus(&set, &owner(0, 0, 0)), 0);
}

#[test]
fn d035_three_player_threshold_is_five() {
    let set = d035_set(3);
    assert_eq!(end_game_bonus(&set, &owner(10, 0, 0)), 2);
    assert_eq!(end_game_bonus(&set, &owner(9, 0, 0)), 1);
    assert_eq!(end_game_bonus(&set, &owner(5, 0, 0)), 1);
    assert_eq!(end_game_bonus(&set, &owner(4, 0, 0)), 0);
    assert_eq!(end_game_bonus(&set, &owner(3, 4, 8)), 3);
}

#[test]
fn d035_four_player_10_livestock_is_2_points() {
    // Spec case D035-four-player: 4p, 10 livestock -> floor(10 / 4) = 2.
    let set = d035_set(4);
    assert_eq!(end_game_bonus(&set, &owner(10, 0, 0)), 2);
    // Boundaries around the 4-animal threshold.
    assert_eq!(end_game_bonus(&set, &owner(11, 0, 0)), 2);
    assert_eq!(end_game_bonus(&set, &owner(12, 0, 0)), 3);
    assert_eq!(end_game_bonus(&set, &owner(4, 0, 0)), 1);
    assert_eq!(end_game_bonus(&set, &owner(3, 0, 0)), 0);
    assert_eq!(end_game_bonus(&set, &owner(0, 7, 0)), 1);
}

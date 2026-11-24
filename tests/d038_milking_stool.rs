//! D038 搾乳台 (Milking Stool): cattle-tiered harvest food plus end-game
//! cattle-pair bonus.
//!
//! Effect-only tests per data/bga-card-tests/D038.json
//! (D038-normal / D038-zero / D038-scoring), extended to the full cattle
//! boundaries 0/1/2/3/4/5/6 for harvest and 0/1/2/3/4 for scoring.
//!
//! The harvest side uses [`HarvestFieldRule::CattleThresholds`] via
//! [`crate::cards::harvest_field_rewards`]; the scoring side uses
//! [`ScoreBonusRule::CattlePairs`] via [`crate::cards::end_game_bonus`].
//! Both arms are mirror implementations (cards.rs legacy path and
//! card_events.rs dispatcher path), so the legacy-path assertions below
//! pin the shared tier table.

use agricola_re_ai::cards::{
    end_game_bonus, harvest_field_rewards, verified_band_d_cards, CardSet,
};
use agricola_re_ai::model::{Animal, Pasture, Player};

fn d038_set() -> CardSet {
    let def = verified_band_d_cards()
        .into_iter()
        .find(|card| card.id == "D038")
        .expect("D038 missing from verified band D pool");
    CardSet {
        definitions: vec![def],
    }
}

fn player_with_cattle(n: u32) -> Player {
    let mut player = Player::new(0, 0);
    player.played_cards.push("D038".to_owned());
    player.pastures.push(Pasture {
        id: 0,
        cells: Vec::new(),
        animal: Some(Animal::Cattle),
        quantity: n,
    });
    player
}

fn food_delta(set: &CardSet, cattle: u32) -> u32 {
    let mut player = player_with_cattle(cattle);
    let before = player.supply.food;
    harvest_field_rewards(set, &mut player).expect("harvest field phase must resolve");
    player.supply.food - before
}

#[test]
fn d038_harvest_cattle_tiers_match_the_spec_table() {
    let set = d038_set();
    // Spec: cattle 0/1/2/3/4/5/6+ -> food 0/1/1/2/2/3/3 (tiers at 1/3/5,
    // highest reached tier only).
    let cases = [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3), (6, 3)];
    for (cattle, expected_food) in cases {
        assert_eq!(
            food_delta(&set, cattle),
            expected_food,
            "cattle {cattle} must yield {expected_food} food"
        );
    }
}

#[test]
fn d038_normal_and_zero_spec_cases() {
    let set = d038_set();
    // D038-normal: 3 cattle -> 2 food.
    assert_eq!(food_delta(&set, 3), 2);
    // D038-zero: 0 cattle -> 0 food.
    assert_eq!(food_delta(&set, 0), 0);
}

#[test]
fn d038_end_game_cattle_pairs() {
    let set = d038_set();
    // animal_count(Cattle) / 2, truncated: 0/1 -> 0, 2/3 -> 1, 4 -> 2.
    let cases = [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2)];
    for (cattle, expected_points) in cases {
        let player = player_with_cattle(cattle);
        assert_eq!(
            end_game_bonus(&set, &player),
            expected_points,
            "cattle {cattle} must score {expected_points} points"
        );
    }
}

#[test]
fn d038_scoring_spec_case() {
    let set = d038_set();
    // D038-scoring: 5 cattle -> 2 points.
    let player = player_with_cattle(5);
    assert_eq!(end_game_bonus(&set, &player), 2);
}

#[test]
fn d038_food_and_points_are_independent() {
    // Harvest must only add food; scoring must only add points.
    let set = d038_set();
    let mut player = player_with_cattle(3);
    let food_before = player.supply.food;
    harvest_field_rewards(&set, &mut player).unwrap();
    assert_eq!(player.supply.food, food_before + 2, "3 cattle yield 2 food");
    let scored = end_game_bonus(&set, &player);
    assert_eq!(scored, 1, "3 cattle score 1 point (3 / 2 truncated)");
    assert_eq!(
        player.supply.food,
        food_before + 2,
        "scoring must not consume food"
    );
}

#[test]
fn d038_serde_round_trip_preserves_behavior() {
    // No new save/load fields are introduced; the new enum variants must
    // survive serde serialization and reproduce the same effects.
    let set = d038_set();
    let json = serde_json::to_string(&set.definitions[0]).expect("definition serializes");
    let round_tripped: agricola_re_ai::cards::CardDefinition =
        serde_json::from_str(&json).expect("definition deserializes");
    let round_tripped_set = CardSet {
        definitions: vec![round_tripped],
    };
    assert_eq!(food_delta(&round_tripped_set, 5), 3);
    let player = player_with_cattle(5);
    assert_eq!(end_game_bonus(&round_tripped_set, &player), 2);
}

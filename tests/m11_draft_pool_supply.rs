//! M11 (#104): the production draft pool comes from implemented cards, and
//! test fixtures must never be mistaken for verified cards.
use agricola_re_ai::cards::{verified_basic_card_set, CardKind};
use agricola_re_ai::draft::DraftMode;
use agricola_re_ai::draft_game::pool_from_card_set;

#[test]
fn implemented_two_player_pool_supports_normal_deal_and_seven_seven() {
    let set = verified_basic_card_set(2).unwrap();
    let occupations = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .count();
    let minors = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .count();
    assert!(occupations >= 14, "7+7 at two players needs 14 occupations");
    assert!(
        minors >= 14,
        "7+7 at two players needs 14 minor improvements"
    );

    let pool = pool_from_card_set(&set);
    pool.validate().unwrap();
    pool.check_supply(DraftMode::Deal7, 2).unwrap();
    pool.check_supply(DraftMode::Draft77, 2).unwrap();
}

#[test]
fn ten_seven_production_mode_is_blocked_until_the_pool_is_large_enough() {
    let set = verified_basic_card_set(2).unwrap();
    let pool = pool_from_card_set(&set);
    // A 10-7 draft needs 20 of each kind. The implemented set is smaller, so
    // the mode is rejected instead of padded with unverified cards.
    assert!(pool.check_supply(DraftMode::Choose10_7, 2).is_err());
    assert!(pool
        .check_supply(DraftMode::Draft10_7Simultaneous, 2)
        .is_err());
}

#[test]
fn four_player_production_pool_is_not_available_yet() {
    assert!(verified_basic_card_set(4).is_err());
    assert!(verified_basic_card_set(3).is_err());
}

#[test]
fn fixture_ids_never_appear_in_the_verified_pool() {
    let set = verified_basic_card_set(2).unwrap();
    assert!(set
        .definitions
        .iter()
        .all(|card| !card.id.starts_with("FIX-")));
}

//! Tests for B153 寮長 (Housemaster) through the real engine paths.
//!
//! B153 is a 4-player-only occupation: at final scoring, sum the
//! major-improvement scores after doubling the lowest single one (one only,
//! even when the lowest score is tied), then take 1/2/3/4 bonus points at a
//! doubled total of 5/7/9/11 or more. The ordinary major-improvement points
//! stay in their own scoring category; the bonus lands in `card_bonus`.
//!
//! Covered per the spec test contract (data/bga-card-tests/B153.json) and the
//! spec (data/bga-card-specs/B153.json):
//! - registry reachability (B153 resolves via `build_default_registry`) and
//!   the MinimumPlayers(4) + EndGameBonus definition shape
//! - pool eligibility: absent from the 2/3-player projections, present at 4
//!   (both through `card_set_for_game` and the raw projection grammar)
//! - B153-normal: the spec's [2, 3, 4] example — doubled total 11
//! - the 5/7/9/11 threshold ladder on the doubled total (1/2/3/4 points)
//! - a tied lowest score doubles exactly once; no major improvement -> 0
//! - separation of the bonus from the ordinary improvement points
//! - engine: a 4-player game plays B153 via Lessons and `score_with_cards`
//!   pays the bonus to the owner only

use agricola_re_ai::card_pool::Eligibility;
use agricola_re_ai::cards::{
    card_set_for_game, verified_band_b_cards, CardEffect, CardKind, CardRequirement, CardSet,
    ScoreBonusRule,
};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Improvement};
use agricola_re_ai::registry::build_default_registry;
use agricola_re_ai::score::score_with_cards;

fn b153_definition() -> agricola_re_ai::cards::CardDefinition {
    verified_band_b_cards()
        .into_iter()
        .find(|c| c.id == "B153")
        .expect("B153 must be defined in verified_band_b_cards")
}

/// A sandbox set holding just B153: end-game bonuses are read-time
/// computations over public played cards, so a one-definition set is enough.
fn set_with_b153() -> CardSet {
    CardSet {
        definitions: vec![b153_definition()],
    }
}

/// Bonus for an owner holding exactly the listed major improvements.
fn bonus_for(improvements: &[Improvement]) -> i32 {
    let set = set_with_b153();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("B153".into());
    player.improvements.extend_from_slice(improvements);
    agricola_re_ai::cards::end_game_bonus(&set, &player)
}

/// One-point major improvements (the engine's Fireplace2/3, Hearth4/5): n
/// copies double the single lowest one, so the doubled total is n + 1.
fn fireplaces(count: usize) -> Vec<Improvement> {
    vec![Improvement::Fireplace2; count]
}

#[test]
fn b153_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry
        .card_id("B153")
        .expect("B153 must be registered (band B pool)");
    let definition = registry
        .get(card_id)
        .expect("registered B153 resolves")
        .to_card_definition();
    assert_eq!(definition.id, "B153");
    assert_eq!(definition.kind, CardKind::Occupation);
    assert_eq!(definition.cost, vec![]);
    assert_eq!(definition.points, 0);
    assert_eq!(
        definition.effects,
        vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(4),
            },
            CardEffect::EndGameBonus {
                rule: ScoreBonusRule::MajorImprovementsLowestDoubledThresholds,
            },
        ]
    );
}

/// The registry projection drops B153's guarded bonus run at 2/3 players and
/// keeps it at 4, so the merged definition is eligible only at 4.
#[test]
fn b153_registry_projection_excludes_two_and_three_players() {
    let registry = build_default_registry().expect("default registry builds");
    let card = registry
        .get_by_external("B153")
        .expect("B153 must be registered");
    assert!(
        card.to_card_definition_for_players(2).effects.is_empty(),
        "the guarded bonus run is dropped at 2 players"
    );
    assert!(
        card.to_card_definition_for_players(3).effects.is_empty(),
        "the guarded bonus run is dropped at 3 players"
    );
    assert_eq!(
        card.to_card_definition_for_players(4).effects.len(),
        2,
        "the prerequisite and the bonus both survive at 4 players"
    );
    let review = registry
        .review_by_external("B153")
        .expect("B153 carries review metadata");
    assert_eq!(review.eligible_at_2, Eligibility::No);
    assert_eq!(review.eligible_at_3, Eligibility::No);
    assert_eq!(review.eligible_at_4, Eligibility::Yes);
}

#[test]
fn b153_pool_eligibility_four_yes_two_three_no() {
    assert!(card_set_for_game(2, true).definition("B153").is_none());
    assert!(card_set_for_game(3, true).definition("B153").is_none());
    let four = card_set_for_game(4, true);
    assert!(four.definition("B153").is_some());
    assert!(four
        .definition("B153")
        .unwrap()
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::EndGameBonus { .. })));
}

/// The raw band-B set is a 4-player catalog: the shared projection grammar
/// rejects it for a 2-player game entry (B153 would lose every effect)
/// instead of silently keeping a dead card.
#[test]
fn b153_raw_band_set_is_rejected_for_two_players() {
    let band = CardSet {
        definitions: verified_band_b_cards(),
    };
    assert!(
        band.clone().projected_for_players(2).is_err(),
        "B153 has no active effect at 2 players"
    );
    assert!(
        band.projected_for_players(4).is_ok(),
        "the band-B set projects cleanly at 4 players"
    );
}

/// B153-normal (data/bga-card-tests/B153.json): the spec example plays
/// [2, 3, 4], doubles the lowest score (2 -> 4) and records the doubled
/// total 11 — reproduced here exactly.
///
/// The same file records `bonus_points: 1` with the note "bonus threshold
/// 5 => +1 point"; that value is NOT what the printed card pays. The BGA
/// wiki effect text (agricola.cloudfree.jp ?p=3104, the page the spec cites)
/// reads "合計が5/7/9/11以上ならボーナス1/2/3/4点を得る", and the page's play
/// reports confirm the stacking reading: Well + ClayOven + Basketry
/// (4+4+2 = 10) paid 3 points, and a total of 11 is discussed as
/// "11の4点". A doubled total of 11 therefore pays 4, so the spec-test
/// JSON's `bonus_points: 1` is a spec-test data defect (recorded on the PR
/// for the status sync), while the printed rule pays 4.
#[test]
fn b153_normal_spec_case_doubles_lowest_to_eleven_and_pays_four() {
    // ClayOven 2, StoneOven 3, Well 4: double the lowest (2 -> 4) -> 4+3+4 = 11.
    let score = bonus_for(&[
        Improvement::ClayOven,
        Improvement::StoneOven,
        Improvement::Well,
    ]);
    assert_eq!(score, 4, "doubled total 11 meets 5/7/9/11 -> +4");
}

/// The 5/7/9/11 ladder pays 1/2/3/4 on the DOUBLED total, evaluated after
/// the doubling only.
#[test]
fn b153_thresholds_five_seven_nine_eleven_pay_one_two_three_four() {
    assert_eq!(bonus_for(&[]), 0, "no major improvement -> no bonus");
    assert_eq!(bonus_for(&fireplaces(3)), 0, "doubled total 4 < 5");
    assert_eq!(bonus_for(&fireplaces(4)), 1, "doubled total 5 -> +1");
    assert_eq!(bonus_for(&fireplaces(5)), 1, "doubled total 6 still +1");
    assert_eq!(bonus_for(&fireplaces(6)), 2, "doubled total 7 -> +2");
    assert_eq!(bonus_for(&fireplaces(7)), 2, "doubled total 8 still +2");
    assert_eq!(bonus_for(&fireplaces(8)), 3, "doubled total 9 -> +3");
    assert_eq!(bonus_for(&fireplaces(9)), 3, "doubled total 10 still +3");
    assert_eq!(bonus_for(&fireplaces(10)), 4, "doubled total 11 -> +4");
    // A single 4-point major doubles to 8: +2 without the doubling it would
    // be only +1 (4 >= 5 is false), so the threshold really reads the
    // doubled total.
    assert_eq!(
        bonus_for(&[Improvement::Well]),
        2,
        "Well 4 doubles to 8 -> +2"
    );
}

/// A tied lowest score is doubled exactly once (BGA plays it that way: Well
/// + ClayOven + Basketry scores 4+4+2 = 10 -> +3, not 12 -> +4).
#[test]
fn b153_doubles_only_one_of_the_tied_lowest_scores() {
    assert_eq!(
        bonus_for(&[
            Improvement::Well,
            Improvement::ClayOven,
            Improvement::Basketry,
        ]),
        3,
        "4+4+2 = 10 -> +3, the tied 2 doubles once"
    );
    // Two tied lowest scores: ClayOven 2 + Basketry 2 tie, StoneOven 3; only
    // one of the tied 2s doubles, so 4+2+3 = 9 -> +3. Doubling both would
    // give 4+4+3 = 11 -> +4, which BGA does not pay.
    assert_eq!(
        bonus_for(&[
            Improvement::ClayOven,
            Improvement::Basketry,
            Improvement::StoneOven,
        ]),
        3,
        "4+2+3 = 9 -> +3, the tied 2 doubles once"
    );
    // A lone 1-point major doubles to 2: Well 4 + Fireplace2 -> 4+2 = 6,
    // which clears only the 5 threshold (6 < 7).
    assert_eq!(
        bonus_for(&[Improvement::Well, Improvement::Fireplace2]),
        1,
        "4+2 = 6 -> +1"
    );
}

/// The bonus is a separate scoring category: the ordinary major-improvement
/// points keep their undoubled value in `improvements`, and B153's bonus
/// lands in `card_bonus` without ever joining `card_points`.
#[test]
fn b153_bonus_is_separate_from_the_ordinary_improvement_points() {
    let set = set_with_b153();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("B153".into());
    player.improvements.extend([
        Improvement::ClayOven,
        Improvement::StoneOven,
        Improvement::Well,
    ]);

    let scored = score_with_cards(&player, &set);
    assert_eq!(
        scored.categories.get("improvements").copied(),
        Some(9),
        "the ordinary category keeps the undoubled 2+3+4"
    );
    assert_eq!(
        scored.categories.get("card_bonus").copied(),
        Some(4),
        "the doubled-lowest bonus lands in card_bonus"
    );
    assert_eq!(player.card_points, 0, "the bonus never joins card_points");

    // Without the card the same improvements score no bonus at all.
    let mut plain = player.clone();
    plain.played_cards.clear();
    let plain_scored = score_with_cards(&plain, &set);
    assert_eq!(plain_scored.categories.get("card_bonus").copied(), Some(0));
    assert_eq!(scored.total, plain_scored.total + 4);
}

fn hands_with_b153_in_seat0(set: &CardSet) -> Vec<Vec<String>> {
    let mut occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .map(|card| card.id.clone())
        .collect();
    assert!(
        occupations.len() + 1 >= 7 * 4,
        "enough occupations to deal 7 per player plus B153"
    );
    assert!(minors.len() >= 7 * 4, "enough minors to deal 7 per player");
    // Rotate B153 to the front so seat 0 holds it in the initial deal.
    let position = occupations
        .iter()
        .position(|id| id == "B153")
        .expect("B153 in the 4-player pool");
    occupations.remove(position);
    occupations.insert(0, "B153".to_owned());
    let mut hands = vec![Vec::new(); 4];
    let mut occ_cursor = 0;
    let mut minor_cursor = 0;
    for hand in &mut hands {
        hand.extend_from_slice(&occupations[occ_cursor..occ_cursor + 7]);
        occ_cursor += 7;
    }
    for hand in &mut hands {
        hand.extend_from_slice(&minors[minor_cursor..minor_cursor + 7]);
        minor_cursor += 7;
    }
    hands
}

/// A 4-player game from the experimental pool with B153 dealt to seat 0.
fn four_player_game_with_b153() -> Game {
    let set = card_set_for_game(4, true);
    let hands = hands_with_b153_in_seat0(&set);
    let config = Config {
        players: 4,
        seed: 0xB153_5117,
        additional_action_space_enabled: false,
    };
    Game::with_cards_and_hands_with_start_player(config, set, hands, 0)
        .expect("valid explicit B153 game")
}

/// Move the listed major improvements from the market to `player`, keeping
/// the "every physical improvement has exactly one location" partition.
fn take_improvements(game: &mut Game, player: usize, taken: &[Improvement]) {
    for improvement in taken {
        assert!(
            game.public
                .improvement_market
                .iter()
                .any(|i| i == improvement),
            "{improvement:?} must be on the market"
        );
    }
    game.public
        .improvement_market
        .retain(|i| !taken.contains(i));
    game.public.players[player]
        .improvements
        .extend_from_slice(taken);
}

/// Engine proof: a 4-player game plays B153 through Lessons, and
/// `score_with_cards` pays the doubled-lowest bonus to the owner only.
#[test]
fn b153_engine_four_player_score_pays_owner_bonus_only() {
    let mut game = four_player_game_with_b153();
    assert_eq!(game.public.active_player, 0, "seat 0 starts");
    game.apply(Action {
        space: agricola_re_ai::model::Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "B153".to_owned(),
        },
    })
    .expect("B153 play via Lessons");
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B153"));

    // Owner: ClayOven 2 + StoneOven 3 + Well 4 (doubled total 11 -> +4).
    // Opponent: an equal 9-point set WITHOUT B153, so only the card differs.
    let owner_taken = [
        Improvement::ClayOven,
        Improvement::StoneOven,
        Improvement::Well,
    ];
    let other_taken = [
        Improvement::Joinery,
        Improvement::Pottery,
        Improvement::Basketry,
        Improvement::Fireplace2,
        Improvement::Fireplace3,
        Improvement::Hearth4,
    ];
    take_improvements(&mut game, 0, &owner_taken);
    take_improvements(&mut game, 1, &other_taken);

    let owner = score_with_cards(&game.public.players[0], &game.card_set);
    let other = score_with_cards(&game.public.players[1], &game.card_set);
    assert_eq!(
        owner.categories.get("improvements").copied(),
        Some(9),
        "ordinary category stays 2+3+4"
    );
    assert_eq!(
        owner.categories.get("card_bonus").copied(),
        Some(4),
        "owner gets the doubled-lowest bonus (doubled total 11)"
    );
    assert_eq!(
        other.categories.get("card_bonus").copied(),
        Some(0),
        "a non-owner never scores B153"
    );
    // Both players hold the same improvements; only the B153 owner is ahead.
    assert_eq!(
        other.categories.get("improvements").copied(),
        Some(9),
        "the opponent's ordinary points are untouched"
    );
    assert_eq!(owner.total, other.total + 4);
}

/// A 2-player game entry refuses the raw band-B set, which is how the
/// MinimumPlayers(4) constraint keeps B153 out of 2/3-player games.
#[test]
fn b153_two_player_game_entry_refuses_the_band_set() {
    let band = CardSet {
        definitions: verified_band_b_cards(),
    };
    let config = Config {
        players: 2,
        seed: 11,
        additional_action_space_enabled: false,
    };
    assert!(
        Game::with_cards(config, band).is_err(),
        "B153 is not eligible at 2 players"
    );
}

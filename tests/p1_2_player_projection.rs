//! Reproduction tests for the PR #368 close verdict (external static review):
//! "人数投影を全ゲーム入口で固定し未投影セットを拒否" — player-count
//! projection must be fixed at every game entry point, and unprojected card
//! sets must be rejected.
//!
//! Two findings are reproduced here (expected to be RED on the unmodified
//! tree and GREEN after the fix):
//!
//! (a) A raw (unprojected) B102 definition — the registry merge form that
//!     carries ALL player-count series (2-player Clay 3, 3-player Reed 2 and
//!     4-player Sheep 2 via OnPlayGainAnimal) behind paired
//!     MinimumPlayers/MaximumPlayers guards — fires every series when the card
//!     is played in a 2-player game, because neither `CardSet::validate` nor
//!     the effect dispatcher treats a paired guard as blocking at execution
//!     time.
//! (b) Handing a raw A155 (MinimumPlayers(4) whole-card prerequisite) to a
//!     2-player game entry (`Game::with_cards*`) is accepted today: the
//!     paired-guard parser in `CardSet::validate` silently skips the
//!     unsatisfied guard instead of rejecting the set.
//!
//! Also pinned here per the card body:
//! - the 2/3/4-player pool boundary of the 7-cards-per-player deal minimum
//!   for minor improvements (the 28/27 boundary),
//! - the honestly-empty official pool (`in_official` empty at every player
//!   count),
//! - the per-seat 7+7 deal and the card-use ledger increment of a played
//!   B102.

use agricola_re_ai::cards::{
    card_set_for_game, verified_basic_card_set, CardDefinition, CardEffect, CardKind,
    CardRequirement, CardSet,
};
use agricola_re_ai::model::{Config, Good, Space};
use agricola_re_ai::registry::build_default_registry;
use agricola_re_ai::Game;

/// The registry merge form of B102: every player-count series in one card,
/// each behind paired guards. This is what `merge_card_variants("B102", ..)`
/// produces and what any unprojected set built from the registry carries.
fn raw_b102_registry_form() -> CardDefinition {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry
        .card_id("B102")
        .expect("B102 is in the default registry");
    let card = registry.get(card_id).expect("B102 retrievable");
    card.to_card_definition()
}

/// The raw band-A form of A155: [MinimumPlayers(4), gain wood, gain grain].
fn raw_a155_definition() -> CardDefinition {
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    CardDefinition {
        use_limit: None,
        id: "A155".to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(4),
            },
            gain(Space::TravelingPlayers, Good::Wood, 1),
            gain(Space::TravelingPlayers, Good::Grain, 1),
        ],
        post_play: agricola_re_ai::cards::PostPlayDestination::PlayedArea,
    }
}

/// Build explicit 7+7 hands from a set. Seat 0 receives B102 (the only
/// seat-specific request); everything else is filled round-robin from the
/// set's definitions so every hand id exists in the set.
fn hands_with_b102_in_seat0(set: &CardSet) -> Vec<Vec<String>> {
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
        occupations.len() >= 14,
        "set needs 14 occupations for two seats"
    );
    assert!(
        minors.len() >= 14,
        "set needs 14 minor improvements for two seats"
    );

    let mut hands: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
    hands[0].push("B102".to_string());
    occupations.retain(|id| id != "B102");
    // Deal the remaining occupations round-robin until both seats hold 7.
    let mut seat = 1;
    for id in occupations {
        let target = if hands[seat].len() < 7 {
            seat
        } else {
            1 - seat
        };
        if hands[target].len() < 7 {
            hands[target].push(id);
        }
        seat = 1 - seat;
        if hands[0].len() == 7 && hands[1].len() == 7 {
            break;
        }
    }
    // Minors round-robin, 7 per seat.
    let mut seat = 0;
    for id in minors {
        if hands[seat].len() < 14 {
            hands[seat].push(id);
        }
        seat = 1 - seat;
    }
    hands
}

// =========================================================================
// Premise (a): raw B102 in a 2-player game fires BOTH series on play.
// =========================================================================

#[test]
fn raw_b102_def_carries_all_player_series() {
    // Fixture premise: the registry B102 carries one series per player count.
    let b102 = raw_b102_registry_form();
    assert_eq!(
        b102.effects
            .iter()
            .filter(|effect| matches!(effect, CardEffect::OnPlayGain { .. }))
            .count(),
        2,
        "fixture premise: registry B102 carries two OnPlayGain series (2p clay, 3p reed)"
    );
    assert_eq!(
        b102.effects
            .iter()
            .filter(|effect| matches!(effect, CardEffect::OnPlayGainAnimal { .. }))
            .count(),
        1,
        "fixture premise: registry B102 carries the 4p OnPlayGainAnimal sheep series"
    );
}

#[test]
fn raw_b102_both_series_fire_in_two_player_game() {
    // Take the legitimate two-player preset, then swap in the RAW registry
    // form of B102 (the unprojected set a raw entry would carry).
    let mut set = verified_basic_card_set(2).expect("two-player preset builds");
    let raw_b102 = raw_b102_registry_form();
    set.definitions.retain(|card| card.id != "B102");
    set.definitions.push(raw_b102);

    let hands = hands_with_b102_in_seat0(&set);
    let mut game = Game::with_cards_and_hands(
        Config {
            players: 2,
            seed: 7,
            additional_action_space_enabled: false,
        },
        set,
        hands,
    )
    .expect("entry accepts the set; the fixed tree projects it to 2 players (see raw_a155 test for the reject path)");

    // Play the raw B102 in seat 0 and measure the actually granted resources.
    let owner = 0;
    assert!(
        game.cards.hands[owner].contains(&"B102".to_string()),
        "fixture premise: seat 0 holds B102"
    );
    let before_clay = game.public.players[owner].supply.clay;
    let before_reed = game.public.players[owner].supply.reed;
    agricola_re_ai::cards::play(
        &game.card_set,
        &mut game.cards.hands[owner],
        &mut game.public.players[owner],
        "B102",
        CardKind::Occupation,
        0,
    )
    .expect("raw B102 is playable today");

    // FIXED behaviour: only the two-player clay series fires (clay 3, reed 0)
    // because the set handed to the game entry is projected to 2 players.
    // UNFIXED behaviour: both series fire (clay 3 AND reed 2) — RED.
    assert_eq!(
        (
            game.public.players[owner].supply.clay - before_clay,
            game.public.players[owner].supply.reed - before_reed
        ),
        (3, 0),
        "2-player game must grant only the 2p Clay 3 series of B102"
    );

    // The projected card carried into the game has only the 2p series:
    // the 3p Reed series must be absent from the game's own card set.
    let b102_in_game = game
        .card_set
        .definition("B102")
        .expect("B102 present in the game set");
    assert_eq!(
        b102_in_game
            .effects
            .iter()
            .filter(|effect| matches!(
                effect,
                CardEffect::OnPlayGain {
                    good: Good::Reed,
                    ..
                }
            ))
            .count(),
        0,
        "the 2-player game's B102 must not carry the 3p Reed series"
    );
    assert_eq!(
        b102_in_game
            .effects
            .iter()
            .filter(|effect| matches!(
                effect,
                CardEffect::OnPlayGain {
                    good: Good::Clay,
                    ..
                }
            ))
            .count(),
        1,
        "the 2-player game's B102 keeps exactly one 2p Clay series"
    );
    assert_eq!(
        b102_in_game
            .effects
            .iter()
            .filter(|effect| matches!(effect, CardEffect::OnPlayGainAnimal { .. }))
            .count(),
        0,
        "the 2-player game's B102 must not carry the 4p Sheep series"
    );
}

#[test]
fn normal_entry_b102_grants_only_two_player_series() {
    // The pooled entry (card_set_for_game) projects effects for the player
    // count, so a 2-player game from the normal entry grants only Clay 3.
    // B102 is experimental-pool eligible at 2 players.
    let set = card_set_for_game(2, true);
    assert!(set.definitions.iter().any(|c| c.id == "B102"));
    let mut game = Game::with_cards(
        Config {
            players: 2,
            seed: 7,
            additional_action_space_enabled: false,
        },
        set,
    )
    .expect("deal from pooled entry");

    // Seed a guaranteed B102 into seat 0's hand for a deterministic play.
    let owner = 0;
    let b102_seat = game
        .cards
        .hands
        .iter()
        .position(|hand| hand.iter().any(|id| id == "B102"))
        .unwrap_or(owner);
    game.cards.hands[b102_seat].retain(|id| id != "B102");
    game.cards.hands[owner].insert(0, "B102".to_string());

    let before_clay = game.public.players[owner].supply.clay;
    let before_reed = game.public.players[owner].supply.reed;
    agricola_re_ai::cards::play(
        &game.card_set,
        &mut game.cards.hands[owner],
        &mut game.public.players[owner],
        "B102",
        CardKind::Occupation,
        0,
    )
    .expect("B102 is playable");
    assert_eq!(game.public.players[owner].supply.clay - before_clay, 3);
    assert_eq!(game.public.players[owner].supply.reed - before_reed, 0);
}

// =========================================================================
// Premise (b): raw A155 added to the 2-player set is accepted today.
// =========================================================================

#[test]
fn raw_a155_is_rejected_by_two_player_entry() {
    let mut set = verified_basic_card_set(2).expect("two-player preset builds");
    set.definitions.push(raw_a155_definition());

    // UNFIXED behaviour (RED expectation): accepted. FIXED: rejected, because
    // the card is not eligible at 2 players (whole-card MinimumPlayers(4)
    // prerequisite makes it unprojectable for this game).
    let result = Game::with_cards(
        Config {
            players: 2,
            seed: 7,
            additional_action_space_enabled: false,
        },
        set,
    );
    assert!(
        result.is_err(),
        "a 2-player game entry must reject a set carrying raw A155 (4-player-only card)"
    );
}

#[test]
fn normal_entry_pool_excludes_a155_at_two_players() {
    // The pooled entry already excludes A155 at 2 players (registry eligibility).
    for experimental in [false, true] {
        let set = card_set_for_game(2, experimental);
        assert!(
            set.definitions.iter().all(|card| card.id != "A155"),
            "experimental={experimental}: pooled 2-player set must not carry A155"
        );
    }
}

// =========================================================================
// Pinned boundary: the 7-cards-per-player deal minimum for minor improvements.
// =========================================================================

#[test]
fn minor_improvement_deal_minimum_boundary_is_pinned() {
    // The implemented catalog currently exposes many minor improvements on
    // this tree (main merged new minor cards after the 65-card era this
    // fix's intake described — the 28/27 boundary this card cites was the
    // intake-time count). Pin the CURRENT boundary honestly: users of the
    // boundary are the 7-per-seat deal minimums 14/21/28 for 2/3/4 players.
    let registry = build_default_registry().expect("registry");
    let minors: Vec<CardDefinition> = registry
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .map(|card| card.to_card_definition())
        .collect();
    assert!(
        minors.len() >= 28,
        "expected at least the intake-time 28 minor improvements (got {})",
        minors.len()
    );
    let current_minors = minors.len();

    // validate enforces 7*players of EACH kind, so pair every scenario with
    // enough filler occupations (7*4 = 28) to isolate the minor count.
    let filler_occupations: Vec<CardDefinition> = (0..28)
        .map(|index| CardDefinition {
            use_limit: None,
            id: format!("FILLER_OCC_{index}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: agricola_re_ai::cards::PostPlayDestination::PlayedArea,
        })
        .collect();

    // With the full minor catalog: validate passes at 2/3/4 players
    // (14/21/28 needed; the current catalog exceeds every minimum).
    let with_all = CardSet {
        definitions: minors
            .clone()
            .into_iter()
            .chain(filler_occupations.clone())
            .collect(),
    };
    // The RAW catalog is rejected at every player count: D035 is the first
    // minor improvement whose merged form carries raw Minimum/MaximumPlayers
    // guard runs, and `CardSet::validate` by design (PR #368) rejects an
    // unprojected set (B102/A133 are occupations, so this minor-only test
    // never met that shape before D035). Pin the regression cause so the
    // projection below stays necessary.
    for players in [2usize, 3, 4] {
        assert!(
            with_all.validate(players).is_err(),
            "raw unprojected catalog must be rejected at {players} players (D035 player guards)"
        );
    }
    // Every game entry projects exactly once (PR #368); the deal minimum is
    // judged on the PROJECTED set, which keeps the 7-per-seat intent above.
    for players in [2usize, 3, 4] {
        let projected = with_all
            .projected_for_players(players)
            .expect("full minor catalog must project at every player count");
        assert!(
            projected.validate(players).is_ok(),
            "{current_minors} minors + 28 occupations must validate at {players} players"
        );
    }

    // One minor short of the 4-player minimum: with the current catalog the
    // deal minimum fails exactly when the minor count drops below 28.
    // (The transmission case needs the count reduced to 27, so remove down
    // to 27 and confirm 4p fails while 2p/3p pass.)
    let mut with_27 = with_all;
    while with_27
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .count()
        > 27
    {
        let minor_position = with_27
            .definitions
            .iter()
            .position(|card| card.kind == CardKind::MinorImprovement)
            .expect("a minor to remove");
        with_27.definitions.remove(minor_position);
    }
    assert_eq!(
        with_27
            .definitions
            .iter()
            .filter(|card| card.kind == CardKind::MinorImprovement)
            .count(),
        27
    );
    // The 28/27 boundary is judged on the PROJECTED set at each player
    // count, exactly like the full-catalog check above (validate alone sees
    // D035's raw player-count guard runs and rejects before reaching the
    // deal minimum).
    let projected_27_4p = with_27
        .projected_for_players(4)
        .expect("27 minors must project at 4 players");
    assert!(
        projected_27_4p.validate(4).is_err(),
        "27 minors must fail 4 players (28 needed)"
    );
    let projected_27_3p = with_27
        .projected_for_players(3)
        .expect("27 minors must project at 3 players");
    assert!(
        projected_27_3p.validate(3).is_ok(),
        "27 minors pass 3 players (21 needed)"
    );
    let projected_27_2p = with_27
        .projected_for_players(2)
        .expect("27 minors must project at 2 players");
    assert!(
        projected_27_2p.validate(2).is_ok(),
        "27 minors pass 2 players (14 needed)"
    );
}

// =========================================================================
// Pinned boundary: the official pool is honestly empty (in_official = []).
// =========================================================================

#[test]
fn official_pool_is_empty_at_every_player_count() {
    for players in [2usize, 3, 4] {
        let official = card_set_for_game(players, false);
        assert!(
            official.definitions.is_empty(),
            "official pool must be honestly empty at {players} players"
        );
        assert!(!official.enabled());
        // An empty set validates OK (cardless mode is legal).
        assert!(official.validate(players).is_ok());
    }
}

// =========================================================================
// Ledger/deal pins: per-seat 7+7 deal.
// =========================================================================

#[test]
fn two_player_preset_deals_seven_plus_seven_per_seat() {
    let set = verified_basic_card_set(2).expect("preset");
    let game = Game::with_cards(
        Config {
            players: 2,
            seed: 42,
            additional_action_space_enabled: false,
        },
        set,
    )
    .expect("deal");
    for hand in &game.cards.hands {
        assert_eq!(hand.len(), 14, "each seat deals 7 occupations + 7 minors");
    }
    let observation = game.observation(0).expect("observation");
    assert_eq!(observation.hand_counts, vec![14, 14]);
}

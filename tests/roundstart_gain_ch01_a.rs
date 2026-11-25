//! Tests for the A-band round-start gain card implemented in this chapter:
//! A081 一時保管庫 (Interim Storage).
//!
//! A081 stores 1 wood / clay / reed on itself whenever the owner uses the
//! Clay Pit / Reed Bank / either quarry, and moves every stored good to the
//! owner's supply at the start of rounds 7, 11 and 14.

use agricola_re_ai::card_events::dispatch_after_worker_with_card_storage;
use agricola_re_ai::cards::{
    card_definition, card_set_for_game, verified_band_a_cards, CardEffect, CardKind, CardSet,
    CardState,
};
use agricola_re_ai::model::{Config, Game, Good, Player, Space};
use std::collections::{BTreeMap, BTreeSet};

type Ledger = BTreeMap<usize, BTreeMap<String, BTreeMap<Good, u32>>>;

fn a081() -> agricola_re_ai::cards::CardDefinition {
    verified_band_a_cards()
        .into_iter()
        .find(|c| c.id == "A081")
        .expect("A081 must be defined in the band A pool")
}

#[test]
fn a081_definition_matches_spec() {
    let card = a081();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Food, 2)]);
    assert_eq!(card.points, 0);

    let mut stacked = Vec::new();
    for effect in &card.effects {
        if let CardEffect::AfterWorkerStackOnCard {
            space,
            good,
            amount,
        } = effect
        {
            stacked.push((*space, *good, *amount));
        }
    }
    stacked.sort_by_key(|(space, _, _)| format!("{space:?}"));
    assert_eq!(
        stacked,
        vec![
            (Space::ClayPit, Good::Wood, 1),
            (Space::EasternQuarry, Good::Reed, 1),
            (Space::ReedBank, Good::Clay, 1),
            (Space::WesternQuarry, Good::Reed, 1),
        ],
        "spec A081: clay pit -> wood, reed bank -> clay, both quarries -> reed"
    );

    assert!(
        card.effects.iter().any(|effect| matches!(
            effect,
            CardEffect::StartRoundReleaseCardGoods { rounds } if rounds == &vec![7, 11, 14]
        )),
        "spec A081: the card empties at the start of rounds 7, 11 and 14"
    );

    // The default registry must expose the same definition.
    let registry_card = card_definition("A081").expect("A081 must be registered");
    assert_eq!(registry_card.kind, card.kind);
    assert_eq!(registry_card.cost, card.cost);
    assert_eq!(registry_card.points, card.points);
    assert_eq!(registry_card.effects, card.effects);
}

/// Dispatch a single worker completion for `actor` and return the ledger.
fn stack_on(space: Space, actor: usize, ledger: &mut Ledger, pending_out: &mut bool) {
    let set = CardSet {
        definitions: verified_band_a_cards(),
    };
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    players[0].played_cards.push("A081".into());
    let mut cards = CardState {
        hands: vec![Vec::new(), Vec::new()],
        undealt: Vec::new(),
    };
    let mut market = Vec::new();
    let mut well = BTreeMap::new();
    let mut used = BTreeSet::new();
    used.insert(space);
    let mut budget = 16;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    dispatch_after_worker_with_card_storage(
        &set,
        actor,
        space,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        2,
        &used,
        &BTreeMap::new(),
        &BTreeMap::new(),
        Some(ledger),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        false,
    )
    .expect("after-worker dispatch must succeed");
    *pending_out = pending.is_some();
}

#[test]
fn a081_stacks_the_mapped_good_on_each_accumulation_space_use() {
    for (space, expected) in [
        (Space::ClayPit, Good::Wood),
        (Space::ReedBank, Good::Clay),
        (Space::WesternQuarry, Good::Reed),
        (Space::EasternQuarry, Good::Reed),
    ] {
        let mut ledger: Ledger = BTreeMap::new();
        let mut pending = false;
        stack_on(space, 0, &mut ledger, &mut pending);
        assert!(!pending, "A081 stores inline, no decision");
        assert_eq!(
            ledger
                .get(&0)
                .and_then(|cards| cards.get("A081"))
                .and_then(|goods| goods.get(&expected))
                .copied(),
            Some(1),
            "{space:?} must store 1 {expected:?} on A081"
        );
        assert_eq!(
            ledger.get(&0).and_then(|c| c.get("A081")).map(|g| g.len()),
            Some(1),
            "exactly one good type is stored per use"
        );
    }
}

#[test]
fn a081_does_not_stack_for_unmapped_spaces_or_other_players() {
    // Forest is a wood accumulation space but not one of the four listed.
    let mut ledger: Ledger = BTreeMap::new();
    let mut pending = false;
    stack_on(Space::Forest, 0, &mut ledger, &mut pending);
    assert!(ledger.is_empty(), "Forest is not an A081 space");

    // Player 1 uses the Clay Pit; only player 0 owns A081.
    let mut ledger: Ledger = BTreeMap::new();
    stack_on(Space::ClayPit, 1, &mut ledger, &mut pending);
    assert!(
        ledger.is_empty(),
        "A081 must not store goods on another player's use"
    );
}

fn a081_game() -> Game {
    let config = Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    };
    let mut game =
        Game::with_cards(config, card_set_for_game(2, true)).expect("card game construction");
    game.public.players[0].played_cards.push("A081".into());
    game
}

#[test]
fn a081_releases_all_stored_goods_at_rounds_7_11_and_14_only() {
    let mut game = a081_game();
    // Round 1 is opened during construction; the first start_round is round 2.
    for _ in 0..5 {
        game.start_round().expect("advance to round 6");
    }
    assert_eq!(game.public.round, 6);

    game.public.card_accumulations.insert(
        0,
        BTreeMap::from([(
            "A081".to_string(),
            BTreeMap::from([(Good::Wood, 2), (Good::Clay, 1), (Good::Reed, 3)]),
        )]),
    );
    let before = game.public.players[0].supply.clone();

    game.start_round().expect("advance to round 7");
    assert_eq!(game.public.round, 7);
    assert_eq!(game.public.players[0].supply.wood, before.wood + 2);
    assert_eq!(game.public.players[0].supply.clay, before.clay + 1);
    assert_eq!(game.public.players[0].supply.reed, before.reed + 3);
    assert!(
        !game.public.card_accumulations.contains_key(&0),
        "the card must be empty after the release"
    );

    // Nothing is stored again, so the next round start grants nothing.
    let after_release = game.public.players[0].supply.clone();
    game.start_round().expect("advance to round 8");
    assert_eq!(game.public.players[0].supply.wood, after_release.wood);
    assert_eq!(game.public.players[0].supply.clay, after_release.clay);
    assert_eq!(game.public.players[0].supply.reed, after_release.reed);

    // Rounds 11 and 14 are the remaining release points.
    for target in [11u8, 14u8] {
        while game.public.round + 1 < target {
            game.start_round().expect("advance before the next release");
        }
        game.public.card_accumulations.insert(
            0,
            BTreeMap::from([("A081".to_string(), BTreeMap::from([(Good::Wood, 1)]))]),
        );
        let before = game.public.players[0].supply.wood;
        game.start_round().expect("advance into the release round");
        assert_eq!(game.public.round, target);
        assert_eq!(
            game.public.players[0].supply.wood,
            before + 1,
            "round {target} must release the stored wood"
        );
        assert!(!game.public.card_accumulations.contains_key(&0));
    }
}

#[test]
fn a081_stores_nothing_when_the_ledger_is_not_threaded() {
    // The legacy dispatcher entry point (no ledger) must stay a no-op rather
    // than panic, so callers that only model supplies keep working.
    let set = CardSet {
        definitions: verified_band_a_cards(),
    };
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    players[0].played_cards.push("A081".into());
    let mut cards = CardState {
        hands: vec![Vec::new(), Vec::new()],
        undealt: Vec::new(),
    };
    let mut market = Vec::new();
    let mut well = BTreeMap::new();
    let mut used = BTreeSet::new();
    used.insert(Space::ClayPit);
    let mut budget = 16;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    agricola_re_ai::card_events::dispatch_after_worker(
        &set,
        0,
        Space::ClayPit,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        2,
        &used,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        false,
    )
    .expect("legacy dispatch must succeed");
    assert!(pending.is_none());
    assert_eq!(players[0].supply.wood, 0);
}

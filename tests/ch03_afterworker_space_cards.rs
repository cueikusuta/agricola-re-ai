//! Tests for ch03 第3章 cards (A015/A018/A021/A023/A024) - after_worker_space cards.
//!
//! These tests exercise the REAL engine/dispatch path (`dispatch_after_worker` +
//! pending resolution) and assert that effects actually complete (stable built,
//! fields plowed, improvement played, bread baked), not just that the handler
//! returns `Ok()`.

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{
    verified_after_worker_space_cards, CardDefinition, CardEffect, CardKind, CardRequirement,
    CardSet,
};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, House, Improvement, RoundCard, Space, Tile};
use agricola_re_ai::pending::{ImprovementOption, PendingKind, PendingResolution, Transition};
use agricola_re_ai::registry::{build_default_registry, card_set_from_pool, CardPool};

/// Build a CardSet containing exactly the 5 ch03 cards for targeted testing.
fn ch03_card_set() -> CardSet {
    CardSet {
        definitions: verified_after_worker_space_cards(),
    }
}

/// Check if a card definition is eligible for the given player count
/// (no `MinimumPlayers > players`).
fn card_eligible_for_players(card: &CardDefinition, players: usize) -> bool {
    for effect in &card.effects {
        if let CardEffect::Prerequisite {
            rule: CardRequirement::MinimumPlayers(min),
        } = effect
        {
            if *min > 0 && *min as usize > players {
                return false;
            }
        }
    }
    true
}

/// Create a filler card for padding card sets to meet the 7 cards/kind/player requirement.
fn filler_card(kind: CardKind, index: usize) -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: format!("FILLER_{}_{}", kind as u8, index),
        kind,
        cost: vec![],
        points: 0,
        effects: vec![],
        post_play: agricola_re_ai::cards::PostPlayDestination::PlayedArea,
    }
}

/// Build a valid CardSet for the given player count using the experimental pool
/// from the default registry, filtered to only include cards eligible for the player count.
fn valid_card_set_for_players(players: usize) -> CardSet {
    let registry = build_default_registry().expect("default registry builds");
    let pool = CardPool::new(&registry, players, true); // experimental_mode = true
    let mut full_set = card_set_from_pool(&registry, &pool);

    // Filter out cards that have MinimumPlayers > players
    full_set
        .definitions
        .retain(|card| card_eligible_for_players(card, players));

    // Add ch03 cards if they're eligible for this player count
    for ch03_card in ch03_card_set().definitions {
        if card_eligible_for_players(&ch03_card, players)
            && !full_set.definitions.iter().any(|c| c.id == ch03_card.id)
        {
            full_set.definitions.push(ch03_card);
        }
    }

    // Ensure we have at least 7 cards of each kind per player by adding filler cards
    let min_per_kind = 7 * players;
    for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
        let count = full_set
            .definitions
            .iter()
            .filter(|c| c.kind == kind)
            .count();
        if count < min_per_kind {
            for i in count..min_per_kind {
                full_set.definitions.push(filler_card(kind, i));
            }
        }
    }

    // Validate the card set
    full_set
        .validate(players)
        .expect("filtered card set should be valid");
    full_set
}

/// Build a game with a valid card set for the given player count.
fn game_with_valid_cards(players: usize, seed: u64) -> Game {
    let card_set = valid_card_set_for_players(players);
    Game::with_cards(
        Config {
            players,
            seed,
            additional_action_space_enabled: false,
        },
        card_set,
    )
    .unwrap()
}

/// MOVE `card_id` into `player`'s played area (a real play). The dealt hands
/// hold only seven cards of each kind per player, so the instance is taken
/// from whichever zone holds it — hand or `undealt`. A card instance must
/// live in exactly one zone for the engine's card-state validation, which
/// the committed `apply` path enforces; the printed points join the player's
/// card score, exactly as a real play records them.
fn move_card_to_played_area(game: &mut Game, player: usize, card_id: &str) {
    let mut removed = false;
    for hand in &mut game.cards.hands {
        if let Some(pos) = hand.iter().position(|id| id == card_id) {
            hand.remove(pos);
            removed = true;
            break;
        }
    }
    if !removed {
        if let Some(pos) = game.cards.undealt.iter().position(|id| id == card_id) {
            game.cards.undealt.remove(pos);
            removed = true;
        }
    }
    assert!(removed, "{card_id} must exist in the card state");
    game.public.players[player]
        .played_cards
        .push(card_id.to_string());
    let points = game
        .card_set
        .definition(card_id)
        .map(|card| card.points)
        .unwrap_or(0);
    agricola_re_ai::cards::update_card_points_on_play(&mut game.public.players[player], points)
        .expect("card points update");
}

/// Helper: the currently active player owns `card_id` (in its played area).
fn setup_active_player_with_card(game: &mut Game, card_id: &str) -> usize {
    let player = game.public.active_player;
    move_card_to_played_area(game, player, card_id);
    player
}

/// Move a SECOND card into the same player's played area.
fn play_second_card(game: &mut Game, player: usize, card_id: &str) {
    move_card_to_played_area(game, player, card_id);
}

/// Create a game + active player owning `card_id`.
fn dispatch_setup(card_id: &str, players: usize) -> (Game, usize) {
    let mut game = game_with_valid_cards(players, 42);
    let owner = setup_active_player_with_card(&mut game, card_id);
    (game, owner)
}

/// Dispatch the after-worker effect for `player` placing on `space`.
/// `start_player` controls the first-person-of-round condition (actor == start_player).
fn dispatch_aw(
    game: &mut Game,
    player: usize,
    space: Space,
    start_player: usize,
) -> Result<Transition, String> {
    dispatch_after_worker(
        &game.card_set,
        player,
        space,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        start_player,
        &game.public.used,
        &game.public.accumulations,
        &game.public.animal_accumulations,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &mut game.public.card_uses_game,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
}

/// Simulate the engine's pause-commit (which stamps `state_hash_before`) and then
/// resolve the pending decision. Direct `dispatch_after_worker` leaves
/// `state_hash_before = ""`, so without this stamping `resolve_pending` rejects
/// with a hash mismatch.
fn stamp_and_resolve(game: &mut Game, resolution: PendingResolution) -> Result<Transition, String> {
    let hash = game.state_hash();
    if let Some(pending) = game.pending.as_mut() {
        pending.state_hash_before = hash;
    }
    game.resolve_pending_and_resume(resolution)
}

/// Extract the valid stable sites from an A015 BuildStable pending.
fn build_stable_sites(game: &Game) -> Vec<usize> {
    match &game.pending().unwrap().kind {
        PendingKind::BuildStable { valid_sites, .. } => valid_sites.clone(),
        _ => panic!("expected BuildStable pending"),
    }
}

/// Extract the valid plow sites from an A018 PlowFields pending.
fn plow_sites(game: &Game) -> Vec<usize> {
    match &game.pending().unwrap().kind {
        PendingKind::PlowFields { valid_sites, .. } => valid_sites.clone(),
        _ => panic!("expected PlowFields pending"),
    }
}

/// Extract the improvement options from an A023 ImprovementAction pending.
fn improvement_options(game: &Game) -> Vec<ImprovementOption> {
    match &game.pending().unwrap().kind {
        PendingKind::ImprovementAction {
            available_improvements,
            ..
        } => available_improvements.clone(),
        _ => panic!("expected ImprovementAction pending"),
    }
}

// ========================================================================
// Step 1: Registry/construction path reachability tests
// ========================================================================

#[test]
fn ch03_cards_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");

    for id in ["A015", "A018", "A021", "A023", "A024"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must have a CardId"));
        let card = registry
            .get(card_id)
            .unwrap_or_else(|| panic!("{id} must be retrievable by CardId"));
        assert_eq!(card.external_id, id, "round-trip external_id for {id}");
        assert_eq!(
            card.kind,
            CardKind::MinorImprovement,
            "{id} must be MinorImprovement"
        );
    }
}

#[test]
fn ch03_cards_registry_event_index_populated() {
    let registry = build_default_registry().expect("default registry builds");

    let candidates = registry.event_candidates(agricola_re_ai::registry::EventKind::WorkerPlaced);
    let candidate_ids: std::collections::BTreeSet<_> = candidates.iter().copied().collect();

    for id in ["A015", "A018", "A021", "A023", "A024"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must have CardId"));
        assert!(
            candidate_ids.contains(&card_id),
            "{id} must be in WorkerPlaced event index"
        );
    }
}

#[test]
fn ch03_cards_are_implemented_but_not_officially_verified() {
    let registry = build_default_registry().expect("default registry builds");

    for id in ["A015", "A018", "A021", "A023", "A024"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} in registry"));
        let review = registry
            .review(card_id)
            .unwrap_or_else(|| panic!("{id} has review"));
        // Implementation is complete (the cards sit in the registry with
        // effects), but the official verification pass has not granted
        // Verified yet, so they stay out of the official-eligible pool.
        assert_eq!(
            review.review_status,
            agricola_re_ai::card_pool::ReviewStatus::Unverified,
            "{id} should be Unverified in default registry"
        );
        for players in [2usize, 3, 4] {
            assert_eq!(
                review.eligibility_at(players),
                Some(agricola_re_ai::card_pool::Eligibility::Yes),
                "{id} should be eligible at {players} players"
            );
        }
    }
}

#[test]
fn pool_excludes_all_ch03_cards_due_to_unverified_status() {
    let registry = build_default_registry().expect("default registry builds");

    for players in [2usize, 3, 4] {
        let pool = CardPool::new(&registry, players, false);
        for id in ["A015", "A018", "A021", "A023", "A024"] {
            let card_id = registry
                .card_id(id)
                .unwrap_or_else(|| panic!("{id} in registry"));
            assert!(
                !pool.contains(card_id),
                "{id} must not be in official pool at {players} players (unverified)"
            );
        }
    }
}

// ========================================================================
// Step 2: Space-specific triggering + effect completion tests
// ========================================================================

// ---- A015 大工の手斧 (Carpenter's Axe) ----
// Cost 1 wood. After Forest/Copse/Grove with 7+ wood in supply: optionally
// build 1 stable for 1 wood. (Typed BuildStable pending.)

#[test]
fn a015_triggers_on_wood_space_creates_build_stable_pending() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    let transition = dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
    assert!(matches!(transition, Transition::Paused(_)));
    assert!(game.pending.is_some());
    let sites = build_stable_sites(&game);
    assert!(
        !sites.is_empty(),
        "A015 must offer at least one stable site on Forest"
    );
}

#[test]
fn a015_resolve_build_stable_actually_builds_and_pays_wood() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
    let sites = build_stable_sites(&game);
    assert!(!sites.is_empty());

    let before_wood = game.public.players[owner].supply.wood;
    // The resolution addresses the offered candidate list by position;
    // position 0 maps back to the farm site `sites[0]`.
    let transition = stamp_and_resolve(
        &mut game,
        PendingResolution::BuildStable {
            site_index: Some(0),
        },
    )
    .unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    // Stable built at the chosen site
    assert!(
        game.public.players[owner].farm[sites[0]].stable,
        "stable must be built at chosen site"
    );
    // Paid 1 wood
    assert_eq!(
        game.public.players[owner].supply.wood,
        before_wood - 1,
        "A015 must charge 1 wood for the stable"
    );
}

#[test]
fn a015_decline_leaves_farm_and_wood_untouched() {
    // Spec A015 is `optional: true`: declining builds nothing and pays nothing.
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
    assert!(game.pending.is_some());

    let before_wood = game.public.players[owner].supply.wood;
    let before_stables = game.public.players[owner].occupied_stables();
    let transition = stamp_and_resolve(
        &mut game,
        PendingResolution::BuildStable { site_index: None },
    )
    .unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    assert_eq!(
        game.public.players[owner].supply.wood, before_wood,
        "declining must not charge wood"
    );
    assert_eq!(
        game.public.players[owner].occupied_stables(),
        before_stables,
        "declining must not build a stable"
    );
}

#[test]
fn a015_does_not_trigger_below_7_wood() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 6;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    let transition = dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    assert_eq!(game.public.players[owner].supply.wood, 6);
}

#[test]
fn a015_does_not_trigger_on_non_wood_space() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    for space in [Space::Farmland, Space::ClayPit, Space::DayLaborer] {
        let transition = dispatch_aw(&mut game, owner, space, 0).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "A015 must not trigger on {space:?}"
        );
        assert!(
            game.pending.is_none(),
            "A015 must not create pending on {space:?}"
        );
    }
}

// ---- A018 車輪鋤 (Wheel Plow) ----
// Cost 2 wood. Once per game: when the first person of round uses Farmland or
// Cultivation, plow 2 fields. (Typed PlowFields pending; first_person_of_round
// + once_per_game enforced in real state.)

#[test]
fn a018_first_person_triggers_plow_pending() {
    let (mut game, owner) = dispatch_setup("A018", 2);
    game.public.players[owner].farm[1].tile = Tile::Empty;
    game.public.players[owner].farm[1].stable = false;
    game.public.players[owner].farm[2].tile = Tile::Empty;
    game.public.players[owner].farm[2].stable = false;

    // owner is the start player of round 1 -> first person of round
    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert!(matches!(transition, Transition::Paused(_)));
    assert!(matches!(
        game.pending().unwrap().kind,
        PendingKind::PlowFields { .. }
    ));
}

#[test]
fn a018_resolve_plow_fields_actually_plows_two_fields() {
    let (mut game, owner) = dispatch_setup("A018", 2);
    // Fresh farm: all Empty -> all sites valid for plowing
    dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    let sites = plow_sites(&game);
    assert!(sites.len() >= 2, "A018 must offer at least 2 plow sites");

    let before_fields = game.public.players[owner]
        .farm
        .iter()
        .filter(|c| matches!(c.tile, Tile::Field { .. }))
        .count();

    // The resolution addresses the offered candidate list by position;
    // positions 0 and 1 map back to the farm sites `sites[0]` / `sites[1]`.
    let transition = stamp_and_resolve(
        &mut game,
        PendingResolution::PlowFields {
            site_indices: vec![0, 1],
        },
    )
    .unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());

    // Exactly the two chosen cells became fields — not merely "two fields".
    assert!(
        matches!(
            game.public.players[owner].farm[sites[0]].tile,
            Tile::Field { .. }
        ),
        "first offered plow site must become a field"
    );
    assert!(
        matches!(
            game.public.players[owner].farm[sites[1]].tile,
            Tile::Field { .. }
        ),
        "second offered plow site must become a field"
    );
    let after_fields = game.public.players[owner]
        .farm
        .iter()
        .filter(|c| matches!(c.tile, Tile::Field { .. }))
        .count();
    assert_eq!(
        after_fields,
        before_fields + 2,
        "A018 must plow exactly 2 fields"
    );
}

#[test]
fn a018_decline_leaves_farm_untouched() {
    // Spec A018 is `optional: true`: declining plows nothing.
    let (mut game, owner) = dispatch_setup("A018", 2);
    dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    let sites = plow_sites(&game);
    assert!(sites.len() >= 2);

    let before_fields = game.public.players[owner]
        .farm
        .iter()
        .filter(|c| matches!(c.tile, Tile::Field { .. }))
        .count();

    let transition = stamp_and_resolve(
        &mut game,
        PendingResolution::PlowFields {
            site_indices: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    let after_fields = game.public.players[owner]
        .farm
        .iter()
        .filter(|c| matches!(c.tile, Tile::Field { .. }))
        .count();
    assert_eq!(after_fields, before_fields, "declining must not plow");
}

#[test]
fn a018_too_few_plow_sites_does_not_trigger() {
    // The spec plows exactly 2 fields; a farm with fewer legal sites leaves
    // nothing to decide, so no pending is created (a pending the player could
    // not legally resolve would be a deadlock).
    let (mut game, owner) = dispatch_setup("A018", 2);
    // Fill the farm with rooms except exactly one empty tile.
    for cell in &mut game.public.players[owner].farm {
        cell.tile = Tile::Room;
        cell.stable = false;
    }
    game.public.players[owner].farm[3].tile = Tile::Empty;

    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(
        game.pending.is_none(),
        "A018 must not create a pending it cannot satisfy"
    );
}

#[test]
fn a018_once_per_game_second_use_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A018", 2);
    game.public.players[owner].farm[1].tile = Tile::Empty;
    game.public.players[owner].farm[1].stable = false;
    game.public.players[owner].farm[2].tile = Tile::Empty;
    game.public.players[owner].farm[2].stable = false;

    // First use: triggers
    dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert!(game.pending.is_some());
    assert_eq!(
        game.public.card_uses_game.get("A018"),
        Some(&1),
        "first use must mark the once-per-game counter"
    );
    stamp_and_resolve(
        &mut game,
        PendingResolution::PlowFields {
            site_indices: vec![0, 1],
        },
    )
    .unwrap();
    assert!(game.pending.is_none());

    // Second use: once per game exhausted -> no trigger, no pending
    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
}

#[test]
fn a018_non_starter_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A018", 2);
    game.public.players[owner].farm[1].tile = Tile::Empty;
    game.public.players[owner].farm[1].stable = false;
    game.public.players[owner].farm[2].tile = Tile::Empty;
    game.public.players[owner].farm[2].stable = false;

    // actor (owner=0) is NOT the start player (start_player=1)
    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 1).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
}

#[test]
fn a018_does_not_trigger_on_non_farmland_space() {
    let (mut game, owner) = dispatch_setup("A018", 2);
    for space in [Space::Forest, Space::ClayPit, Space::DayLaborer] {
        let transition = dispatch_aw(&mut game, owner, space, 0).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "A018 must not trigger on {space:?}"
        );
        assert!(
            game.pending.is_none(),
            "A018 must not create pending on {space:?}"
        );
    }
}

// ---- A021 家族向け住宅 (Family Friendly Home) ----
// Cost 0. Prerequisite: 1+ occupation. After Build Rooms while rooms > people:
// Family Growth + 1 food. (Automatic, no pending.)
//
// NOTE: Build Rooms is Space::FarmExpansion, NOT FamilyGrowthWithRoom. The old
// implementation wrongly bound to FamilyGrowthWithRoom.

#[test]
fn a021_build_rooms_rooms_exceed_people_triggers_family_growth_and_food() {
    let (mut game, owner) = dispatch_setup("A021", 2);
    // rooms (2) > people (1)
    game.public.players[owner].house = House::Wood;
    game.public.players[owner].farm[0].tile = Tile::Room;
    game.public.players[owner].farm[1].tile = Tile::Room;
    game.public.players[owner].family = 1;

    let before_food = game.public.players[owner].supply.food;
    let before_family = game.public.players[owner].family;

    // Build Rooms is Space::FarmExpansion
    let transition = dispatch_aw(&mut game, owner, Space::FarmExpansion, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none(), "A021 is automatic, no pending");
    assert_eq!(
        game.public.players[owner].supply.food,
        before_food + 1,
        "A021 must grant 1 food"
    );
    assert_eq!(
        game.public.players[owner].family,
        before_family + 1,
        "A021 must trigger Family Growth (+1 family)"
    );
}

#[test]
fn a021_rooms_not_exceed_people_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A021", 2);
    // rooms (1) <= people (2)
    game.public.players[owner].house = House::Wood;
    game.public.players[owner].farm[0].tile = Tile::Room;
    game.public.players[owner].family = 2;

    let before_food = game.public.players[owner].supply.food;
    let before_family = game.public.players[owner].family;

    let transition = dispatch_aw(&mut game, owner, Space::FarmExpansion, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    assert_eq!(game.public.players[owner].supply.food, before_food);
    assert_eq!(game.public.players[owner].family, before_family);
}

#[test]
fn a021_family_growth_space_does_not_trigger() {
    // Regression: A021 must trigger on Build Rooms (FarmExpansion), NOT on
    // FamilyGrowthWithRoom. The old implementation wrongly bound to
    // FamilyGrowthWithRoom and fired after Family Growth instead of Build Rooms.
    let (mut game, owner) = dispatch_setup("A021", 2);
    game.public.players[owner].house = House::Wood;
    game.public.players[owner].farm[0].tile = Tile::Room;
    game.public.players[owner].farm[1].tile = Tile::Room;
    game.public.players[owner].family = 1;

    let family_before = game.public.players[owner].family;
    let food_before = game.public.players[owner].supply.food;

    let transition = dispatch_aw(&mut game, owner, Space::FamilyGrowthWithRoom, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    assert_eq!(
        game.public.players[owner].family, family_before,
        "A021 must not grow family on FamilyGrowthWithRoom"
    );
    assert_eq!(
        game.public.players[owner].supply.food, food_before,
        "A021 must not add food on FamilyGrowthWithRoom"
    );
}

#[test]
fn a021_non_build_space_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A021", 2);
    game.public.players[owner].house = House::Wood;
    game.public.players[owner].farm[0].tile = Tile::Room;
    game.public.players[owner].farm[1].tile = Tile::Room;
    game.public.players[owner].family = 1;

    for space in [Space::Forest, Space::Farmland, Space::ClayPit] {
        let transition = dispatch_aw(&mut game, owner, space, 0).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "A021 must not trigger on {space:?}"
        );
        assert!(
            game.pending.is_none(),
            "A021 must not create pending on {space:?}"
        );
    }
}

// ---- A023 採石事業所 (Stone Company) ----
// Cost 2 clay + 1 reed. After Western/Eastern Quarry: Improvement action that
// must spend 1+ stone. (Typed ImprovementAction pending.)

#[test]
fn a023_quarry_triggers_improvement_action_pending() {
    let (mut game, owner) = dispatch_setup("A023", 2);
    // ClayOven (clay 3 + stone 1) is payable, so the Quarry must offer it.
    game.public.players[owner].supply.clay = 3;
    game.public.players[owner].supply.stone = 1;
    let transition = dispatch_aw(&mut game, owner, Space::WesternQuarry, 0).unwrap();
    assert!(matches!(transition, Transition::Paused(_)));
    assert!(game.pending.is_some());
    let opts = improvement_options(&game);
    assert!(
        !opts.is_empty(),
        "A023 must offer at least one payable improvement on Quarry"
    );
    // Every offered candidate must actually cost at least 1 stone (spec):
    // the candidate list is narrowed to stone-costing, payable improvements.
    assert!(
        opts.iter().all(|o| o.costs_stone),
        "A023 candidates must all cost stone: {opts:?}"
    );
}

#[test]
fn a023_resolve_improvement_action_plays_stone_improvement() {
    let (mut game, owner) = dispatch_setup("A023", 2);
    // StoneOven (clay 1 + stone 3) must be payable so it can be offered.
    game.public.players[owner].supply.stone = 3;
    game.public.players[owner].supply.clay = 1;

    let transition = dispatch_aw(&mut game, owner, Space::WesternQuarry, 0).unwrap();
    assert!(matches!(transition, Transition::Paused(_)));
    let opts = improvement_options(&game);
    let position = opts
        .iter()
        .position(|o| o.improvement_id == "StoneOven")
        .expect("A023 must offer StoneOven (a stone-costing improvement)");
    let stone_opt = &opts[position];
    let stone_cost = *stone_opt
        .cost
        .get(&Good::Stone)
        .expect("StoneOven must have a stone cost");
    assert!(
        stone_opt.costs_stone,
        "StoneOven must be flagged stone-costing"
    );
    let market_len_before = game.public.improvement_market.len();
    let player_stone_before = game.public.players[owner].supply.stone;

    // The resolution addresses the offered candidate list (position), and the
    // engine maps it back to the market entry it was built from.
    let transition = stamp_and_resolve(
        &mut game,
        PendingResolution::ImprovementAction {
            improvement_index: position,
        },
    )
    .unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());

    // Improvement removed from market, added to player
    assert_eq!(
        game.public.improvement_market.len(),
        market_len_before - 1,
        "chosen improvement must leave the market"
    );
    assert!(
        !game
            .public
            .improvement_market
            .contains(&Improvement::StoneOven),
        "the chosen copy must leave the market"
    );
    assert!(
        game.public.players[owner]
            .improvements
            .contains(&Improvement::StoneOven),
        "chosen improvement must enter the player's play area"
    );
    // Stone paid (StoneOven costs 3 stone)
    assert_eq!(
        game.public.players[owner].supply.stone,
        player_stone_before - stone_cost,
        "A023 improvement must spend stone"
    );
}

#[test]
fn a023_no_payable_stone_improvement_does_not_trigger() {
    // The candidate list is narrowed to improvements the player can actually
    // pay for AND that cost at least 1 stone. With no resources nothing is
    // payable, so no decision is forced (and the headless default stays legal).
    let (mut game, owner) = dispatch_setup("A023", 2);
    // Empty supply: nothing is affordable.
    game.public.players[owner].supply.stone = 0;
    game.public.players[owner].supply.clay = 0;
    game.public.players[owner].supply.wood = 0;
    game.public.players[owner].supply.reed = 0;

    let transition = dispatch_aw(&mut game, owner, Space::WesternQuarry, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
}

#[test]
fn a023_non_stone_payable_only_does_not_trigger() {
    // Stone without clay/wood/reed: the affordable set is empty, so even
    // though stone-costing improvements exist in the market, none is payable.
    let (mut game, owner) = dispatch_setup("A023", 2);
    game.public.players[owner].supply.stone = 5;
    game.public.players[owner].supply.clay = 0;
    game.public.players[owner].supply.wood = 0;
    game.public.players[owner].supply.reed = 0;

    let transition = dispatch_aw(&mut game, owner, Space::WesternQuarry, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
}

#[test]
fn a023_non_quarry_space_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A023", 2);
    for space in [Space::Forest, Space::Farmland, Space::ClayPit] {
        let transition = dispatch_aw(&mut game, owner, space, 0).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "A023 must not trigger on {space:?}"
        );
        assert!(
            game.pending.is_none(),
            "A023 must not create pending on {space:?}"
        );
    }
}

// ---- A024 脱穀板 (Threshing Board) ----
// Cost 1 wood. Prerequisite: 2+ occupations. After Farmland or Cultivation:
// additional Bake Bread action. (Typed BakeBread pending.)
//
// The `has_baking` / `grain > 0` guards are not spec conditions; they are kept
// as pending-creation guards so a meaningless decision is not forced when the
// player cannot bake. Rationale recorded in PR #392 body.

#[test]
fn a024_farmland_triggers_bake_bread_pending() {
    let (mut game, owner) = dispatch_setup("A024", 2);
    game.public.players[owner]
        .improvements
        .push(Improvement::Fireplace2);
    game.public.players[owner].supply.grain = 1;

    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert!(matches!(transition, Transition::Paused(_)));
    assert!(game.pending.is_some());
}

#[test]
fn a024_resolve_bake_bread_actually_bakes() {
    let (mut game, owner) = dispatch_setup("A024", 2);
    game.public.players[owner]
        .improvements
        .push(Improvement::Fireplace2);
    game.public.players[owner].supply.grain = 1;

    dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();

    let before_grain = game.public.players[owner].supply.grain;
    let before_food = game.public.players[owner].supply.food;

    let transition =
        stamp_and_resolve(&mut game, PendingResolution::BakeBread { confirmed: true }).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
    // Fireplace2: 1 grain -> 2 food
    assert_eq!(
        game.public.players[owner].supply.grain,
        before_grain - 1,
        "baking consumes grain"
    );
    assert_eq!(
        game.public.players[owner].supply.food,
        before_food + 2,
        "baking produces food"
    );
}

#[test]
fn a024_resolve_bake_bread_uses_canonical_oven_rates() {
    // The granted Bake Bread action must use the engine's own baking rates.
    // StoneOven: up to 2 grain at 4 food each (engine `bake_allocated` table).
    let (mut game, owner) = dispatch_setup("A024", 2);
    game.public.players[owner]
        .improvements
        .push(Improvement::StoneOven);
    game.public.players[owner].supply.grain = 2;

    dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    let before_food = game.public.players[owner].supply.food;

    let transition =
        stamp_and_resolve(&mut game, PendingResolution::BakeBread { confirmed: true }).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert_eq!(
        game.public.players[owner].supply.grain, 0,
        "StoneOven must bake both grain"
    );
    assert_eq!(
        game.public.players[owner].supply.food,
        before_food + 8,
        "StoneOven: 2 grain x 4 food = 8 food"
    );
}

#[test]
fn a024_no_grain_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A024", 2);
    game.public.players[owner]
        .improvements
        .push(Improvement::Fireplace2);
    // grain = 0

    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
}

#[test]
fn a024_no_baking_improvement_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A024", 2);
    game.public.players[owner].supply.grain = 1;
    // no baking improvement

    let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
    assert_eq!(transition, Transition::Completed);
    assert!(game.pending.is_none());
}

#[test]
fn a024_non_farmland_space_does_not_trigger() {
    let (mut game, owner) = dispatch_setup("A024", 2);
    game.public.players[owner]
        .improvements
        .push(Improvement::Fireplace2);
    game.public.players[owner].supply.grain = 1;

    for space in [Space::Forest, Space::ClayPit, Space::DayLaborer] {
        let transition = dispatch_aw(&mut game, owner, space, 0).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "A024 must not trigger on {space:?}"
        );
        assert!(
            game.pending.is_none(),
            "A024 must not create pending on {space:?}"
        );
    }
}

// ========================================================================
// Step 3: Headless / save-replay integrity tests
// ========================================================================

#[test]
fn a015_pending_blocks_apply_and_spaces_empty() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
    assert!(game.pending.is_some());

    // While a decision is pending, no spaces are legal
    assert!(
        game.available_spaces().is_empty(),
        "no spaces while pending"
    );
    assert!(
        game.legal_spaces().is_empty(),
        "legal_spaces empty while pending"
    );

    // apply / apply_with_harvest_policy must reject a new action while pending
    let err = game
        .apply(Action {
            space: Space::Forest,
            detail: Detail::None,
        })
        .unwrap_err();
    assert!(
        err.contains("pending decision"),
        "apply must fail while a decision is pending: {err}"
    );

    // Resolving clears the pending and applies the effect (headless continuation)
    let sites = build_stable_sites(&game);
    stamp_and_resolve(
        &mut game,
        PendingResolution::BuildStable {
            site_index: Some(0),
        },
    )
    .unwrap();
    assert!(game.pending.is_none());
    assert!(
        game.public.players[owner].farm[sites[0]].stable,
        "stable built after resume"
    );
}

#[test]
fn pending_state_hash_mismatch_is_rejected() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
    assert!(game.pending.is_some());

    // Direct dispatch leaves state_hash_before = ""; resolve must reject
    let err = game
        .resolve_pending(PendingResolution::BuildStable {
            site_index: Some(0),
        })
        .unwrap_err();
    assert!(
        err.contains("state hash mismatch"),
        "direct-dispatch pending must fail hash check: {err}"
    );

    // After stamping the hash (as the engine does at pause commit), resolution succeeds
    stamp_and_resolve(
        &mut game,
        PendingResolution::BuildStable {
            site_index: Some(0),
        },
    )
    .unwrap();
    assert!(game.pending.is_none());
}

// ========================================================================
// Step 4: Headless one-shot apply path (AI / batch simulation contract)
// ========================================================================
//
// `apply` / `apply_with_harvest_policy` cannot represent a paused state, so a
// decision with a deterministic, legal default (the A-deck cards, whose
// handlers only ever offer legal options) is committed AND resolved with that
// default inside the same transaction. `play_to_end_with_budget` drives games
// through exactly this path, so these tests use the real one-shot `apply` and
// assert the effect completed, the game continued, and the replay chain stays
// exact.

/// Make `card` the round's revealed card so its space is available this round
/// (same board-editing helper the engine tests use).
fn reveal(g: &mut Game, card: RoundCard) {
    if g.public.revealed[0] == card {
        return;
    }
    let at = g.hidden_round_deck.iter().position(|&x| x == card).unwrap();
    let old = g.public.revealed[0];
    g.public.revealed[0] = card;
    g.hidden_round_deck[at] = old;
}

fn replay_from_log(game: &Game) -> Game {
    Game::replay_with_cards(
        game.config.clone(),
        (*game.card_set).clone(),
        &game.log.events,
    )
    .expect("replay must reproduce the logged game")
}

fn resolution_events(game: &Game) -> usize {
    game.log
        .events
        .iter()
        .filter(|e| e.kind == "pending_resolution")
        .count()
}

/// A game (2 players) whose active player was really DEALT `card_id`.
/// The deal is engine-generated for the seed, so no card-state edit happens
/// before the first logged action and the log stays replayable.
fn dealt_game_with_card(card_id: &str) -> Option<(Game, usize)> {
    for seed in 0..128u64 {
        let game = game_with_valid_cards(2, seed);
        let player = game.public.active_player;
        if game.cards.hands[player].iter().any(|id| id == card_id) {
            return Some((game, player));
        }
    }
    None
}

fn forest_action() -> Action {
    Action {
        space: Space::Forest,
        detail: Detail::None,
    }
}

/// Really PLAY `card_id` for the start player through the engine
/// (MeetingPlace + PlayMinor), leaving the player active again in a fresh
/// round with the card in their played area. Every step is a logged action,
/// so the resulting game saves, loads and replays exactly.
fn logged_game_with_card_played(card_id: &str) -> (Game, usize) {
    let (mut game, owner) =
        dealt_game_with_card(card_id).expect("card is dealt to a start player within 128 seeds");
    game.apply(forest_action()).unwrap(); // start player worker 1: +3 wood
    game.apply(Action {
        space: Space::GrainSeeds,
        detail: Detail::None,
    })
    .unwrap(); // opponent worker 1
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: card_id.to_string(),
        },
    })
    .unwrap(); // start player worker 2: play the card (logged)
    game.apply(Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    })
    .unwrap(); // opponent worker 2 -> round advances
    assert_eq!(game.public.active_player, owner);
    assert!(game.public.players[owner]
        .played_cards
        .contains(&card_id.to_string()));
    (game, owner)
}

#[test]
fn a015_headless_apply_builds_stable_and_continues() {
    let (mut game, owner) = dispatch_setup("A015", 2);
    game.public.players[owner].supply.wood = 7;
    game.public.players[owner].farm[0].tile = Tile::Empty;
    game.public.players[owner].farm[0].stable = false;

    // One-shot apply on Forest: previously this returned
    // "action paused on a pending decision" and broke headless play.
    game.apply(Action {
        space: Space::Forest,
        detail: Detail::None,
    })
    .unwrap();

    assert!(
        game.pending.is_none(),
        "headless path must resolve the pause"
    );
    // Forest grants 3 wood; the A015 stable costs 1 of them.
    assert_eq!(game.public.players[owner].supply.wood, 7 + 3 - 1);
    assert!(
        game.public.players[owner].farm[0].stable,
        "default resolution must build the first offered stable"
    );
    assert_eq!(game.public.players[owner].occupied_stables(), 1);
    // The action itself completed: space used, worker placed, game continues.
    assert!(game.public.used.contains(&Space::Forest));
    assert_eq!(game.public.workers_placed[owner], 1);
    assert!(
        !game.legal_spaces().is_empty(),
        "the game must continue after the headless resolution"
    );
    game.validate().unwrap();
    assert_eq!(resolution_events(&game), 1, "default resolution is logged");
}

#[test]
fn a018_headless_apply_plows_two_fields() {
    let (mut game, owner) = dispatch_setup("A018", 2);

    game.apply(Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 2 },
    })
    .unwrap();

    assert!(game.pending.is_none());
    // Farmland action plowed 1 field; A018 plowed 2 more.
    assert_eq!(game.public.players[owner].fields(), 3);
    assert_eq!(
        game.public.card_uses_game.get("A018"),
        Some(&1),
        "once-per-game counter must be marked"
    );
    // The action itself completed: the game continues.
    assert!(game.public.used.contains(&Space::Farmland));
    assert_eq!(game.public.workers_placed[owner], 1);
    game.validate().unwrap();
}

#[test]
fn a023_headless_apply_plays_stone_improvement() {
    let (mut game, owner) = dispatch_setup("A023", 2);
    game.public.players[owner].supply.stone = 3;
    game.public.players[owner].supply.clay = 1;
    reveal(&mut game, RoundCard::WesternQuarry);
    // The round-1 accumulation table was computed for the original card, so
    // register the swapped-in Quarry's stone pile as well.
    *game
        .public
        .accumulations
        .entry(Space::WesternQuarry)
        .or_default() += 1;

    game.apply(Action {
        space: Space::WesternQuarry,
        detail: Detail::None,
    })
    .unwrap();

    assert!(game.pending.is_none());
    // WesternQuarry granted 1 stone; StoneOven costs clay 1 + stone 3.
    assert_eq!(
        game.public.players[owner].supply.stone, 1,
        "A023 must actually pay the stone cost"
    );
    assert_eq!(game.public.players[owner].supply.clay, 0);
    assert!(
        game.public.players[owner]
            .improvements
            .contains(&Improvement::StoneOven),
        "chosen improvement must enter the player's play area"
    );
    assert!(
        !game
            .public
            .improvement_market
            .contains(&Improvement::StoneOven),
        "chosen improvement must leave the market"
    );
    game.validate().unwrap();
    assert_eq!(resolution_events(&game), 1);
}

#[test]
fn a024_headless_apply_bakes_bread() {
    let (mut game, owner) = dispatch_setup("A024", 2);
    // Take the Fireplace out of the market (a physical improvement has exactly
    // one location) before handing it to the player.
    game.public
        .improvement_market
        .retain(|imp| *imp != Improvement::Fireplace2);
    game.public.players[owner]
        .improvements
        .push(Improvement::Fireplace2);
    game.public.players[owner].supply.grain = 1;
    let before_food = game.public.players[owner].supply.food;

    game.apply(Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 2 },
    })
    .unwrap();

    assert!(game.pending.is_none());
    assert_eq!(game.public.players[owner].fields(), 1);
    assert_eq!(
        game.public.players[owner].supply.grain, 0,
        "A024 bake must consume the grain"
    );
    assert_eq!(
        game.public.players[owner].supply.food,
        before_food + 2,
        "Fireplace2: 1 grain -> 2 food"
    );
    game.validate().unwrap();
}

/// The owner places a worker on Forest; the two seats then alternate (the
/// owner's second placement on `second_owner_space`, the opponent on
/// GrainSeeds and DayLaborer) until the round ends and the owner is active
/// again. Only base spaces are used, so no round-card availability matters.
fn owner_forest_round(game: &mut Game, owner: usize, second_owner_space: Space) {
    assert_eq!(game.public.active_player, owner);
    game.apply(forest_action()).unwrap();
    game.apply(Action {
        space: Space::GrainSeeds,
        detail: Detail::None,
    })
    .unwrap();
    game.apply(Action {
        space: second_owner_space,
        detail: Detail::None,
    })
    .unwrap();
    game.apply(Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    })
    .unwrap();
    assert_eq!(game.public.active_player, owner);
}

#[test]
fn a015_logged_play_survives_save_load_and_replay() {
    // Fully logged game: the card is really played through the engine (so the
    // log is replayable), then Forest fires it on the headless one-shot path.
    let (mut game, owner) = logged_game_with_card_played("A015");

    // Round 2 Forest: 5 wood in supply — below the 7-wood condition, so the
    // effect does not fire and no decision is raised.
    owner_forest_round(&mut game, owner, Space::ClayPit);
    assert_eq!(game.public.players[owner].supply.wood, 5);
    assert_eq!(game.public.players[owner].occupied_stables(), 0);
    assert_eq!(resolution_events(&game), 0);

    // Round 3 Forest: 8 wood — the effect fires and the headless default
    // resolves it inside the same transaction.
    owner_forest_round(&mut game, owner, Space::ClayPit);
    assert!(game.pending.is_none());
    assert_eq!(
        game.public.players[owner].supply.wood,
        8 - 1,
        "A015 must charge exactly 1 wood for the stable"
    );
    assert_eq!(game.public.players[owner].occupied_stables(), 1);
    assert_eq!(resolution_events(&game), 1);

    // Save -> load (which replays the whole chain internally) -> replay all
    // reproduce the state byte-for-byte.
    let original_hash = game.state_hash();
    let loaded = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(loaded.state_hash(), original_hash);
    assert_eq!(loaded.public, game.public);
    let replayed = replay_from_log(&game);
    assert_eq!(replayed.state_hash(), original_hash);
    assert_eq!(replayed.public, game.public);
}

#[test]
fn a018_logged_once_per_game_survives_save_load_and_replay() {
    let (mut game, owner) = logged_game_with_card_played("A018");

    // The acting player is the start player of the new round: first person of
    // the round, so the once-per-game effect fires.
    game.apply(Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 2 },
    })
    .unwrap();

    assert!(game.pending.is_none());
    assert_eq!(game.public.players[owner].fields(), 3);
    assert_eq!(game.public.card_uses_game.get("A018"), Some(&1));

    let original_hash = game.state_hash();
    let loaded = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(loaded.state_hash(), original_hash);
    assert_eq!(loaded.public, game.public);
    assert_eq!(
        loaded.public.card_uses_game.get("A018"),
        Some(&1),
        "once-per-game counter must survive save/load"
    );
    let replayed = replay_from_log(&game);
    assert_eq!(replayed.state_hash(), original_hash);
    assert_eq!(
        replayed.public.card_uses_game.get("A018"),
        Some(&1),
        "once-per-game counter must survive replay"
    );
}

#[test]
fn two_a_deck_cards_resolve_both_pauses_headlessly() {
    // One Farmland action triggers A018 AND A024 for the same player, so the
    // headless path must resolve a second pause raised during the resume.
    let (mut game, owner) = dispatch_setup("A018", 2);
    play_second_card(&mut game, owner, "A024");
    game.public
        .improvement_market
        .retain(|imp| *imp != Improvement::Fireplace2);
    game.public.players[owner]
        .improvements
        .push(Improvement::Fireplace2);
    game.public.players[owner].supply.grain = 1;
    let before_food = game.public.players[owner].supply.food;

    game.apply(Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 2 },
    })
    .unwrap();

    assert!(
        game.pending.is_none(),
        "headless path must resolve every A-deck pause in one action"
    );
    assert_eq!(
        game.public.players[owner].fields(),
        3,
        "1 action plow + 2 A018 plows"
    );
    assert_eq!(game.public.players[owner].supply.grain, 0);
    assert_eq!(game.public.players[owner].supply.food, before_food + 2);
    assert_eq!(game.public.card_uses_game.get("A018"), Some(&1));
    assert_eq!(game.public.workers_placed[owner], 1);
    assert_eq!(
        resolution_events(&game),
        2,
        "both default resolutions must be logged for replay"
    );
    // The action itself completed exactly once: space used, worker placed.
    assert!(game.public.used.contains(&Space::Farmland));
    assert_eq!(game.public.workers_placed[owner], 1);
    game.validate().unwrap();
}

#[test]
fn tampered_once_per_game_counter_is_rejected_on_load() {
    // Fully logged game (the card is really played), so the save is replayable
    // and the only thing standing between the tampered counter and acceptance
    // is the integrity hash.
    let (mut game, _owner) = logged_game_with_card_played("A018");
    game.apply(Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 2 },
    })
    .unwrap();
    assert_eq!(game.public.card_uses_game.get("A018"), Some(&1));

    let json = game.save_json().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Reset the counter in the save: A018 could then fire a second time, so
    // the integrity hash must reject the tampered ledger.
    value["public"]["card_uses_game"]["A018"] = serde_json::json!(0);
    let tampered = serde_json::to_string(&value).unwrap();
    let err =
        Game::load_json(&tampered).expect_err("tampered once-per-game counter must be rejected");
    assert!(
        err.contains("integrity") || err.contains("bookkeeping"),
        "rejection must come from the integrity check: {err}"
    );
}

// ========================================================================
// Mutation verification tests - prove the implementation is actually inspected
// ========================================================================

#[cfg(test)]
mod mutation_verification {
    use super::*;

    // Mutation: change A015's wood requirement from 7 to 6.
    // With the real requirement (7), 6 wood must NOT trigger and 7 wood MUST.
    // If the requirement were weakened to 6, the first assertion below would fail.
    #[test]
    fn mutation_a015_wood_requirement_detected() {
        let (mut game, owner) = dispatch_setup("A015", 2);
        game.public.players[owner].farm[0].tile = Tile::Empty;
        game.public.players[owner].farm[0].stable = false;

        game.public.players[owner].supply.wood = 6;
        let transition = dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
        assert_eq!(transition, Transition::Completed);
        assert!(game.pending.is_none(), "6 wood must not trigger A015");

        game.public.players[owner].supply.wood = 7;
        let transition = dispatch_aw(&mut game, owner, Space::Forest, 0).unwrap();
        assert!(matches!(transition, Transition::Paused(_)));
        assert!(game.pending.is_some(), "7 wood must trigger A015");
    }

    // Mutation: rebind A021 to the wrong space (FamilyGrowthWithRoom instead
    // of Build Rooms). With the spec-correct binding, FarmExpansion fires
    // (family + food) and FamilyGrowthWithRoom does not. Rebinding the card to
    // FamilyGrowthWithRoom flips both assertions.
    #[test]
    fn mutation_a021_space_binding_detected() {
        let fire = |space: Space| -> (bool, u32) {
            let (mut game, owner) = dispatch_setup("A021", 2);
            game.public.players[owner].house = House::Wood;
            game.public.players[owner].farm[0].tile = Tile::Room;
            game.public.players[owner].farm[1].tile = Tile::Room;
            game.public.players[owner].family = 1;
            let before_food = game.public.players[owner].supply.food;
            let before_family = game.public.players[owner].family;
            dispatch_aw(&mut game, owner, space, 0).unwrap();
            (
                game.public.players[owner].family > before_family,
                game.public.players[owner].supply.food - before_food,
            )
        };
        // Build Rooms is FarmExpansion: fires (rooms 2 > people 1).
        assert_eq!(
            fire(Space::FarmExpansion),
            (true, 1),
            "A021 must fire after Build Rooms"
        );
        // FamilyGrowthWithRoom must NOT fire for A021.
        assert_eq!(
            fire(Space::FamilyGrowthWithRoom),
            (false, 0),
            "A021 must not fire after Family Growth"
        );
    }

    // Mutation: drop A018's once-per-game enforcement. With the enforcement
    // the second trigger in one game must be a no-op; without it the second
    // trigger pauses again.
    #[test]
    fn mutation_a018_once_per_game_detected() {
        let (mut game, owner) = dispatch_setup("A018", 2);

        // First trigger: pauses and marks the once-per-game counter.
        let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
        assert!(matches!(transition, Transition::Paused(_)));
        assert_eq!(game.public.card_uses_game.get("A018"), Some(&1));

        // Resolve the first use.
        stamp_and_resolve(
            &mut game,
            PendingResolution::PlowFields {
                site_indices: vec![0, 1],
            },
        )
        .unwrap();

        // Second trigger in the SAME game: must be a no-op.
        let transition = dispatch_aw(&mut game, owner, Space::Farmland, 0).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "A018 must not fire twice in one game"
        );
        assert!(game.pending.is_none());
    }

    // Mutation: drop A018's first-person-of-round enforcement. With the
    // enforcement a non-starter trigger is a no-op; without it the same
    // dispatch pauses.
    #[test]
    fn mutation_a018_first_person_detected() {
        let (mut game, owner) = dispatch_setup("A018", 2);

        // owner (0) is NOT the start player (1): no trigger.
        let transition = dispatch_aw(&mut game, owner, Space::Farmland, 1).unwrap();
        assert_eq!(
            transition,
            Transition::Completed,
            "only the first person of the round may trigger A018"
        );
        assert!(game.pending.is_none());
        assert!(
            !game.public.card_uses_game.contains_key("A018"),
            "a non-starter trigger must not consume the once-per-game use"
        );
    }
}

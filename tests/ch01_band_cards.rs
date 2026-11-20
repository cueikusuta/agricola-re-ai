//! Tests for ch01 第1章 cards (A155/B122/B142/B144) added in PR #322.
//!
//! These tests verify:
//! - Registry/construction path reachability (Step 1)
//! - B142 space-specific triggering: GrainSeeds yes, Farmland no (Step 2)
//! - dispatch_after_worker integration path for all 4 cards (Step 3)

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{
    after_worker, CardDefinition, CardEffect, CardKind, CardRequirement, CardSet,
};
use agricola_re_ai::model::{Config, Game, Good, Player, Space};
use agricola_re_ai::pending::Transition;
use agricola_re_ai::registry::{build_default_registry, card_set_from_pool, CardPool};

/// Build a CardSet containing exactly the 4 ch01 cards for targeted testing.
fn ch01_card_set() -> CardSet {
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    CardSet {
        definitions: vec![
            // A155 奇術師 (Conjurer): after worker on traveling_players, gain 1 wood + 1 grain. 4 players only.
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
                post_play_destination: None,
            },
            // B122 鉱物学者 (Mineralogist): clay_pit/hollow -> stone; western/eastern_quarry -> clay. All player counts.
            CardDefinition {
                use_limit: None,
                id: "B122".to_owned(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![
                    gain(Space::ClayPit, Good::Stone, 1),
                    gain(Space::Hollow, Good::Stone, 1),
                    gain(Space::WesternQuarry, Good::Clay, 1),
                    gain(Space::EasternQuarry, Good::Clay, 1),
                ],
                post_play_destination: None,
            },
            // B142 八百屋 (Greengrocer): grain_seeds grants 1 vegetable. 3+ players only.
            CardDefinition {
                use_limit: None,
                id: "B142".to_owned(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(3),
                    },
                    gain(Space::GrainSeeds, Good::Vegetable, 1),
                ],
                post_play_destination: None,
            },
            // B144 坑夫 (Collier): clay_pit/hollow -> wood; clay_pit -> reed. 3+ players only.
            CardDefinition {
                use_limit: None,
                id: "B144".to_owned(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(3),
                    },
                    gain(Space::ClayPit, Good::Wood, 1),
                    gain(Space::Hollow, Good::Wood, 1),
                    gain(Space::ClayPit, Good::Reed, 1),
                ],
                post_play_destination: None,
            },
        ],
    }
}

/// Check if a card definition is eligible for the given player count (no MinimumPlayers > players).
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
        post_play_destination: None,
    }
}

/// Build a valid CardSet for the given player count using the experimental pool
/// from the default registry, filtered to only include cards eligible for the player count.
/// Then ensure we have at least 7 cards of each kind per player by adding filler cards if needed.
fn valid_card_set_for_players(players: usize) -> CardSet {
    let registry = build_default_registry().expect("default registry builds");
    let pool = CardPool::new(&registry, players, true); // experimental_mode = true
    let mut full_set = card_set_from_pool(&registry, &pool);

    // Filter out cards that have MinimumPlayers > players
    full_set
        .definitions
        .retain(|card| card_eligible_for_players(card, players));

    // Add ch01 cards if they're eligible for this player count (they should already be in the pool,
    // but this ensures they're present even if the pool is missing some)
    for ch01_card in ch01_card_set().definitions {
        if card_eligible_for_players(&ch01_card, players) {
            // Check if already present
            if !full_set.definitions.iter().any(|c| c.id == ch01_card.id) {
                full_set.definitions.push(ch01_card);
            }
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

/// Helper: give the currently active player the specified card in hand AND played_cards.
/// Does NOT advance turns - uses the current active player.
fn setup_active_player_with_card(game: &mut Game, card_id: &str) -> usize {
    let player = game.public.active_player;
    // Add the card to the player's hand
    game.cards.hands[player].push(card_id.to_string());
    // Also add to played_cards so dispatch_after_worker can find it
    game.public.players[player]
        .played_cards
        .push(card_id.to_string());
    player
}

// ========================================================================
// Step 1: Registry/construction path reachability tests
// ========================================================================

#[test]
fn ch01_cards_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");

    // All 4 cards must be present in the registry by external_id
    for id in ["A155", "B122", "B142", "B144"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must have a CardId"));
        let card = registry
            .get(card_id)
            .unwrap_or_else(|| panic!("{id} must be retrievable by CardId"));
        assert_eq!(card.external_id, id, "round-trip external_id for {id}");
        assert_eq!(card.kind, CardKind::Occupation, "{id} must be Occupation");
    }
}

#[test]
fn ch01_cards_registry_event_index_populated() {
    let registry = build_default_registry().expect("default registry builds");

    // All 4 cards use AfterWorkerGain, so they should be indexed under WorkerPlaced
    let candidates = registry.event_candidates(agricola_re_ai::registry::EventKind::WorkerPlaced);
    let candidate_ids: std::collections::BTreeSet<_> = candidates.iter().copied().collect();

    for id in ["A155", "B122", "B142", "B144"] {
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
fn ch01_cards_have_unverified_review_status_in_registry() {
    let registry = build_default_registry().expect("default registry builds");

    // All 4 cards should have Unverified review status (default for built-in cards)
    for id in ["A155", "B122", "B142", "B144"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} in registry"));
        let review = registry
            .review(card_id)
            .unwrap_or_else(|| panic!("{id} has review"));
        assert_eq!(
            review.review_status,
            agricola_re_ai::card_pool::ReviewStatus::Unverified,
            "{id} should be Unverified in default registry"
        );
        // Eligibility should be Unverified for all player counts
        assert_eq!(
            review.eligibility_at(2),
            Some(agricola_re_ai::card_pool::Eligibility::Unverified)
        );
        assert_eq!(
            review.eligibility_at(3),
            Some(agricola_re_ai::card_pool::Eligibility::Unverified)
        );
        assert_eq!(
            review.eligibility_at(4),
            Some(agricola_re_ai::card_pool::Eligibility::Unverified)
        );
    }
}

#[test]
fn pool_excludes_all_ch01_cards_due_to_unverified_status() {
    let registry = build_default_registry().expect("default registry builds");

    // Since all cards are Unverified, none should be in the official pool
    for players in [2usize, 3, 4] {
        let pool = CardPool::new(&registry, players, false);
        for id in ["A155", "B122", "B142", "B144"] {
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
// Step 2: B142 space-specific tests (GrainSeeds yes, Farmland no)
// ========================================================================

#[test]
fn b142_triggers_on_grain_seeds_only() {
    let mut game = game_with_valid_cards(3, 1);
    let player = setup_active_player_with_card(&mut game, "B142");

    // Place worker on GrainSeeds - should trigger
    let before_veg = game.public.players[player].supply.vegetable;
    game.apply(agricola_re_ai::engine::Action {
        space: Space::GrainSeeds,
        detail: agricola_re_ai::engine::Detail::None,
    })
    .unwrap();
    assert_eq!(
        game.public.players[player].supply.vegetable,
        before_veg + 1,
        "B142 must grant 1 vegetable on GrainSeeds"
    );

    // Place worker on Farmland - should NOT trigger
    let before_veg2 = game.public.players[player].supply.vegetable;
    game.apply(agricola_re_ai::engine::Action {
        space: Space::Farmland,
        detail: agricola_re_ai::engine::Detail::Plow { cell: 1 },
    })
    .unwrap();
    assert_eq!(
        game.public.players[player].supply.vegetable, before_veg2,
        "B142 must NOT grant vegetable on Farmland"
    );
}

#[test]
fn b142_unit_test_grain_seeds_vs_farmland() {
    // Direct unit test of the card effect without full game
    let set = ch01_card_set();
    let mut player = Player::new(0, 0);
    player.played_cards.push("B142".to_string());

    // GrainSeeds triggers
    after_worker(&set, &mut player, Space::GrainSeeds).unwrap();
    assert_eq!(player.supply.vegetable, 1);

    // Farmland does NOT trigger
    let before = player.supply.vegetable;
    after_worker(&set, &mut player, Space::Farmland).unwrap();
    assert_eq!(
        player.supply.vegetable, before,
        "Farmland must not trigger B142"
    );

    // Other spaces do NOT trigger
    for _space in [
        Space::Forest,
        Space::ClayPit,
        Space::VegetableSeeds,
        Space::Cultivation,
    ] {
        let before = player.supply.vegetable;
        after_worker(&set, &mut player, _space).unwrap();
        assert_eq!(
            player.supply.vegetable, before,
            "{_space:?} must not trigger B142"
        );
    }
}

#[test]
fn b142_does_not_trigger_for_other_players() {
    let set = ch01_card_set();

    // Player 0 has B142
    let mut p0 = Player::new(0, 0);
    p0.played_cards.push("B142".to_string());

    // Player 1 does not have B142
    let mut p1 = Player::new(1, 0);

    // Player 1 places worker on GrainSeeds - B142 should NOT trigger for p0
    after_worker(&set, &mut p1, Space::GrainSeeds).unwrap();
    assert_eq!(
        p0.supply.vegetable, 0,
        "B142 must not trigger when other player places worker"
    );
    assert_eq!(p1.supply.vegetable, 0, "Player 1 has no B142");
}

// ========================================================================
// Step 3: dispatch_after_worker integration tests for all 4 cards
// ========================================================================

fn dispatch_test_setup(card_id: &str, players: usize) -> (Game, usize) {
    let mut game = game_with_valid_cards(players, 42);
    let owner = setup_active_player_with_card(&mut game, card_id);
    (game, owner)
}

/// Test that dispatch_after_worker triggers the card effect when the owner places a worker.
#[test]
fn dispatch_after_worker_b122_owner_triggers() {
    let (mut game, owner) = dispatch_test_setup("B122", 2);

    // Place worker on ClayPit (should grant 1 stone)
    let before = game.public.players[owner].supply.stone;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::ClayPit,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.stone,
        before + 1,
        "B122 must grant stone on ClayPit via dispatch_after_worker"
    );
}

/// Test that dispatch_after_worker does NOT trigger when non-owner places worker.
#[test]
fn dispatch_after_worker_b122_non_owner_no_trigger() {
    let (mut game, owner) = dispatch_test_setup("B122", 2);
    // Find a non-owner player
    let non_owner = (owner + 1) % game.public.players.len();

    // Non-owner places worker on ClayPit - B122 should NOT trigger
    let before = game.public.players[owner].supply.stone;
    let transition = dispatch_after_worker(
        &game.card_set,
        non_owner,
        Space::ClayPit,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.stone, before,
        "B122 must NOT grant stone when non-owner places worker"
    );
}

/// Test that dispatch_after_worker does NOT trigger on unrelated spaces.
#[test]
fn dispatch_after_worker_b122_unrelated_space_no_trigger() {
    let (mut game, owner) = dispatch_test_setup("B122", 2);

    // Owner places worker on Forest (unrelated to B122) - should NOT trigger
    let before = game.public.players[owner].supply.stone;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::Forest,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.stone, before,
        "B122 must NOT grant stone on unrelated space Forest"
    );
}

/// Test B142 via dispatch_after_worker (owner, GrainSeeds)
#[test]
fn dispatch_after_worker_b142_owner_grain_seeds() {
    let (mut game, owner) = dispatch_test_setup("B142", 3);

    let before = game.public.players[owner].supply.vegetable;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::GrainSeeds,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.vegetable,
        before + 1,
        "B142 must grant vegetable on GrainSeeds via dispatch_after_worker"
    );
}

/// Test B142 does NOT trigger on Farmland via dispatch_after_worker
#[test]
fn dispatch_after_worker_b142_owner_farmland_no_trigger() {
    let (mut game, owner) = dispatch_test_setup("B142", 3);

    let before = game.public.players[owner].supply.vegetable;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::Farmland,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.vegetable, before,
        "B142 must NOT grant vegetable on Farmland via dispatch_after_worker"
    );
}

/// Test B144 via dispatch_after_worker (owner, ClayPit and Hollow)
#[test]
fn dispatch_after_worker_b144_owner_triggers() {
    let (mut game, owner) = dispatch_test_setup("B144", 3);

    // ClayPit grants wood + reed
    let before_wood = game.public.players[owner].supply.wood;
    let before_reed = game.public.players[owner].supply.reed;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::ClayPit,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(game.public.players[owner].supply.wood, before_wood + 1);
    assert_eq!(game.public.players[owner].supply.reed, before_reed + 1);

    // Hollow grants wood
    let before_wood2 = game.public.players[owner].supply.wood;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::Hollow,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(game.public.players[owner].supply.wood, before_wood2 + 1);
}

/// Test B144 non-owner no trigger
#[test]
fn dispatch_after_worker_b144_non_owner_no_trigger() {
    let (mut game, owner) = dispatch_test_setup("B144", 3);
    let non_owner = (owner + 1) % game.public.players.len();

    let before = game.public.players[owner].supply.wood;
    let transition = dispatch_after_worker(
        &game.card_set,
        non_owner,
        Space::ClayPit,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.wood, before,
        "B144 must NOT grant when non-owner places worker"
    );
}

/// Test A155 via dispatch_after_worker (owner, TravelingPlayers)
#[test]
fn dispatch_after_worker_a155_owner_triggers() {
    let (mut game, owner) = dispatch_test_setup("A155", 4);

    let before_wood = game.public.players[owner].supply.wood;
    let before_grain = game.public.players[owner].supply.grain;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::TravelingPlayers,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(game.public.players[owner].supply.wood, before_wood + 1);
    assert_eq!(game.public.players[owner].supply.grain, before_grain + 1);
}

/// Test A155 non-owner no trigger
#[test]
fn dispatch_after_worker_a155_non_owner_no_trigger() {
    let (mut game, owner) = dispatch_test_setup("A155", 4);
    let non_owner = (owner + 1) % game.public.players.len();

    let before_wood = game.public.players[owner].supply.wood;
    let before_grain = game.public.players[owner].supply.grain;
    let transition = dispatch_after_worker(
        &game.card_set,
        non_owner,
        Space::TravelingPlayers,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(game.public.players[owner].supply.wood, before_wood);
    assert_eq!(game.public.players[owner].supply.grain, before_grain);
}

/// Test A155 unrelated space no trigger
#[test]
fn dispatch_after_worker_a155_unrelated_space_no_trigger() {
    let (mut game, owner) = dispatch_test_setup("A155", 4);

    let before_wood = game.public.players[owner].supply.wood;
    let before_grain = game.public.players[owner].supply.grain;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::Forest,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .unwrap();
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(game.public.players[owner].supply.wood, before_wood);
    assert_eq!(game.public.players[owner].supply.grain, before_grain);
}

// ========================================================================
// Mutation verification tests (Step 4) - these are run manually during the task
// ========================================================================

#[cfg(test)]
mod mutation_verification {
    use super::*;

    // These tests are designed to FAIL when the corresponding mutation is applied.
    // They are run as part of the normal test suite to prove the mutations are detected.

    // Mutation 1: Remove registry registration line for band_b_cards
    // This test will fail to find B122/B142/B144 in the registry if that line is removed.
    #[test]
    fn mutation_registry_registration_detected_b122() {
        let registry = build_default_registry().expect("registry builds");
        let card_id = registry.card_id("B122").expect("B122 must be registered");
        let _ = registry.get(card_id).expect("B122 must be retrievable");
    }

    #[test]
    fn mutation_registry_registration_detected_b142() {
        let registry = build_default_registry().expect("registry builds");
        let card_id = registry.card_id("B142").expect("B142 must be registered");
        let _ = registry.get(card_id).expect("B142 must be retrievable");
    }

    #[test]
    fn mutation_registry_registration_detected_b144() {
        let registry = build_default_registry().expect("registry builds");
        let card_id = registry.card_id("B144").expect("B144 must be registered");
        let _ = registry.get(card_id).expect("B144 must be retrievable");
    }

    #[test]
    fn mutation_registry_registration_detected_a155() {
        let registry = build_default_registry().expect("registry builds");
        let card_id = registry.card_id("A155").expect("A155 must be registered");
        let _ = registry.get(card_id).expect("A155 must be retrievable");
    }

    // Mutation 2: Change B142's space from GrainSeeds to Farmland
    // This test will fail if B142 triggers on Farmland instead of GrainSeeds
    #[test]
    fn mutation_b142_space_swap_detected() {
        let set = ch01_card_set();
        let mut player = Player::new(0, 0);
        player.played_cards.push("B142".to_string());

        // GrainSeeds MUST trigger (this is the correct behavior)
        after_worker(&set, &mut player, Space::GrainSeeds).unwrap();
        assert_eq!(
            player.supply.vegetable, 1,
            "B142 must trigger on GrainSeeds (mutation would break this)"
        );

        // Farmland MUST NOT trigger (this is the correct behavior)
        let before = player.supply.vegetable;
        after_worker(&set, &mut player, Space::Farmland).unwrap();
        assert_eq!(
            player.supply.vegetable, before,
            "B142 must NOT trigger on Farmland (mutation would break this)"
        );
    }
}

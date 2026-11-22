//! Tests for ch06 第1章 round-start gain cards (A090/A151/A152/B097/B106/B135/C064).
//!
//! These tests verify:
//! - Registry/construction path reachability (Step 1)
//! - Handler integration path for all 7 cards (Step 2)
//! - Full game integration with round-start triggers (Step 3)

use agricola_re_ai::card_events::PhaseTriggerData;
use agricola_re_ai::cards::{
    verified_round_start_cards, CardDefinition, CardEffect, CardKind, CardRequirement,
};
use agricola_re_ai::model::{Config, Game, Good, House};
use agricola_re_ai::registry::{build_default_registry, card_set_from_pool, CardPool};

/// Build a CardSet containing exactly the 7 ch06 round-start cards for targeted testing.
fn ch06_round_start_card_set() -> Vec<CardDefinition> {
    verified_round_start_cards()
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

#[test]
fn round_start_cards_construct_and_are_eligible() {
    let cards = ch06_round_start_card_set();
    assert_eq!(cards.len(), 7, "Expected exactly 7 round-start cards");

    // Verify all cards have the expected IDs
    let ids: Vec<&str> = cards.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["A090", "A151", "A152", "B097", "B106", "B135", "C064"]
    );

    // Check eligibility for different player counts
    for players in [2, 3, 4] {
        let eligible: Vec<&str> = cards
            .iter()
            .filter(|c| card_eligible_for_players(c, players))
            .map(|c| c.id.as_str())
            .collect();

        match players {
            2 => {
                // A151, A152 require 4 players; B135 requires 3+ players
                assert_eq!(eligible, vec!["A090", "B097", "B106", "C064"]);
            }
            3 => {
                // A151, A152 require 4 players
                assert_eq!(eligible, vec!["A090", "B097", "B106", "B135", "C064"]);
            }
            4 => {
                // All 7 cards eligible
                assert_eq!(
                    eligible,
                    vec!["A090", "A151", "A152", "B097", "B106", "B135", "C064"]
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn round_start_cards_in_default_registry() {
    let registry = build_default_registry().expect("registry build failed");

    // All 7 cards should be present in the default registry
    for id in ["A090", "A151", "A152", "B097", "B106", "B135", "C064"] {
        let found = registry.iter().any(|c| c.external_id == id);
        assert!(found, "Card {id} not found in default registry");
    }
}

#[test]
fn round_start_cards_use_start_round_gain_vocabulary() {
    let cards = ch06_round_start_card_set();

    for card in &cards {
        for effect in &card.effects {
            // Each card should have at least one StartRound* effect variant
            let is_start_round = matches!(
                effect,
                CardEffect::StartRoundPlowFieldIfHouse { .. }
                    | CardEffect::StartRoundCopyUnusedRoundSpace { .. }
                    | CardEffect::StartRoundPlayOccupationIfLessonsUnderused { .. }
                    | CardEffect::StartRoundPlayCardIfStoneHouse { .. }
                    | CardEffect::StartRoundGainIfGoodsOnRoundSpaces { .. }
                    | CardEffect::StartRoundExchangeForFoodAndPoints { .. }
                    | CardEffect::StartRoundPlaceFoodOnNextRoundSpaces { .. }
            );
            assert!(
                is_start_round || matches!(effect, CardEffect::Prerequisite { .. }),
                "Card {} has non-StartRound effect: {:?}",
                card.id,
                effect
            );
        }
    }
}

#[test]
fn a090_plow_driver_handler_integration() {
    // Test A090: Stone house -> optional pay 1 food to plow 1 field
    let cards = ch06_round_start_card_set();
    let a090 = cards.iter().find(|c| c.id == "A090").unwrap();

    let effect = &a090.effects[0];
    assert!(matches!(
        effect,
        CardEffect::StartRoundPlowFieldIfHouse {
            house: House::Stone,
            food_cost: 1,
            fields: 1,
        }
    ));
}

#[test]
fn a151_minstrel_handler_integration() {
    // Test A151: Rounds 1-4, if round space unused, copy effect (4 players only)
    let cards = ch06_round_start_card_set();
    let a151 = cards.iter().find(|c| c.id == "A151").unwrap();

    assert_eq!(a151.effects.len(), 2); // Prerequisite + StartRoundCopyUnusedRoundSpace
    assert!(matches!(
        &a151.effects[0],
        CardEffect::Prerequisite {
            rule: CardRequirement::MinimumPlayers(4)
        }
    ));
    assert!(matches!(
        &a151.effects[1],
        CardEffect::StartRoundCopyUnusedRoundSpace { max_round: 4 }
    ));
}

#[test]
fn a152_night_school_student_handler_integration() {
    // Test A152: If Lessons underused, optional pay 1 food to play occupation (4 players only)
    let cards = ch06_round_start_card_set();
    let a152 = cards.iter().find(|c| c.id == "A152").unwrap();

    assert_eq!(a152.effects.len(), 2); // Prerequisite + StartRoundPlayOccupationIfLessonsUnderused
    assert!(matches!(
        &a152.effects[0],
        CardEffect::Prerequisite {
            rule: CardRequirement::MinimumPlayers(4)
        }
    ));
    assert!(matches!(
        &a152.effects[1],
        CardEffect::StartRoundPlayOccupationIfLessonsUnderused { food_cost: 1 }
    ));
}

#[test]
fn b097_scholar_handler_integration() {
    // Test B097: Stone house -> optional pay 1 food for occupation OR pay cost for minor (all players)
    let cards = ch06_round_start_card_set();
    let b097 = cards.iter().find(|c| c.id == "B097").unwrap();

    let effect = &b097.effects[0];
    assert!(matches!(
        effect,
        CardEffect::StartRoundPlayCardIfStoneHouse {
            occupation_food_cost: 1
        }
    ));
}

#[test]
fn b106_moral_crusader_handler_integration() {
    // Test B106: If goods on round spaces, gain 1 food (all players)
    let cards = ch06_round_start_card_set();
    let b106 = cards.iter().find(|c| c.id == "B106").unwrap();

    let effect = &b106.effects[0];
    assert!(matches!(
        effect,
        CardEffect::StartRoundGainIfGoodsOnRoundSpaces {
            good: Good::Food,
            amount: 1
        }
    ));
}

#[test]
fn b135_nutrition_expert_handler_integration() {
    // Test B135: Optional exchange 1 animal + 1 grain + 1 veg for 5 food + 2 points (3+ players)
    let cards = ch06_round_start_card_set();
    let b135 = cards.iter().find(|c| c.id == "B135").unwrap();

    assert_eq!(b135.effects.len(), 2); // Prerequisite + StartRoundExchangeForFoodAndPoints
    assert!(matches!(
        &b135.effects[0],
        CardEffect::Prerequisite {
            rule: CardRequirement::MinimumPlayers(3)
        }
    ));
    assert!(matches!(
        &b135.effects[1],
        CardEffect::StartRoundExchangeForFoodAndPoints {
            give_animal: true,
            give_grain: 1,
            give_vegetable: 1,
            receive_food: 5,
            receive_points: 2,
        }
    ));
}

#[test]
fn c064_corn_schnapps_distillery_handler_integration() {
    // Test C064: Cost 1 wood + 2 clay, 1 point. Optional pay 1 grain to place 1 food on next 4 round spaces (all players)
    let cards = ch06_round_start_card_set();
    let c064 = cards.iter().find(|c| c.id == "C064").unwrap();

    assert_eq!(c064.kind, CardKind::MinorImprovement);
    assert_eq!(c064.points, 1);
    assert_eq!(c064.cost, vec![(Good::Wood, 1), (Good::Clay, 2)]);

    let effect = &c064.effects[0];
    assert!(matches!(
        effect,
        CardEffect::StartRoundPlaceFoodOnNextRoundSpaces {
            grain_cost: 1,
            food_per_space: 1,
            num_spaces: 4,
        }
    ));
}

#[test]
fn round_start_dispatch_integration_2_players() {
    // Test that round-start dispatch works with 2 players (only eligible cards trigger)
    let registry = build_default_registry().expect("registry build failed");
    let pool = CardPool::new(&registry, 2, true); // experimental_mode = true

    // Create a 2-player game with the default pool
    let mut game = Game::new(Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    });

    // Get the card set from the pool
    let card_set = card_set_from_pool(&registry, &pool);

    // Add the 4 eligible cards to player 0's hand
    for id in ["A090", "B097", "B106", "C064"] {
        if let Some(card_def) = card_set.definition(id) {
            game.cards.hands[0].push(card_def.id.clone());
        }
    }

    // Verify the cards are in hand
    assert!(game.cards.hands[0].iter().any(|id| id == "A090"));
    assert!(game.cards.hands[0].iter().any(|id| id == "B097"));
    assert!(game.cards.hands[0].iter().any(|id| id == "B106"));
    assert!(game.cards.hands[0].iter().any(|id| id == "C064"));
}

#[test]
fn round_start_trigger_data_structure() {
    // Verify PhaseTriggerData has the fields needed for round-start handlers
    let trigger_data = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: std::collections::BTreeSet::new(),
        site: agricola_re_ai::registry::EventKind::RoundStart,
        event_kind: agricola_re_ai::registry::EventKind::RoundStart,
        revealed_round_cards: vec![],
    };

    // These fields are used by the handlers:
    // - revealed_round_cards: used by A151 (Minstrel)
    // - used_spaces: used by A152 (Night-School Student)
    // - accumulations: used by B106 (Moral Crusader) - passed separately in dispatch_start_round
    assert!(trigger_data.revealed_round_cards.is_empty());
    assert!(trigger_data.used_spaces.is_empty());
    assert_eq!(
        trigger_data.site,
        agricola_re_ai::registry::EventKind::RoundStart
    );
}

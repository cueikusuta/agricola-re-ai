//! M8-1: hidden-state changes must not affect an AI's public Observation.
use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::{Config, Game};
use rand::RngCore;

fn seeded_card_game() -> Game {
    let config = Config {
        seed: 42,
        players: 2,
        additional_action_space_enabled: false,
    };
    Game::with_cards(config, verified_basic_card_set(2).unwrap()).unwrap()
}

#[test]
fn opponent_hand_order_and_identity_are_not_observable() {
    let original = seeded_card_game();
    let own_view = original.observation(0).unwrap();

    // The opponent's private hand ordering is not a public observation.
    let mut reordered = original.clone();
    assert!(reordered.cards.hands[1].len() > 1);
    reordered.cards.hands[1].reverse();
    reordered.validate().unwrap();
    assert_ne!(reordered.state_hash(), original.state_hash());
    assert_eq!(reordered.observation(0).unwrap(), own_view);

    // Exchange a hidden card with an undealt card of the same kind.
    // This keeps hand sizes, kind quotas and the complete card partition valid.
    let pair = original.cards.hands[1]
        .iter()
        .enumerate()
        .find_map(|(hand_index, held)| {
            let kind = original.card_set.definition(held).unwrap().kind;
            original
                .cards
                .undealt
                .iter()
                .position(|id| original.card_set.definition(id).unwrap().kind == kind)
                .map(|undealt_index| (hand_index, undealt_index))
        })
        .expect("the verified two-player card pool has undealt cards");
    let mut swapped = original.clone();
    let (hand_index, undealt_index) = pair;
    let held = swapped.cards.hands[1][hand_index].clone();
    swapped.cards.hands[1][hand_index] = swapped.cards.undealt[undealt_index].clone();
    swapped.cards.undealt[undealt_index] = held;
    swapped.validate().unwrap();
    assert_ne!(swapped.state_hash(), original.state_hash());
    assert_eq!(swapped.observation(0).unwrap(), own_view);
    assert_ne!(
        swapped.observation(1).unwrap(),
        original.observation(1).unwrap()
    );
}

#[test]
fn unrevealed_round_order_and_internal_rng_are_not_observable() {
    let original = seeded_card_game();
    let view = original.observation(0).unwrap();

    let mut future = original.clone();
    assert!(future.hidden_round_deck.len() >= 2);
    // Both earliest vector slots are unrevealed cards from the final stage.
    assert_eq!(
        future.hidden_round_deck[0].stage(),
        future.hidden_round_deck[1].stage()
    );
    future.hidden_round_deck.swap(0, 1);
    future.validate().unwrap();
    assert_ne!(future.state_hash(), original.state_hash());
    assert_eq!(future.observation(0).unwrap(), view);

    let mut rng_change = original.clone();
    let _ = rng_change.rng.round.next_u32();
    assert_ne!(rng_change.state_hash(), original.state_hash());
    assert_eq!(rng_change.observation(0).unwrap(), view);
}

#[test]
fn observation_contains_only_intended_public_fields_and_own_hand() {
    let game = seeded_card_game();
    let view = game.observation(0).unwrap();
    assert_eq!(view.hand, game.cards.hands[0]);
    assert_eq!(view.hand_counts, vec![14, 14]);
    assert_eq!(view.public, game.public);
    assert_eq!(view.card_definitions, game.card_set.definitions);

    let value = serde_json::to_value(&view).unwrap();
    let fields = value.as_object().unwrap();
    for secret in [
        "rng",
        "hidden_round_deck",
        "hands",
        "undealt",
        "round_seed",
        "action_seed",
    ] {
        assert!(
            !fields.contains_key(secret),
            "secret field leaked: {secret}"
        );
    }
    assert!(
        game.observation(2).is_err(),
        "invalid player must be rejected"
    );
}

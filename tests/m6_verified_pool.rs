//! Verified only: the 2-player built-in A/B pack must play and replay end-to-end.
use agricola_re_ai::cards::{verified_basic_card_set, CardKind};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::policy::{play_to_end, SimpleDetails};
use agricola_re_ai::{Config, Game};

#[test]
fn verified_pool_has_fourteen_unique_occupations_and_minors() {
    let set = verified_basic_card_set(2).unwrap();
    assert_eq!(set.definitions.len(), 34);
    assert_eq!(
        set.definitions
            .iter()
            .filter(|c| c.kind == CardKind::Occupation)
            .count(),
        15
    );
    assert_eq!(
        set.definitions
            .iter()
            .filter(|c| c.kind == CardKind::MinorImprovement)
            .count(),
        19
    );
    assert!(verified_basic_card_set(3).is_err());
}

#[test]
fn fixed_seed_cards_complete_and_preserve_hashes_and_hidden_hands() {
    for seed in [22, 49] {
        let config = Config {
            players: 2,
            seed,
            additional_action_space_enabled: seed % 2 == 0,
        };
        let set = verified_basic_card_set(2).unwrap();
        let mut game = Game::with_cards(config.clone(), set.clone()).unwrap();
        let human = game.observation(0).unwrap();
        assert_eq!(human.hand, game.cards.hands[0]);
        assert_eq!(human.hand_counts, vec![14, 14]);
        assert!(human
            .hand
            .iter()
            .all(|id| !game.cards.hands[1].contains(id)));
        let observed_json = serde_json::to_value(human).unwrap();
        assert!(observed_json.get("hands").is_none());
        assert!(observed_json.get("undealt").is_none());
        play_to_end(&mut game, &EvaluationPolicy, &SimpleDetails).unwrap();
        assert!(game.public.finished, "seed {seed}");
        assert_eq!(game.public.round, 14);
        game.validate().unwrap();
        let replay = Game::replay_with_cards(config, set, &game.log.events).unwrap();
        assert_eq!(replay.state_hash(), game.state_hash());
        let saved = game.save_json().unwrap();
        let restored = Game::load_json(&saved).unwrap();
        assert_eq!(restored.state_hash(), game.state_hash());
    }
}

#[test]
fn harvest_preview_includes_conditional_card_rewards_without_committing() {
    use agricola_re_ai::engine::{Action, Detail};
    use agricola_re_ai::model::Space;
    use std::collections::BTreeSet;

    // B062 awards food when a worker chooses Grain Seeds after Farmland
    // has already been occupied. A harvest preview must show that food.
    let (mut game, owner) = (0..32)
        .find_map(|seed| {
            let config = Config {
                players: 2,
                seed,
                additional_action_space_enabled: false,
            };
            let game = Game::with_cards(config, verified_basic_card_set(2).unwrap()).ok()?;
            let owner = game
                .cards
                .hands
                .iter()
                .position(|hand| hand.iter().any(|id| id == "B062"))?;
            Some((game, owner))
        })
        .expect("B062 should be dealt in at least one fixed seed");

    game.cards.hands[owner].retain(|id| id != "B062");
    game.public.players[owner].played_cards.push("B062".into());
    game.public.round = 4;
    game.public.active_player = owner;
    game.public.used = BTreeSet::from([Space::Farmland]);
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[owner] -= 1;

    let before = game.state_hash();
    let original_food = game.public.players[owner].supply.food;
    let preview = game
        .preview_harvest(
            &Action {
                space: Space::GrainSeeds,
                detail: Detail::None,
            },
            None,
        )
        .unwrap();
    assert_eq!(preview.players[owner].supply.food, original_food + 3);
    assert_eq!(game.state_hash(), before, "preview must not commit");
}

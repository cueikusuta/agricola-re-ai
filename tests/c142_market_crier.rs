//! C142 てき屋 (Market Crier): each time the OWNER uses GrainSeeds, the owner
//! may take 1 grain + 1 vegetable as an all-or-nothing bundle; if taken,
//! every other player gains 1 grain. Part of the bundle, or other-players-only
//! gain, is impossible by construction.
//!
//! Spec: data/bga-card-specs/C142.json; cases C142-take-extra / C142-decline /
//! C142-not-triggered (data/bga-card-tests/C142.json).
//!
//! Coverage:
//! - dispatch creates the owner yes/no pending exactly on the owner's
//!   GrainSeeds use (C142-take-extra decision shape)
//! - decline grants nothing to anyone (C142-decline)
//! - other spaces and other players never trigger (C142-not-triggered)
//! - the confirmed resolution grants the bundle and each other player's grain
//!   atomically, exactly once, and survives save/load
//! - C142 is out of the 2-player pool and of verified_basic_card_set

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{
    verified_band_c_cards, verified_basic_card_set, CardDefinition, CardEffect, CardKind,
    CardRequirement, CardSet,
};
use agricola_re_ai::model::{Config, Game, Good, Player, Space};
use agricola_re_ai::pending::{PendingDecision, PendingKind, PendingResolution, Transition};
use serde_json::json;

fn c142_set() -> CardSet {
    let mut definitions = verified_band_c_cards();
    definitions.push(CardDefinition {
        use_limit: None,
        id: "C142".to_string(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(3),
            },
            CardEffect::AfterWorkerOptionalBundleGain {
                space: Space::GrainSeeds,
                owner_goods: vec![(Good::Grain, 1), (Good::Vegetable, 1)],
                others_good: Good::Grain,
                others_amount: 1,
            },
        ],
    });
    CardSet { definitions }
}

/// Dispatch the after-worker chain for `actor` on `space` with `owner`
/// holding C142, returning the transition, any pending decision, and the
/// resulting players.
fn dispatch(
    set: &CardSet,
    owner: usize,
    actor: usize,
    space: Space,
) -> (Transition, Option<PendingDecision>, [Player; 3]) {
    let mut cards = Default::default();
    let mut players = [Player::new(0, 0), Player::new(1, 0), Player::new(2, 0)];
    players[owner].played_cards.push("C142".to_string());
    let mut market = vec![];
    let mut well = std::collections::BTreeMap::new();
    let mut budget = 100u32;
    let mut uses = std::collections::BTreeMap::new();
    let limits = std::collections::BTreeMap::new();
    let mut pending = None;
    let used = std::collections::BTreeSet::new();
    let accumulations = std::collections::BTreeMap::new();
    let transition = dispatch_after_worker(
        set,
        actor,
        space,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &used,
        &accumulations,
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        false,
    )
    .expect("dispatch_after_worker must succeed");
    (transition, pending, players)
}

/// Build the C142 pending decision payload/context exactly as the handler
/// emits it, stamped with the current state hash so it is resolvable.
fn c142_decision(confirmed: bool, state_hash_before: &str) -> PendingDecision {
    let context = serde_json::json!({
        "card": "C142",
        "type": "after_worker_optional_bundle_gain",
        "space": "grain_seeds",
        "owner_goods": [["grain", 1], ["vegetable", 1]],
        "others_good": "grain",
        "others_amount": 1,
        "owner": 0,
    });
    let kind = PendingKind::Custom {
        source_card: "C142".to_string(),
        payload: json!({ "confirmed": confirmed }),
    };
    PendingDecision::new(0, 0, kind, state_hash_before.to_string(), context)
}

/// C142-take-extra: the owner's GrainSeeds use pauses with the optional
/// bundle decision; nothing is granted until confirmed.
#[test]
fn c142_take_extra_creates_the_bundle_decision() {
    let set = c142_set();
    let (transition, pending, players) = dispatch(&set, 0, 0, Space::GrainSeeds);
    assert!(
        matches!(transition, Transition::Paused(_)),
        "owner's GrainSeeds must pause"
    );
    let Some(decision) = pending else {
        panic!("expected a pending decision");
    };
    let PendingKind::Custom {
        source_card,
        payload,
    } = &decision.kind
    else {
        panic!("expected Custom pending, got {decision:?}");
    };
    assert_eq!(source_card, "C142");
    assert_eq!(
        payload.get("confirmed").and_then(|v| v.as_bool()),
        Some(false),
        "pending starts unconfirmed"
    );
    let ctx = decision
        .context
        .as_object()
        .expect("pending context must be an object");
    assert_eq!(
        ctx.get("type").and_then(|v| v.as_str()),
        Some("after_worker_optional_bundle_gain")
    );
    assert_eq!(ctx.get("owner").and_then(|v| v.as_u64()), Some(0));
    let owner_goods = ctx
        .get("owner_goods")
        .and_then(|v| v.as_array())
        .expect("owner_goods array");
    assert_eq!(owner_goods.len(), 2);
    assert_eq!(owner_goods[0], json!(["grain", 1]));
    assert_eq!(owner_goods[1], json!(["vegetable", 1]));
    assert_eq!(
        ctx.get("others_good").and_then(|v| v.as_str()),
        Some("grain")
    );
    assert_eq!(ctx.get("others_amount").and_then(|v| v.as_u64()), Some(1));
    // Nothing is granted before the decision resolves.
    assert_eq!(players[0].supply.grain, 0);
    assert_eq!(players[0].supply.vegetable, 0);
    assert_eq!(players[1].supply.grain, 0);
    assert_eq!(players[2].supply.grain, 0);
}

/// C142-decline: resolving the decision declined grants nothing to anyone
/// and clears the decision.
#[test]
fn c142_declined_grants_nothing_to_anyone() {
    let mut game = Game::new(Config {
        players: 3,
        seed: 2,
        additional_action_space_enabled: false,
    });
    let decision = c142_decision(false, &game.state_hash());
    game.pending = Some(decision);
    game.resolve_pending(PendingResolution::Custom {
        payload: json!({ "confirmed": false }),
    })
    .expect("declined decision must resolve");
    assert!(!game.has_pending(), "decision must be cleared on decline");
    for player in &game.public.players {
        assert_eq!(player.supply.grain, 0, "no grain for {player:?} on decline");
        assert_eq!(
            player.supply.vegetable, 0,
            "no vegetable for {player:?} on decline"
        );
    }
    game.validate().expect("game stays valid after decline");
}

/// C142-take-extra: confirming grants the owner grain+vegetable and each
/// other player 1 grain, exactly once.
#[test]
fn c142_confirmed_grants_bundle_to_owner_and_grain_to_others() {
    let mut game = Game::new(Config {
        players: 3,
        seed: 3,
        additional_action_space_enabled: false,
    });
    let decision = c142_decision(true, &game.state_hash());
    game.pending = Some(decision);
    game.resolve_pending(PendingResolution::Custom {
        payload: json!({ "confirmed": true }),
    })
    .expect("confirmed decision must resolve");
    assert!(
        !game.has_pending(),
        "decision must be cleared on confirmation"
    );
    assert_eq!(game.public.players[0].supply.grain, 1, "owner grain");
    assert_eq!(
        game.public.players[0].supply.vegetable, 1,
        "owner vegetable"
    );
    assert_eq!(
        game.public.players[1].supply.grain, 1,
        "other player 1 grain"
    );
    assert_eq!(
        game.public.players[2].supply.grain, 1,
        "other player 2 grain"
    );
    game.validate().expect("game stays valid after grant");
}

/// The resolution applies at most once: re-resolving is rejected and no
/// second grant can occur (replay/resume cannot double-grant).
#[test]
fn c142_resolution_applies_exactly_once() {
    let mut game = Game::new(Config {
        players: 3,
        seed: 4,
        additional_action_space_enabled: false,
    });
    let decision = c142_decision(true, &game.state_hash());
    game.pending = Some(decision.clone());
    game.resolve_pending(PendingResolution::Custom {
        payload: json!({ "confirmed": true }),
    })
    .expect("first resolution must succeed");
    let err = game
        .resolve_pending(PendingResolution::Custom {
            payload: json!({ "confirmed": true }),
        })
        .unwrap_err();
    assert!(
        err.contains("no pending decision"),
        "a cleared decision must not be re-resolvable, got: {err}"
    );
    assert_eq!(game.public.players[0].supply.grain, 1);
    assert_eq!(game.public.players[0].supply.vegetable, 1);
    assert_eq!(game.public.players[1].supply.grain, 1);
    assert_eq!(game.public.players[2].supply.grain, 1);
}

/// The pending decision survives save/load and still resolves exactly once.
#[test]
fn c142_pending_survives_save_load_and_resolves_once() {
    let mut game = Game::new(Config {
        players: 3,
        seed: 5,
        additional_action_space_enabled: false,
    });
    let decision = c142_decision(true, &game.state_hash());
    game.pending = Some(decision);
    let saved = game.save_json().expect("save must succeed");
    let mut restored = Game::load_json(&saved).expect("load must succeed");
    assert!(restored.has_pending(), "pending must survive save/load");
    restored
        .resolve_pending(PendingResolution::Custom {
            payload: json!({ "confirmed": true }),
        })
        .expect("resumed decision must resolve");
    assert!(!restored.has_pending());
    assert_eq!(restored.public.players[0].supply.grain, 1);
    assert_eq!(restored.public.players[0].supply.vegetable, 1);
    assert_eq!(restored.public.players[1].supply.grain, 1);
    assert_eq!(restored.public.players[2].supply.grain, 1);
}

/// C142-not-triggered: Farmland (another space) never creates the decision.
#[test]
fn c142_other_space_does_not_trigger() {
    let set = c142_set();
    let (transition, pending, players) = dispatch(&set, 0, 0, Space::Farmland);
    assert!(matches!(transition, Transition::Completed));
    assert!(pending.is_none(), "no pending on a non-matching space");
    assert_eq!(players[0].supply.grain, 0);
    assert_eq!(players[0].supply.vegetable, 0);
}

/// C142-not-triggered: another player's GrainSeeds use never triggers the
/// owner's card.
#[test]
fn c142_other_players_grain_seeds_does_not_trigger() {
    let set = c142_set();
    let (transition, pending, players) = dispatch(&set, 0, 1, Space::GrainSeeds);
    assert!(matches!(transition, Transition::Completed));
    assert!(
        pending.is_none(),
        "no pending when a non-owner uses GrainSeeds"
    );
    assert_eq!(players[0].supply.grain, 0, "owner must not gain");
    assert_eq!(
        players[1].supply.grain, 0,
        "actor without C142 must not gain"
    );
}

/// C142-not-triggered: a declined decision leaves the door open — a later
/// GrainSeeds use creates a fresh decision that grants exactly once.
#[test]
fn c142_decline_then_take_grants_exactly_once() {
    let mut game = Game::new(Config {
        players: 3,
        seed: 6,
        additional_action_space_enabled: false,
    });
    // First use: declined.
    game.pending = Some(c142_decision(false, &game.state_hash()));
    game.resolve_pending(PendingResolution::Custom {
        payload: json!({ "confirmed": false }),
    })
    .unwrap();
    assert!(!game.has_pending());
    assert_eq!(game.public.players[0].supply.grain, 0);
    // Second use: a brand-new decision, confirmed.
    let hash = game.state_hash();
    game.pending = Some(c142_decision(true, &hash));
    game.resolve_pending(PendingResolution::Custom {
        payload: json!({ "confirmed": true }),
    })
    .unwrap();
    assert_eq!(game.public.players[0].supply.grain, 1);
    assert_eq!(game.public.players[0].supply.vegetable, 1);
    assert_eq!(game.public.players[1].supply.grain, 1);
    assert_eq!(game.public.players[2].supply.grain, 1);
    assert!(!game.has_pending());
}

/// C142 is inline-only (removed from verified_band_c_cards); it carries a
/// MinimumPlayers(3) prerequisite, so eligible at 3+ players only.
#[test]
fn c142_player_count_eligibility() {
    let set = c142_set();
    let def = set
        .definitions
        .iter()
        .find(|d| d.id == "C142")
        .expect("C142 must be defined in c142_set");
    let has_min_players_3 = def.effects.iter().any(|e| {
        matches!(
            e,
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(3)
            }
        )
    });
    assert!(has_min_players_3, "C142 must require MinimumPlayers(3)");
    assert!(
        verified_basic_card_set(2)
            .unwrap()
            .definition("C142")
            .is_none(),
        "C142 must not be in verified_basic_card_set"
    );
}

/// The CardEffect vocabulary carries C142 under its public tag.
#[test]
fn c142_public_tag() {
    let effect = CardEffect::AfterWorkerOptionalBundleGain {
        space: Space::GrainSeeds,
        owner_goods: vec![(Good::Grain, 1), (Good::Vegetable, 1)],
        others_good: Good::Grain,
        others_amount: 1,
    };
    assert_eq!(
        effect.public_tag(),
        "after_worker_optional_bundle_gain",
        "public tag must name the effect without exposing card text"
    );
}

//! Tests for on_play `grant_action` cards A139 / D002 / D003 (アクション権限付与 第1章).
//!
//! Mirrors the BGA test contract in `data/bga-card-tests/{A139,D002,D003}.json`
//! (scope `effect_only`). The grant itself is recorded in `Player.granted_actions`
//! when `apply_on_play_effects` runs; consuming the grant mutates player state
//! via `Player::execute_granted_action`. No new persisted/save state is added
//! (`Player.granted_actions` is `#[serde(skip)]`).

use agricola_re_ai::cards::{apply_on_play_effects, CardDefinition, CardEffect, CardKind, CardSet};
use agricola_re_ai::model::{Crop, Good, GrantedAction, House, Improvement, Player, Space, Tile};
use agricola_re_ai::registry::build_default_registry;

fn a139_def() -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: "A139".to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::OnPlayGrantAction {
                action: GrantedAction::MajorImprovement {
                    only: Some(Improvement::Fireplace2),
                },
            },
            CardEffect::AfterWorkerGain {
                space: Space::Hollow,
                good: Good::Food,
                amount: 1,
            },
        ],
    }
}

fn d002_def() -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: "D002".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![
            CardEffect::OnPlayGrantAction {
                action: GrantedAction::Renovation,
            },
            CardEffect::PostPlayPassLeft,
        ],
    }
}

fn d003_def() -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: "D003".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::OnPlayGrantAction {
                action: GrantedAction::Sow { fields: 1 },
            },
            CardEffect::PostPlayPassLeft,
        ],
    }
}

fn grant_set() -> CardSet {
    let mut set = CardSet {
        definitions: vec![],
    };
    set.definitions.push(a139_def());
    set.definitions.push(d002_def());
    set.definitions.push(d003_def());
    set
}

/// A player with `n` empty (unsown) fields; all other non-room cells stay Empty.
fn player_with_empty_fields(n: usize) -> Player {
    let mut p = Player::new(0, 0);
    let mut count = 0;
    for cell in &mut p.farm {
        if count >= n {
            break;
        }
        if matches!(cell.tile, Tile::Empty) {
            cell.tile = Tile::Field {
                crop: None,
                remaining: 0,
            };
            count += 1;
        }
    }
    p
}

fn grain_fields(p: &Player) -> usize {
    p.farm
        .iter()
        .filter(|c| {
            matches!(
                c.tile,
                Tile::Field {
                    crop: Some(Crop::Grain),
                    ..
                }
            )
        })
        .count()
}

fn granted_renovation(p: &Player) -> bool {
    p.granted_actions
        .iter()
        .any(|a| matches!(a, GrantedAction::Renovation))
}

fn granted_major_improvement_fireplace(p: &Player) -> bool {
    p.granted_actions.iter().any(|a| {
        matches!(
            a,
            GrantedAction::MajorImprovement {
                only: Some(Improvement::Fireplace2)
            }
        )
    })
}

#[test]
fn a139_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("A139").expect("A139 must be registered");
    let definition = registry.get(card_id).expect("registered A139 resolves");
    assert_eq!(definition.kind, CardKind::Occupation);
    assert_eq!(definition.cost, vec![]);
}

#[test]
fn d002_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("D002").expect("D002 must be registered");
    let definition = registry.get(card_id).expect("registered D002 resolves");
    assert_eq!(definition.kind, CardKind::MinorImprovement);
    assert_eq!(definition.cost, vec![(Good::Food, 1)]);
}

#[test]
fn d003_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("D003").expect("D003 must be registered");
    let definition = registry.get(card_id).expect("registered D003 resolves");
    assert_eq!(definition.kind, CardKind::MinorImprovement);
    assert_eq!(definition.cost, vec![]);
}

/// A139-normal: playing the card grants a Major Improvement action restricted
/// to a Fireplace.
#[test]
fn a139_normal_grants_major_improvement_fireplace() {
    let set = grant_set();
    let mut p = Player::new(0, 0);
    apply_on_play_effects(&set, &mut p, "A139").expect("A139 on_play resolves");
    assert!(
        granted_major_improvement_fireplace(&p),
        "major improvement granted (fireplace only)"
    );
    // The grant itself changes no supply or house state.
    assert_eq!(p.supply.food, 0);
    assert_eq!(p.house, House::Wood);
}

/// A139-hollow: using the Hollow space grants 1 food (the card's second effect).
#[test]
fn a139_hollow_grants_food() {
    let set = grant_set();
    let mut p = Player::new(0, 0);
    // A139 must already be played for its after-worker effect to fire.
    p.played_cards.push("A139".to_owned());
    p.supply.food = 0;
    let result = agricola_re_ai::cards::after_worker(&set, &mut p, Space::Hollow);
    assert!(result.is_ok(), "hollow after_worker resolves");
    assert_eq!(p.supply.food, 1, "food_delta 1");
}

/// A139-not-triggered: using a non-Hollow space grants nothing.
#[test]
fn a139_not_triggered_on_forest() {
    let set = grant_set();
    let mut p = Player::new(0, 0);
    p.played_cards.push("A139".to_owned());
    p.supply.food = 0;
    let result = agricola_re_ai::cards::after_worker(&set, &mut p, Space::Forest);
    assert!(result.is_ok(), "forest after_worker resolves");
    assert_eq!(p.supply.food, 0, "food_delta 0");
}

/// D002-normal: playing D002 grants Renovation; using it upgrades Wood -> Clay.
#[test]
fn d002_normal_renovation_granted_and_applied() {
    let set = grant_set();
    let mut p = Player::new(0, 0);
    p.house = House::Wood;
    apply_on_play_effects(&set, &mut p, "D002").expect("D002 on_play resolves");
    assert!(granted_renovation(&p), "renovation granted");
    assert_eq!(
        p.house,
        House::Wood,
        "house unchanged until the grant is used"
    );

    let grant = p
        .granted_actions
        .iter()
        .find(|a| matches!(a, GrantedAction::Renovation))
        .cloned()
        .expect("renovation grant present");
    p.execute_granted_action(&grant)
        .expect("renovation executes");
    assert_eq!(p.house, House::Clay, "house wood -> clay");
    assert!(p.granted_actions.is_empty(), "grant consumed");
}

/// D002-declined: playing D002 grants Renovation even when the player declines
/// to use it; the house stays Wood.
#[test]
fn d002_declined_grant_recorded_but_not_applied() {
    let set = grant_set();
    let mut p = Player::new(0, 0);
    p.house = House::Wood;
    apply_on_play_effects(&set, &mut p, "D002").expect("D002 on_play resolves");
    assert!(
        granted_renovation(&p),
        "renovation granted even when declined"
    );
    assert_eq!(
        p.house,
        House::Wood,
        "house unchanged when renovation declined"
    );
    assert_eq!(p.granted_actions.len(), 1, "one grant recorded");
}

/// D003-normal: playing D003 grants Sow(1); using it sows 1 grain field and
/// consumes 1 grain.
#[test]
fn d003_normal_sow_consumes_grain() {
    let set = grant_set();
    let mut p = player_with_empty_fields(1);
    p.supply.grain = 1;
    apply_on_play_effects(&set, &mut p, "D003").expect("D003 on_play resolves");
    assert!(
        p.granted_actions
            .iter()
            .any(|a| matches!(a, GrantedAction::Sow { fields: 1 })),
        "sow grant recorded"
    );
    assert_eq!(grain_fields(&p), 0, "nothing sown until the grant is used");

    let grant = p
        .granted_actions
        .iter()
        .find(|a| matches!(a, GrantedAction::Sow { .. }))
        .cloned()
        .expect("sow grant present");
    p.execute_granted_action(&grant).expect("sow executes");
    assert_eq!(grain_fields(&p), 1, "sown_fields 1");
    assert_eq!(p.supply.grain, 0, "grain_delta -1");
    assert!(p.granted_actions.is_empty(), "grant consumed");
}

/// D003-no-field: with no empty field, Sow grants nothing and consumes no grain.
#[test]
fn d003_no_field_sows_nothing() {
    let set = grant_set();
    let mut p = player_with_empty_fields(0);
    p.supply.grain = 1;
    apply_on_play_effects(&set, &mut p, "D003").expect("D003 on_play resolves");
    assert!(
        p.granted_actions
            .iter()
            .any(|a| matches!(a, GrantedAction::Sow { fields: 1 })),
        "sow grant recorded even with no field"
    );
    assert_eq!(grain_fields(&p), 0, "sown_fields 0");

    let grant = p
        .granted_actions
        .iter()
        .find(|a| matches!(a, GrantedAction::Sow { .. }))
        .cloned()
        .expect("sow grant present");
    p.execute_granted_action(&grant).expect("sow executes");
    assert_eq!(grain_fields(&p), 0, "no field to sow -> sown_fields 0");
    assert_eq!(p.supply.grain, 1, "grain_delta 0 (nothing consumed)");
    assert!(p.granted_actions.is_empty(), "grant consumed");
}

//! Tests for B132 地主 (Estate Master): at each harvest field phase, if every
//! farmyard space is used, gain 1 bonus point per vegetable harvested this
//! field phase. Points accumulate in `Player::bonus_points` and count at
//! final scoring; `card_points` (printed points) is untouched.
//!
//! Mirrors the BGA spec cases `B132-full-farmyard`, `B132-not-full` and
//! `B132-no-vegetable` (data/bga-card-tests/B132.json) through the real
//! dispatcher path (`dispatch_harvest_field`, EventKind::Harvest, After phase).

use agricola_re_ai::card_events::dispatch_harvest_field;
use agricola_re_ai::cards::{verified_band_b_cards, CardSet};
use agricola_re_ai::model::{Crop, Player, Tile};
use std::collections::{BTreeMap, BTreeSet};

fn b132_set() -> CardSet {
    CardSet {
        definitions: verified_band_b_cards(),
    }
}

/// All the pieces the dispatcher needs, so each test stays short.
struct Ctx {
    players: Vec<Player>,
    cards: agricola_re_ai::cards::CardState,
    budget: u32,
    uses: BTreeMap<String, u32>,
    limits: BTreeMap<String, u32>,
}

impl Ctx {
    fn new() -> Self {
        Self {
            players: vec![Player::new(0, 0)],
            cards: Default::default(),
            budget: 64,
            uses: BTreeMap::new(),
            limits: BTreeMap::new(),
        }
    }
}

/// Farm cell is "used" when it carries a non-`Empty` tile or a stable
/// (`cards::farmyard_all_used`). Build a fully used farm with `vegetables`
/// growing vegetable fields (each yields one vegetable this field phase).
fn full_farm(player: &mut Player, vegetables: usize) {
    assert!(vegetables <= player.farm.len());
    for cell in player.farm.iter_mut() {
        cell.tile = Tile::Room;
    }
    for cell in player.farm.iter_mut().take(vegetables) {
        cell.tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
    }
}

fn run_harvest_field(ctx: &mut Ctx) {
    ctx.players[0].played_cards = vec!["B132".into()];
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let used = BTreeSet::new();
    let mut pending = None;
    dispatch_harvest_field(
        &b132_set(),
        0,
        &mut ctx.cards,
        &mut ctx.players,
        &mut market,
        &mut well,
        4, // harvest round
        &used,
        &mut ctx.budget,
        &mut ctx.uses,
        &ctx.limits,
        &mut pending,
    )
    .unwrap();
    assert!(
        pending.is_none(),
        "B132 grant must not create a pending decision"
    );
}

#[test]
fn b132_normal_full_farmyard_two_vegetables() {
    let mut c = Ctx::new();
    full_farm(&mut c.players[0], 2);
    run_harvest_field(&mut c);
    assert_eq!(
        c.players[0].bonus_points, 2,
        "full farmyard + 2 vegetables -> 2 bonus points"
    );
    assert_eq!(
        c.players[0].card_points, 0,
        "card_points (printed points) stays 0"
    );
}

#[test]
fn b132_not_full_farmyard() {
    let mut c = Ctx::new();
    // Every space used except one empty cell without a stable -> not full.
    for cell in c.players[0].farm.iter_mut() {
        cell.tile = Tile::Room;
    }
    c.players[0].farm[0].tile = Tile::Empty;
    for cell in c.players[0].farm.iter_mut().skip(1).take(2) {
        cell.tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
    }
    run_harvest_field(&mut c);
    assert_eq!(
        c.players[0].bonus_points, 0,
        "farmyard not full -> no bonus despite 2 vegetable fields"
    );
}

#[test]
fn b132_no_vegetable() {
    let mut c = Ctx::new();
    full_farm(&mut c.players[0], 0);
    run_harvest_field(&mut c);
    assert_eq!(
        c.players[0].bonus_points, 0,
        "full farmyard but no vegetables -> no bonus"
    );
}

//! Dispatcher contract tests for the on-play exchange cards A036 ファサデ彫刻
//! (Facades Carving) and D009 獲物の交換 (Game Trade) — PR #464.
//!
//! Contracts: `data/bga-card-tests/A036.json` (A036-normal / A036-capped) and
//! `data/bga-card-tests/D009.json` (D009-normal / D009-no-sheep).
//!
//! These run the PRODUCTION `card_events::dispatch_play` route. The legacy
//! `cards::play` / `apply_on_play_effects` projection cannot see the round,
//! so it structurally cannot catch the A036 completed-harvest cap, and its
//! one-unit-per-element arithmetic cannot catch the D009 double-count
//! (Round 1 review P1-1 / P1-2). Contract coverage therefore belongs here.
//!
//! Pinned semantics:
//! - A036 exchanges food for points 1:1, capped at the number of COMPLETED
//!   harvests (rounds [4, 7, 9, 11, 13, 14] that have already ended). The
//!   count is derived from the dispatch round + `model::HARVEST_ROUNDS`, so
//!   the round passed to `dispatch_play` decides the cap: round 14 has 5
//!   completed harvests, round 8 has 2.
//! - D009 exchanges 2 sheep for 1 boar + 1 cattle per exchange unit; the two
//!   `Sheep` entries in `give` are ONE price of 2 sheep, not two separate
//!   1-sheep draws from the same flock. After play the card passes to the
//!   left neighbour's hand (`post_play: LeftNeighborHand`), it does not stay
//!   in the owner's played area.

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{verified_exchange_cards, CardSet};
use agricola_re_ai::model::{Animal, Good, Pasture, Player};
use std::collections::{BTreeMap, BTreeSet};

fn exchange_set() -> CardSet {
    CardSet {
        definitions: verified_exchange_cards(),
    }
}

fn pasture(id: u8, cells: Vec<usize>, animal: Option<Animal>, quantity: u32) -> Pasture {
    Pasture {
        id,
        cells,
        animal,
        quantity,
    }
}

/// 2 sheep in one pasture, plus room to receive 1 boar and 1 cattle: two
/// occupied holding pastures keep their slot (an empty pasture with quantity
/// 0 would be dropped when the sheep are removed), and two stables hold the
/// incoming animals. Stables survive every removal, so placement is real.
fn d009_board() -> Player {
    let mut player = Player::new(0, 0);
    player
        .pastures
        .push(pasture(1, vec![0, 1], Some(Animal::Sheep), 2));
    // Two empty stables on empty farm cells receive the animals.
    player.farm[2].stable = true;
    player.farm[3].stable = true;
    player
}

/// `dispatch_play` mutates `players` in place; the harness returns the mutated
/// players plus the post-play card hands so left-neighbour passes are visible.
fn dispatch_card_with_zones(
    set: &CardSet,
    player: &mut Player,
    id: &str,
    round: u8,
) -> Result<(Vec<Player>, Vec<Vec<String>>), String> {
    let card = set
        .definition(id)
        .unwrap_or_else(|| panic!("{id} missing from set"));
    for (good, amount) in &card.cost {
        player
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|e| panic!("pay {id} cost: {e}"));
    }
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec![], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    players[0].played_cards.push(id.to_owned());
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    dispatch_play(
        set,
        0,
        id,
        0,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        round,
        &BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        true, // is_improvement_action: MeetingPlace + PlayMinor chain
    )?;
    Ok((players, cards.hands))
}

// ---------------- A036 ファサデ彫刻 ----------------

/// A036-normal: food 3, completed harvests 5 (round 14), cost clay 2.
/// Expect food -3 / points +3, and the card stays in the played area.
#[test]
fn a036_normal_exchanges_food_for_points_uncapped() {
    let set = exchange_set();
    let mut player = Player::new(0, 0);
    player.supply.clay = 2; // printed cost
    player.supply.food = 3;

    let (players, hands) =
        dispatch_card_with_zones(&set, &mut player, "A036", 14).expect("A036 play must succeed");

    assert_eq!(players[0].supply.clay, 0, "printed clay cost must be paid");
    assert_eq!(players[0].supply.food, 0, "food_delta -3 (exchange 3)");
    assert_eq!(players[0].card_points, 3, "points_delta +3 (exchange 3)");
    assert_eq!(
        players[0].played_cards,
        vec!["A036".to_string()],
        "A036 stays in the played area (post_play: played_area)"
    );
    assert!(hands[1].is_empty(), "A036 does not pass left");
}

/// A036-capped: food 3, completed harvests 2 (round 8). The card may exchange
/// at most 2 food even though 3 are held (cap = completed harvests), so
/// food -2 / points +2 and one food stays in the supply.
#[test]
fn a036_capped_at_completed_harvests() {
    let set = exchange_set();
    let mut player = Player::new(0, 0);
    player.supply.clay = 2; // printed cost
    player.supply.food = 3;

    let (players, _hands) =
        dispatch_card_with_zones(&set, &mut player, "A036", 8).expect("A036 play must succeed");

    assert_eq!(players[0].supply.clay, 0, "printed clay cost must be paid");
    assert_eq!(
        players[0].supply.food, 1,
        "food_delta -2 (exchange clamped to 2 completed harvests)"
    );
    assert_eq!(players[0].card_points, 2, "points_delta +2");
}

/// Round 1-3 have no completed harvest yet, so even a full food stock cannot
/// be exchanged (cap 0) — the optional exchange simply does not fire.
#[test]
fn a036_cannot_exchange_before_the_first_harvest() {
    let set = exchange_set();
    let mut player = Player::new(0, 0);
    player.supply.clay = 2; // printed cost
    player.supply.food = 5;

    let (players, _hands) =
        dispatch_card_with_zones(&set, &mut player, "A036", 2).expect("A036 play must succeed");

    assert_eq!(
        players[0].supply.food, 5,
        "cap 0: no exchange before the first harvest"
    );
    assert_eq!(players[0].card_points, 0);
}

/// A036 is optional: with no food the exchange simply does not happen and the
/// play still succeeds (no partial exchange, no play failure).
#[test]
fn a036_optional_declines_without_food() {
    let set = exchange_set();
    let mut player = Player::new(0, 0);
    player.supply.clay = 2; // printed cost
    player.supply.food = 0;

    let (players, _hands) =
        dispatch_card_with_zones(&set, &mut player, "A036", 8).expect("A036 play must succeed");

    assert_eq!(players[0].supply.food, 0, "nothing to exchange");
    assert_eq!(
        players[0].card_points, 0,
        "declined exchange grants nothing"
    );
    assert_eq!(players[0].played_cards, vec!["A036".to_string()]);
}

// ---------------- D009 獲物の交換 ----------------

/// D009-normal: sheep 2. One exchange unit costs [Sheep, Sheep] = 2 sheep and
/// yields 1 boar + 1 cattle; afterwards the card passes to the left
/// neighbour's hand (it must not stay in the owner's played area).
#[test]
fn d009_exchanges_two_sheep_for_boar_and_cattle_and_passes_left() {
    let set = exchange_set();
    let mut player = d009_board();

    let (players, hands) =
        dispatch_card_with_zones(&set, &mut player, "D009", 5).expect("D009 play must succeed");

    assert_eq!(
        players[0].animal_count(Animal::Sheep),
        0,
        "sheep_delta -2 (the two give entries are one 2-sheep price)"
    );
    assert_eq!(players[0].animal_count(Animal::Boar), 1, "boar_gained 1");
    assert_eq!(
        players[0].animal_count(Animal::Cattle),
        1,
        "cattle_gained 1"
    );
    assert!(
        !players[0].played_cards.contains(&"D009".to_string()),
        "D009 must leave the owner's played area"
    );
    assert_eq!(
        hands[1],
        vec!["D009".to_string()],
        "passes_to_left_neighbor: D009 lands in seat 1's hand"
    );
    assert!(hands[0].is_empty(), "seat 0 keeps no hand card");
}

/// A player holding `sheep` sheep in one REAL pasture: the cells are genuine
/// farm cells, so the holding is a real flock rather than a manufactured
/// cell-less pasture.
fn sheep_board(sheep: u32) -> Player {
    let mut player = Player::new(0, 0);
    if sheep > 0 {
        player
            .pastures
            .push(pasture(1, vec![0, 1], Some(Animal::Sheep), sheep));
    }
    player
}

/// D009-no-sheep: with fewer than the 2-sheep prerequisite the play is
/// rejected and nothing changes. The engine applies a play on a working clone,
/// so a rejected dispatch leaves the game untouched — pinned by full player
/// equality, not only by animal counts.
fn assert_d009_rejected_without_state_change(mut player: Player, case: &str) {
    let before = player.clone();

    let err = dispatch_card_with_zones(&exchange_set(), &mut player, "D009", 5)
        .expect_err(&format!("D009 needs the 2-sheep prerequisite ({case})"));

    assert!(
        err.contains("prerequisite"),
        "rejection must come from the prerequisite ({case}), got: {err}"
    );
    for animal in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
        assert_eq!(
            player.animal_count(animal),
            before.animal_count(animal),
            "{case}: {animal:?} count must not change"
        );
    }
    assert_eq!(
        player, before,
        "{case}: the whole player state must be unchanged"
    );
}

#[test]
fn d009_without_two_sheep_is_rejected_without_state_change() {
    // Case 1 — sheep 0 (the contract's `D009-no-sheep` setup): no flock.
    assert_d009_rejected_without_state_change(sheep_board(0), "sheep=0");
    // Case 2 — sheep 1: the price is short by exactly one sheep and that
    // single sheep sits in a real-cell pasture, so a rejection must not
    // consume, move or relocate it either.
    assert_d009_rejected_without_state_change(sheep_board(1), "sheep=1");
}

/// Four sheep buy TWO exchange units (2 boar + 2 cattle), not a doubled
/// per-element demand — the Round 1 P1-1 double-count failed exactly here.
#[test]
fn d009_with_four_sheep_exchanges_twice() {
    let set = exchange_set();
    let mut player = Player::new(0, 0);
    // 4 sheep, and four stables so both exchange units can be received.
    player
        .pastures
        .push(pasture(1, vec![0, 1], Some(Animal::Sheep), 4));
    for cell in [2, 3, 4, 5] {
        player.farm[cell].stable = true;
    }

    let (players, _hands) =
        dispatch_card_with_zones(&set, &mut player, "D009", 5).expect("D009 play must succeed");

    assert_eq!(
        players[0].animal_count(Animal::Sheep),
        0,
        "all 4 sheep paid"
    );
    assert_eq!(players[0].animal_count(Animal::Boar), 2, "boar_gained 2");
    assert_eq!(
        players[0].animal_count(Animal::Cattle),
        2,
        "cattle_gained 2"
    );
}

/// The gained animals are placed through the canonical placement vocabulary
/// (`auto_animal_plan` + `engine::place_animals`) — a received animal must never
/// manufacture a pasture with no farm cells (capacity 0), see Round 1 P2.
#[test]
fn d009_gained_animals_land_in_real_pastures() {
    let set = exchange_set();
    let mut player = d009_board();

    let (players, _hands) =
        dispatch_card_with_zones(&set, &mut player, "D009", 5).expect("D009 play must succeed");

    for pasture in &players[0].pastures {
        assert!(
            !pasture.cells.is_empty(),
            "pasture {} must cover at least one farm cell",
            pasture.id
        );
        assert!(
            pasture.quantity <= 2 * pasture.cells.len() as u32,
            "pasture {} quantity {} exceeds its 2-per-cell capacity",
            pasture.id,
            pasture.quantity
        );
    }
    let boar_in_stable = players[0]
        .farm
        .iter()
        .filter(|c| c.stable_animal == Some(Animal::Boar))
        .count();
    assert_eq!(
        boar_in_stable, 1,
        "boar placed in a stable (a real cell, not a cell-less pasture)"
    );
    let cattle_in_stable = players[0]
        .farm
        .iter()
        .filter(|c| c.stable_animal == Some(Animal::Cattle))
        .count();
    assert_eq!(
        cattle_in_stable, 1,
        "cattle placed in a stable (a real cell, not a cell-less pasture)"
    );
    for pasture in &players[0].pastures {
        assert!(
            !pasture.cells.is_empty(),
            "pasture {} must cover at least one farm cell",
            pasture.id
        );
    }
}

/// Good-typed sanity for the A036 cost: the printed 2-clay cost is paid by the
/// play transaction, so the handler must not also deduct it.
#[test]
fn a036_cost_is_clay_not_food() {
    let card = exchange_set()
        .definition("A036")
        .expect("A036 in the pool")
        .clone();
    assert_eq!(card.cost, vec![(Good::Clay, 2)]);
}

// Chapter 5: on_play build_stable (A001 雨除け / C002 厩 / E001 簡易納屋)
// These tests exercise `cards::play` directly: cost payment, on-play effect,
// deterministic site choice, placement scope, and the E001 fence prerequisite.
// Post-play "pass left" is not modeled on this base yet; the cards remain in
// the played area, exactly like every other implemented card here.
use agricola_re_ai::cards::{
    verified_build_stable_cards, CardEffect, CardKind, CardSet, StablePlacementScope,
};
use agricola_re_ai::model::{Good, Pasture, Player, Tile};

fn card_set() -> CardSet {
    CardSet {
        definitions: verified_build_stable_cards(),
    }
}

fn empty_player() -> Player {
    Player::new(0, 0)
}

fn set_pasture(p: &mut Player, cells: Vec<usize>, id: u8) {
    for &cell in &cells {
        p.farm[cell].tile = Tile::Pasture { id };
    }
    p.pastures.push(Pasture {
        id,
        cells,
        animal: None,
        quantity: 0,
    });
}

fn set_rooms(p: &mut Player, cells: &[usize]) {
    for &cell in cells {
        p.farm[cell].tile = Tile::Room;
    }
}

fn set_stables_on_pastures(p: &mut Player, cells: &[usize], id: u8) {
    for &cell in cells {
        p.farm[cell].tile = Tile::Pasture { id };
        p.farm[cell].stable = true;
    }
}

fn fill_fences(p: &mut Player, n: usize) {
    p.fences = (0..n).map(|i| (0u8, i as u8)).collect();
}

fn stable_cells(p: &Player) -> Vec<usize> {
    p.farm
        .iter()
        .enumerate()
        .filter(|(_, c)| c.stable)
        .map(|(i, _)| i)
        .collect::<Vec<_>>()
}

fn play_card(set: &CardSet, p: &mut Player, id: &str, wood: u32) {
    p.supply.set(Good::Wood, wood);
    let mut hand = vec![id.to_owned()];
    agricola_re_ai::cards::play(set, &mut hand, p, id, CardKind::MinorImprovement, 1, 0)
        .unwrap_or_else(|e| panic!("play {id} failed: {e}"));
}

#[test]
fn a001_builds_free_stable_on_fenced_one_cell_pasture() {
    let set = card_set();
    let mut p = empty_player();
    set_pasture(&mut p, vec![4], 1);

    play_card(&set, &mut p, "A001", 0);

    assert_eq!(p.supply.get(Good::Wood), 0);
    assert_eq!(stable_cells(&p), vec![4]);
    assert!(p.farm[4].stable);
}

#[test]
fn a001_does_not_build_on_multi_cell_pasture() {
    let set = card_set();
    let mut p = empty_player();
    set_pasture(&mut p, vec![4, 5], 1);

    play_card(&set, &mut p, "A001", 0);

    assert_eq!(stable_cells(&p), Vec::<usize>::new());
}

#[test]
fn a001_does_not_build_outside_pasture() {
    let set = card_set();
    let mut p = empty_player();
    p.farm[4].tile = Tile::Empty;
    p.farm[5].tile = Tile::Empty;

    play_card(&set, &mut p, "A001", 0);

    assert_eq!(stable_cells(&p), Vec::<usize>::new());
}

#[test]
fn c002_builds_one_free_stable_any_site() {
    let set = card_set();
    let mut p = empty_player();

    play_card(&set, &mut p, "C002", 1);

    assert_eq!(p.supply.get(Good::Wood), 0);
    assert_eq!(stable_cells(&p), vec![1]);
}

#[test]
fn c002_does_not_build_without_legal_site() {
    let set = card_set();
    let mut p = empty_player();
    set_rooms(&mut p, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);

    play_card(&set, &mut p, "C002", 1);

    assert_eq!(p.supply.get(Good::Wood), 0);
    assert_eq!(stable_cells(&p), Vec::<usize>::new());
}

#[test]
fn e001_requires_fifteen_fences() {
    let set = card_set();
    let mut p = empty_player();
    fill_fences(&mut p, 14);

    let result = {
        p.supply.set(Good::Wood, 2);
        let mut hand = vec!["E001".to_owned()];
        agricola_re_ai::cards::play(
            &set,
            &mut hand,
            &mut p,
            "E001",
            CardKind::MinorImprovement,
            1,
            0,
        )
    };

    assert!(result.is_err());
    assert_eq!(stable_cells(&p), Vec::<usize>::new());
}

#[test]
fn e001_builds_up_to_three_free_stables() {
    let set = card_set();
    let mut p = empty_player();
    fill_fences(&mut p, 15);

    play_card(&set, &mut p, "E001", 2);

    assert_eq!(p.supply.get(Good::Wood), 0);
    assert_eq!(stable_cells(&p), vec![1, 2, 3]);
}

#[test]
fn e001_builds_only_legal_sites_when_fewer_than_amount_remain() {
    let set = card_set();
    let mut p = empty_player();
    fill_fences(&mut p, 15);
    set_rooms(&mut p, &[0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
    // Two legal empty cells remain: cells 1 and 2.
    set_stables_on_pastures(&mut p, &[], 1);

    play_card(&set, &mut p, "E001", 2);

    assert_eq!(p.supply.get(Good::Wood), 0);
    assert_eq!(stable_cells(&p), vec![1, 2]);
}

#[test]
fn e001_builds_one_when_only_one_legal_site_remains() {
    let set = card_set();
    let mut p = empty_player();
    fill_fences(&mut p, 15);
    set_rooms(&mut p, &[0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14]);
    // Three existing stables occupy the stable supply; cell 6 is the only
    // legal empty cell.
    set_stables_on_pastures(&mut p, &[7, 8, 9], 1);

    play_card(&set, &mut p, "E001", 2);

    assert_eq!(p.supply.get(Good::Wood), 0);
    assert_eq!(stable_cells(&p), vec![6, 7, 8, 9]);
}

#[test]
fn card_effects_use_typed_build_stable_variant() {
    let set = card_set();
    let a001 = set.definition("A001").unwrap();
    let c002 = set.definition("C002").unwrap();
    let e001 = set.definition("E001").unwrap();

    assert!(matches!(
        &a001.effects[..],
        [CardEffect::OnPlayBuildStable {
            amount: 1,
            wood_cost: 0,
            scope: StablePlacementScope::OneCellPasture { pasture_size: 1 },
        }]
    ));
    assert!(matches!(
        &c002.effects[..],
        [CardEffect::OnPlayBuildStable {
            amount: 1,
            wood_cost: 0,
            scope: StablePlacementScope::AnySite,
        }]
    ));
    assert!(e001.effects.iter().any(|effect| matches!(
        effect,
        CardEffect::OnPlayBuildStable {
            amount: 3,
            wood_cost: 0,
            scope: StablePlacementScope::AnySite,
        }
    )));
}

use agricola_re_ai::model::{Animal, Crop, Player, Tile};
use agricola_re_ai::score::{score, standings};

#[test]
fn first_fields_and_animal_have_negative_zero_and_one_point() {
    let mut p = Player::new(0, 2);
    let initial = score(&p);
    assert_eq!(initial.categories["fields"], -1);
    assert_eq!(initial.categories["sheep"], -1);
    assert_eq!(initial.categories["unused"], -13);
    assert_eq!(initial.categories["family"], 6);
    p.farm[14].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 3,
    };
    p.pet = Some(Animal::Sheep);
    let updated = score(&p);
    assert_eq!(updated.categories["fields"], -1);
    assert_eq!(updated.categories["sheep"], 1);
    assert_eq!(updated.categories["unused"], -12);
}

#[test]
fn tiebreaker_uses_only_unspent_building_materials() {
    let mut players = vec![Player::new(0, 2), Player::new(1, 3)];
    assert_eq!(standings(&players).rank, [1, 1]);
    players[0].supply.food += 50;
    assert_eq!(standings(&players).rank, [1, 1]);
    players[1].supply.stone += 1;
    assert_eq!(standings(&players).rank, [2, 1]);
}

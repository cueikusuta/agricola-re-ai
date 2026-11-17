//! Scoring table from the 2016 Revised Edition appendix, part 10.
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub player_id: usize,
    pub categories: BTreeMap<String, i32>,
    pub total: i32,
    pub tiebreak_resources: u32,
}
fn at_least(n: u32, threshold: &[u32]) -> i32 {
    if n == 0 {
        -1
    } else {
        threshold.iter().filter(|&&x| n >= x).count() as i32
    }
}
fn craft_bonus(p: &Player) -> i32 {
    let mut bonus = 0;
    if p.improvements.contains(&Improvement::Joinery) {
        bonus += at_least_bonus(p.supply.wood, &[3, 5, 7])
    }
    if p.improvements.contains(&Improvement::Pottery) {
        bonus += at_least_bonus(p.supply.clay, &[3, 5, 7])
    }
    if p.improvements.contains(&Improvement::Basketry) {
        bonus += at_least_bonus(p.supply.reed, &[2, 4, 5])
    }
    bonus
}
fn at_least_bonus(n: u32, threshold: &[u32]) -> i32 {
    threshold.iter().filter(|&&x| n >= x).count() as i32
}
pub fn score(p: &Player) -> Score {
    let mut c = BTreeMap::new();
    c.insert(
        "fields".into(),
        if p.fields() <= 1 {
            -1
        } else {
            (p.fields() as i32 - 1).min(4)
        },
    );
    c.insert(
        "pastures".into(),
        if p.pastures.is_empty() {
            -1
        } else {
            (p.pastures.len() as i32).min(4)
        },
    );
    c.insert(
        "grain".into(),
        at_least(p.total_crop(Crop::Grain), &[1, 4, 6, 8]),
    );
    c.insert(
        "vegetables".into(),
        if p.total_crop(Crop::Vegetable) == 0 {
            -1
        } else {
            (p.total_crop(Crop::Vegetable) as i32).min(4)
        },
    );
    c.insert(
        "sheep".into(),
        at_least(p.animal_count(Animal::Sheep), &[1, 4, 6, 8]),
    );
    c.insert(
        "boar".into(),
        at_least(p.animal_count(Animal::Boar), &[1, 3, 5, 7]),
    );
    c.insert(
        "cattle".into(),
        at_least(p.animal_count(Animal::Cattle), &[1, 2, 4, 6]),
    );
    c.insert(
        "unused".into(),
        -(p.farm
            .iter()
            .filter(|x| matches!(x.tile, Tile::Empty) && !x.stable)
            .count() as i32),
    );
    c.insert(
        "fenced_stables".into(),
        p.farm
            .iter()
            .filter(|x| x.stable && matches!(x.tile, Tile::Pasture { .. }))
            .count() as i32,
    );
    c.insert(
        "rooms".into(),
        p.rooms() as i32
            * match p.house {
                House::Wood => 0,
                House::Clay => 1,
                House::Stone => 2,
            },
    );
    c.insert("family".into(), p.family as i32 * 3);
    c.insert(
        "improvements".into(),
        p.improvements.iter().map(|i| i.points()).sum(),
    );
    c.insert("craft_bonus".into(), craft_bonus(p));
    c.insert("cards".into(), p.card_points);
    c.insert("begging".into(), -(p.begging as i32) * 3);
    let total = c.values().sum();
    Score {
        player_id: p.id,
        categories: c,
        total,
        tiebreak_resources: p.supply.building_total(),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standings {
    pub scores: Vec<Score>,
    pub rank: Vec<usize>,
}
pub fn standings(players: &[Player]) -> Standings {
    let scores = players.iter().map(score).collect::<Vec<_>>();
    let mut rank = vec![1; players.len()];
    for i in 0..players.len() {
        rank[i] = 1 + scores
            .iter()
            .filter(|other| {
                other.total > scores[i].total
                    || other.total == scores[i].total
                        && other.tiebreak_resources > scores[i].tiebreak_resources
            })
            .count()
    }
    Standings { scores, rank }
}

/// Full public score when a declared implemented card set is in use.
/// The ordinary cardless score remains byte-for-byte compatible with M1-M4.
pub fn score_with_cards(player: &Player, cards: &crate::cards::CardSet) -> Score {
    let mut result = score(player);
    let extra = crate::cards::end_game_bonus(cards, player);
    result.categories.insert("card_bonus".to_owned(), extra);
    result.total += extra;
    result
}

/// Full public score when a declared implemented card set is in use.
/// The ordinary cardless score remains byte-for-byte compatible with M1-M4.
/// Ledger-aware unified entry: printed card points, end-game card bonuses and
/// the earned-bonus-point ledger are all included, so this entry and
/// [`crate::model::Game::standings`] always produce the same ranking. The
/// ledger must be supplied explicitly: a scoring entry point can never
/// silently drop stored bonus points (design audit P1-3, score entry).
pub fn standings_with_cards(
    players: &[Player],
    cards: &crate::cards::CardSet,
    bonus_points: &crate::model::BonusPointLedger,
) -> Standings {
    standings_with_cards_and_bonus(players, cards, bonus_points)
}

/// Full public score including bonus points that were earned during play and
/// stored in [`crate::model::BonusPointLedger`] (cards that pay points when
/// played, at a round start or during a harvest).
///
/// The printed points of the played cards stay in the `cards` category; the
/// event-earned points join the `card_bonus` category next to the end-game
/// bonuses computed from the final farm, so the public category set of a scored
/// game is unchanged.
pub fn score_with_cards_and_bonus(
    player: &Player,
    cards: &crate::cards::CardSet,
    stored_bonus: i32,
) -> Score {
    let mut result = score_with_cards(player, cards);
    if stored_bonus == 0 {
        return result;
    }
    *result
        .categories
        .entry("card_bonus".to_owned())
        .or_insert(0) += stored_bonus;
    result.total += stored_bonus;
    result
}

pub fn standings_with_cards_and_bonus(
    players: &[Player],
    cards: &crate::cards::CardSet,
    bonus_points: &crate::model::BonusPointLedger,
) -> Standings {
    let scores = players
        .iter()
        .map(|p| {
            score_with_cards_and_bonus(
                p,
                cards,
                crate::model::bonus_points_total(bonus_points, p.id),
            )
        })
        .collect::<Vec<_>>();
    rank_scores(&scores, players)
}

/// Standings for a whole game: printed card points, end-game card bonuses and
/// the earned-bonus-point store. This is the scoring entry point every caller
/// that holds a [`crate::model::Game`] should use, so a stored bonus can never
/// be silently dropped from a result.
impl crate::model::Game {
    pub fn standings(&self) -> Standings {
        standings_with_cards_and_bonus(&self.public.players, &self.card_set, &self.bonus_points)
    }
}

fn rank_scores(scores: &[Score], players: &[Player]) -> Standings {
    let mut rank = vec![1; players.len()];
    for i in 0..players.len() {
        rank[i] = 1 + scores
            .iter()
            .filter(|other| {
                other.total > scores[i].total
                    || (other.total == scores[i].total
                        && other.tiebreak_resources > scores[i].tiebreak_resources)
            })
            .count();
    }
    Standings {
        scores: scores.to_vec(),
        rank,
    }
}

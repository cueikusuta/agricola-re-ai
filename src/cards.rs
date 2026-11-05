//! Card-set metadata is supplied by callers; copyrighted card text is never bundled.
//! This is a deliberately small deterministic effect vocabulary, not a claim that
//! the full Agricola Revised deck has been implemented.
use crate::model::{Good, Player, Space};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Occupation,
    MinorImprovement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardEffect {
    OnPlayGain {
        good: Good,
        amount: u32,
    },
    AfterWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDefinition {
    /// A stable programmatic identifier, NOT the card's printed text.
    pub id: String,
    pub kind: CardKind,
    pub cost: Vec<(Good, u32)>,
    pub points: i32,
    pub effects: Vec<CardEffect>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSet {
    pub definitions: Vec<CardDefinition>,
}

impl CardSet {
    pub fn definition(&self, id: &str) -> Option<&CardDefinition> {
        self.definitions.iter().find(|card| card.id == id)
    }

    pub fn enabled(&self) -> bool {
        !self.definitions.is_empty()
    }

    pub fn validate(&self, players: usize) -> Result<(), String> {
        if !self.enabled() {
            return Ok(());
        }
        let mut ids = BTreeSet::new();
        for card in &self.definitions {
            if card.id.is_empty()
                || card.id.len() > 64
                || !card
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(card.id.clone())
            {
                return Err("invalid or duplicate card id".into());
            }
            if !(-10..=20).contains(&card.points)
                || card
                    .cost
                    .iter()
                    .any(|(_, quantity)| *quantity == 0 || *quantity > 30)
                || card.effects.iter().any(|effect| {
                    let amount = match effect {
                        CardEffect::OnPlayGain { amount, .. }
                        | CardEffect::AfterWorkerGain { amount, .. } => *amount,
                    };
                    amount == 0 || amount > 30
                })
            {
                return Err("unsupported card cost, points, or effect".into());
            }
        }
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let count = self.definitions.iter().filter(|c| c.kind == kind).count();
            if count < 7 * players {
                return Err(
                    "implemented card set needs seven cards of each kind per player".into(),
                );
            }
        }
        Ok(())
    }
}

/// Private: must never be serialized into a player's Observation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardState {
    pub hands: Vec<Vec<String>>,
    pub undealt: Vec<String>,
}

impl CardState {
    pub fn deal(set: &CardSet, seed: u64, players: usize) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xA5E1_9C32_76D4_50BF);
        let mut hands = vec![Vec::new(); players];
        let mut undealt = Vec::new();
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let mut pool = set
                .definitions
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| c.id.clone())
                .collect::<Vec<_>>();
            pool.shuffle(&mut rng);
            for hand in &mut hands {
                for _ in 0..7.min(pool.len()) {
                    hand.push(pool.pop().expect("checked length"));
                }
            }
            undealt.extend(pool);
        }
        Self { hands, undealt }
    }

    pub fn validate(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        if self.hands.len() != players.len() {
            return Err("card hand count mismatch".into());
        }
        let mut all = BTreeSet::new();
        for id in self
            .undealt
            .iter()
            .chain(self.hands.iter().flat_map(|hand| hand.iter()))
            .chain(players.iter().flat_map(|player| player.played_cards.iter()))
        {
            if set.definition(id).is_none() || !all.insert(id.as_str()) {
                return Err("card missing, duplicate or unknown".into());
            }
        }
        if all.len() != set.definitions.len() {
            return Err("card partition does not match card set".into());
        }
        for (hand, player) in self.hands.iter().zip(players) {
            let own = hand.iter().chain(player.played_cards.iter());
            for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
                let count = own
                    .clone()
                    .filter(|id| set.definition(id).is_some_and(|c| c.kind == kind))
                    .count();
                if count != if set.enabled() { 7 } else { 0 } {
                    return Err("player has incorrect original card allocation".into());
                }
            }
            let points: i32 = player
                .played_cards
                .iter()
                .filter_map(|id| set.definition(id))
                .map(|card| card.points)
                .sum();
            if points != player.card_points {
                return Err("card score mismatch".into());
            }
        }
        Ok(())
    }
}

/// Verify card affordability using aggregate costs (including occupation fees).
pub fn affordable(card: &CardDefinition, player: &Player, extra_food: u32) -> bool {
    let mut amounts = std::collections::BTreeMap::<Good, u32>::new();
    for &(good, amount) in &card.cost {
        *amounts.entry(good).or_default() += amount;
    }
    *amounts.entry(Good::Food).or_default() += extra_food;
    amounts
        .iter()
        .all(|(&good, &amount)| player.supply.get(good) >= amount)
}

fn safe_gain(player: &mut Player, good: Good, amount: u32) -> Result<(), String> {
    let next = player
        .supply
        .get(good)
        .checked_add(amount)
        .ok_or("card resource overflow")?;
    player.supply.set(good, next);
    Ok(())
}

/// Execute one prevalidated card play; the containing Game transaction provides rollback.
pub fn play(
    set: &CardSet,
    hand: &mut Vec<String>,
    player: &mut Player,
    id: &str,
    kind: CardKind,
    extra_food: u32,
) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    if card.kind != kind || !hand.iter().any(|x| x == id) {
        return Err("card is not in the player's hand or is the wrong kind".into());
    }
    if !affordable(card, player, extra_food) {
        return Err("cannot pay card costs".into());
    }
    for &(good, amount) in &card.cost {
        player.supply.pay(good, amount)?;
    }
    if extra_food > 0 {
        player.supply.pay(Good::Food, extra_food)?;
    }
    let next_points = player
        .card_points
        .checked_add(card.points)
        .ok_or("card points overflow")?;
    for effect in &card.effects {
        if let CardEffect::OnPlayGain { good, amount } = effect {
            safe_gain(player, *good, *amount)?;
        }
    }
    hand.retain(|x| x != id);
    player.played_cards.push(id.to_string());
    player.card_points = next_points;
    Ok(())
}

/// Deterministic, already-public effects on a recorded worker action.
pub fn after_worker(set: &CardSet, player: &mut Player, space: Space) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterWorkerGain {
                space: trigger,
                good,
                amount,
            } = effect
            {
                if *trigger == space {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/**
 * Verified 2016 base A/B occupations expressible entirely through the worker
 * completion hook. Only effect logic and opaque card IDs are stored here.
 * This subset alone is not a full playable 7+7-card deal.
 */
pub fn verified_worker_bonus_cards(players: usize) -> Vec<CardDefinition> {
    fn occupation(id: &str, effects: Vec<CardEffect>) -> CardDefinition {
        CardDefinition {
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects,
        }
    }
    fn gain(space: Space, good: Good, amount: u32) -> CardEffect {
        CardEffect::AfterWorkerGain {
            space,
            good,
            amount,
        }
    }
    let mut cards = vec![
        occupation(
            "A116",
            [Space::Forest, Space::Copse, Space::Grove]
                .into_iter()
                .map(|space| gain(space, Good::Wood, 1))
                .collect(),
        ),
        occupation(
            "A119",
            [
                Space::Farmland,
                Space::GrainSeeds,
                Space::GrainUtilization,
                Space::Cultivation,
            ]
            .into_iter()
            .map(|space| gain(space, Good::Wood, 1))
            .collect(),
        ),
    ];
    if players >= 4 {
        cards.push(occupation(
            "A155",
            vec![
                gain(Space::TravelingPlayers, Good::Wood, 1),
                gain(Space::TravelingPlayers, Good::Grain, 1),
            ],
        ));
    }
    if players >= 3 {
        cards.push(occupation("B142", vec![gain(Space::GrainSeeds, Good::Vegetable, 1)]));
    }
    cards
}

#[cfg(test)]
mod verified_worker_bonus_tests {
    use super::*;

    fn applied(card_id: &str, spaces: &[Space]) -> Player {
        let definitions = verified_worker_bonus_cards(4);
        let mut player = Player::new(0, 0);
        player.played_cards.push(card_id.to_string());
        for &space in spaces {
            after_worker(
                &CardSet {
                    definitions: definitions.clone(),
                },
                &mut player,
                space,
            )
            .unwrap();
        }
        player
    }

    #[test]
    fn base_deck_player_counts_exclude_multiplayer_only_cards() {
        let two = verified_worker_bonus_cards(2);
        let three = verified_worker_bonus_cards(3);
        let four = verified_worker_bonus_cards(4);
        assert_eq!(two.len(), 2);
        assert!(!two.iter().any(|c| c.id == "A155" || c.id == "B142"));
        assert_eq!(three.len(), 3);
        assert!(three.iter().any(|c| c.id == "B142"));
        assert!(!three.iter().any(|c| c.id == "A155"));
        assert_eq!(four.len(), 4);
    }

    #[test]
    fn a116_wood_from_wood_accumulations_only() {
        let player = applied(
            "A116",
            &[Space::Forest, Space::Copse, Space::Grove, Space::ClayPit],
        );
        assert_eq!(player.supply.wood, 3);
        assert_eq!(player.supply.clay, 0);
    }

    #[test]
    fn a119_wood_from_four_crop_related_actions_only() {
        let player = applied(
            "A119",
            &[
                Space::Farmland,
                Space::GrainSeeds,
                Space::GrainUtilization,
                Space::Cultivation,
                Space::Fishing,
            ],
        );
        assert_eq!(player.supply.wood, 4);
    }

    #[test]
    fn a155_grain_and_wood_from_traveling_players() {
        let player = applied("A155", &[Space::TravelingPlayers, Space::Fishing]);
        assert_eq!(player.supply.wood, 1);
        assert_eq!(player.supply.grain, 1);
        assert_eq!(player.supply.food, 0);
    }

    #[test]
    fn b142_vegetable_from_grain_seeds_only() {
        let player = applied("B142", &[Space::GrainSeeds, Space::Farmland]);
        assert_eq!(player.supply.vegetable, 1);
        assert_eq!(player.supply.grain, 0);
    }
}

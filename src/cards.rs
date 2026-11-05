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
    OnPlayGain { good: Good, amount: u32 },
    AfterWorkerGain { space: Space, good: Good, amount: u32 },
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
                || !card.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(card.id.clone())
            {
                return Err("invalid or duplicate card id".into());
            }
            if !(-10..=20).contains(&card.points)
                || card.cost.iter().any(|(_, quantity)| *quantity == 0 || *quantity > 30)
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
                return Err("implemented card set needs seven cards of each kind per player".into());
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
            let mut pool = set.definitions.iter()
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
        for id in self.undealt.iter()
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
                let count = own.clone().filter(|id| set.definition(id).is_some_and(|c| c.kind == kind)).count();
                if count != if set.enabled() { 7 } else { 0 } {
                    return Err("player has incorrect original card allocation".into());
                }
            }
            let points: i32 = player.played_cards.iter().filter_map(|id| set.definition(id))
                .map(|card| card.points).sum();
            if points != player.card_points {
                return Err("card score mismatch".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    pub fn fixture_set(players: usize) -> CardSet {
        let mut definitions = Vec::new();
        for index in 0..players * 7 {
            definitions.push(CardDefinition {
                id: format!("occ_{index}"), kind: CardKind::Occupation,
                cost: vec![], points: 0, effects: vec![CardEffect::AfterWorkerGain {
                    space: Space::GrainSeeds, good: Good::Food, amount: 1
                }],
            });
            definitions.push(CardDefinition {
                id: format!("minor_{index}"), kind: CardKind::MinorImprovement,
                cost: vec![], points: 1,
                effects: vec![CardEffect::OnPlayGain { good: Good::Wood, amount: 1 }],
            });
        }
        CardSet { definitions }
    }
}

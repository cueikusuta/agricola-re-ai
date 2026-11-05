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

/// A strictly implemented-only slice of the 2016 Revised base A/B cards.
///
/// This is an incomplete catalogue: until seven occupations and seven minor
/// improvements are available for *each* player, Game::with_cards deliberately
/// refuses a standard seven-card hand. We do not pad it with invented or
/// unimplemented cards.
pub fn implemented_base_ab_cards() -> CardSet {
    let gain = |space, good| CardEffect::AfterWorkerGain {
        space,
        good,
        amount: 1,
    };
    CardSet {
        definitions: vec![
            CardDefinition {
                id: "a-corn-scoop".into(),
                kind: CardKind::MinorImprovement,
                cost: vec![(Good::Wood, 1)],
                points: 0,
                effects: vec![gain(Space::GrainSeeds, Good::Grain)],
            },
            CardDefinition {
                id: "a-stone-tongs".into(),
                kind: CardKind::MinorImprovement,
                cost: vec![(Good::Wood, 1)],
                points: 0,
                effects: vec![
                    gain(Space::WesternQuarry, Good::Stone),
                    gain(Space::EasternQuarry, Good::Stone),
                ],
            },
            CardDefinition {
                id: "a-firewood-collector".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![
                    gain(Space::Farmland, Good::Wood),
                    gain(Space::GrainSeeds, Good::Wood),
                    gain(Space::GrainUtilization, Good::Wood),
                    gain(Space::Cultivation, Good::Wood),
                ],
            },
            CardDefinition {
                id: "b-greengrocer".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![gain(Space::GrainSeeds, Good::Vegetable)],
            },
        ],
    }
}

#[cfg(test)]
mod verified_ab_card_tests {
    use super::*;

    fn play_card(id: &str) -> (CardSet, Player) {
        let set = implemented_base_ab_cards();
        let card = set.definition(id).expect("verified card id");
        let mut player = Player::new(0, 3);
        player.supply.wood = 2;
        let mut hand = vec![id.to_string()];
        play(&set, &mut hand, &mut player, id, card.kind, 0).unwrap();
        assert!(hand.is_empty());
        assert_eq!(player.played_cards, vec![id]);
        (set, player)
    }

    #[test]
    fn a_corn_scoop_gives_one_extra_grain_on_grain_seeds() {
        let (set, mut player) = play_card("a-corn-scoop");
        assert_eq!(player.supply.wood, 1);
        let old = player.supply.grain;
        after_worker(&set, &mut player, Space::GrainSeeds).unwrap();
        assert_eq!(player.supply.grain, old + 1);
        after_worker(&set, &mut player, Space::Farmland).unwrap();
        assert_eq!(player.supply.grain, old + 1);
    }

    #[test]
    fn b_greengrocer_gives_vegetable_only_on_grain_seeds() {
        let (set, mut player) = play_card("b-greengrocer");
        after_worker(&set, &mut player, Space::GrainSeeds).unwrap();
        assert_eq!(player.supply.vegetable, 1);
        after_worker(&set, &mut player, Space::VegetableSeeds).unwrap();
        assert_eq!(player.supply.vegetable, 1);
    }

    #[test]
    fn a_firewood_collector_triggers_at_each_named_space_once() {
        let (set, mut player) = play_card("a-firewood-collector");
        let old = player.supply.wood;
        for space in [
            Space::Farmland,
            Space::GrainSeeds,
            Space::GrainUtilization,
            Space::Cultivation,
        ] {
            after_worker(&set, &mut player, space).unwrap();
        }
        assert_eq!(player.supply.wood, old + 4);
        after_worker(&set, &mut player, Space::Forest).unwrap();
        assert_eq!(player.supply.wood, old + 4);
    }

    #[test]
    fn a_stone_tongs_triggers_only_on_stone_accumulation() {
        let (set, mut player) = play_card("a-stone-tongs");
        for space in [Space::WesternQuarry, Space::EasternQuarry] {
            after_worker(&set, &mut player, space).unwrap();
        }
        assert_eq!(player.supply.stone, 2);
        after_worker(&set, &mut player, Space::ResourceMarket).unwrap();
        assert_eq!(player.supply.stone, 2);
    }

    #[test]
    fn partial_catalogue_is_never_dealt_as_complete_hands() {
        let set = implemented_base_ab_cards();
        assert_eq!(set.definitions.len(), 4);
        assert!(set.validate(2).is_err());
        assert!(set.validate(4).is_err());
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

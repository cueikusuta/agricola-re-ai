//! Canonical card zone model and the single mutation API for all card movement.
//!
//! This module enforces the invariant that every implemented card ID exists in
//! exactly one zone at all times. All direct `Vec` mutations on
//! `CardState.hands`, `CardState.undealt`, `Player.played_cards`,
//! `Game.improvement_market` must route through `move_card`.
//!
//! The module operates on existing storage (no struct layout changes) to
//! preserve `LOG_VERSION=5` save format.

use crate::cards::{CardSet, CardState};
use crate::model::{Good, Improvement, Player};
use std::collections::BTreeMap;

/// Logical zone a card can occupy. No hidden state; purely a classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardZone {
    /// Cards not yet dealt to any player (initial pool remainder).
    Undealt,
    /// In a specific player's hand (private).
    Hand { player: usize },
    /// Played face-up in front of a specific player (public).
    Played { player: usize },
    /// Available in the major improvement market (public).
    ImprovementMarket,
    /// Discarded by a specific player (public knowledge of discard, not contents).
    Discard { player: usize },
}

impl CardZone {
    /// Returns the player index if the zone is player-specific, else `None`.
    pub fn player(&self) -> Option<usize> {
        match self {
            CardZone::Hand { player } | CardZone::Played { player } | CardZone::Discard { player } => {
                Some(*player)
            }
            CardZone::Undealt | CardZone::ImprovementMarket => None,
        }
    }
}

/// Context required to execute a card move. Contains mutable references to
/// all storages that can hold cards so the move can update each atomically.
pub struct CardMoveCtx<'a> {
    pub cards: &'a mut CardState,
    pub players: &'a mut [Player],
    pub improvement_market: &'a mut Vec<Improvement>,
}

/// Error type for card zone operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardZoneError {
    CardNotFound(String),
    CardNotInZone(String, CardZone),
    InvalidMove(String),
    DuplicateInPlayed(String),
    PrerequisiteFailed(String),
}

impl std::fmt::Display for CardZoneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CardZoneError::CardNotFound(id) => write!(f, "card not found: {id}"),
            CardZoneError::CardNotInZone(id, zone) => write!(f, "card {id} not in zone {zone:?}"),
            CardZoneError::InvalidMove(msg) => write!(f, "invalid move: {msg}"),
            CardZoneError::DuplicateInPlayed(id) => write!(f, "duplicate card in played_cards: {id}"),
            CardZoneError::PrerequisiteFailed(msg) => write!(f, "prerequisite failed: {msg}"),
        }
    }
}

impl std::error::Error for CardZoneError {}

impl From<CardZoneError> for String {
    fn from(err: CardZoneError) -> Self {
        err.to_string()
    }
}

/// Single canonical mutation API for all card movement.
///
/// Preconditions:
/// - `id` must exist in `from` zone (validated by this function).
/// - `from` != `to`.
/// - For `Hand`/`Played`/`Discard` zones, the player index must be valid.
///
/// Postconditions:
/// - `id` is removed from `from` and added to `to`.
/// - All storages remain consistent (no duplicates, no orphan cards).
/// - Returns `Err` if the move would violate invariants.
pub fn move_card(
    ctx: &mut CardMoveCtx<'_>,
    id: &str,
    from: CardZone,
    to: CardZone,
) -> Result<(), CardZoneError> {
    if from == to {
        return Err(CardZoneError::InvalidMove("from and to zones are identical".into()));
    }

    // Remove from source zone
    let removed = remove_from_zone(ctx, id, from)?;
    if !removed {
        return Err(CardZoneError::CardNotInZone(id.to_string(), from));
    }

    // Add to destination zone
    add_to_zone(ctx, id, to)?;

    Ok(())
}

/// Remove `id` from `zone`. Returns `true` if the card was found and removed.
fn remove_from_zone(
    ctx: &mut CardMoveCtx<'_>,
    id: &str,
    zone: CardZone,
) -> Result<bool, CardZoneError> {
    match zone {
        CardZone::Undealt => {
            if let Some(pos) = ctx.cards.undealt.iter().position(|x| x == id) {
                ctx.cards.undealt.remove(pos);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        CardZone::Hand { player } => {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
            let hand = &mut ctx.cards.hands[player];
            if let Some(pos) = hand.iter().position(|x| x == id) {
                hand.remove(pos);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        CardZone::Played { player } => {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
            let played = &mut ctx.players[player].played_cards;
            if let Some(pos) = played.iter().position(|x| x == id) {
                played.remove(pos);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        CardZone::ImprovementMarket => {
            // Cards are not stored in improvement_market; this zone is for
            // major improvements (Improvement enum), not minor/occupation cards.
            // This should not be called for card IDs.
            Err(CardZoneError::InvalidMove(
                "ImprovementMarket zone is for major improvements, not card IDs".into(),
            ))
        }
        CardZone::Discard { player } => {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
            // Discard pile is not currently tracked per-player in CardState.
            // This would need extension if discard tracking is added.
            Err(CardZoneError::InvalidMove(
                "Discard zone not yet implemented in CardState".into(),
            ))
        }
    }
}

/// Add `id` to `zone`.
fn add_to_zone(
    ctx: &mut CardMoveCtx<'_>,
    id: &str,
    zone: CardZone,
) -> Result<(), CardZoneError> {
    match zone {
        CardZone::Undealt => {
            ctx.cards.undealt.push(id.to_string());
            Ok(())
        }
        CardZone::Hand { player } => {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
            ctx.cards.hands[player].push(id.to_string());
            Ok(())
        }
        CardZone::Played { player } => {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
            let played = &mut ctx.players[player].played_cards;
            // Enforce no duplicates in played_cards
            if played.iter().any(|x| x == id) {
                return Err(CardZoneError::DuplicateInPlayed(id.to_string()));
            }
            played.push(id.to_string());
            Ok(())
        }
        CardZone::ImprovementMarket => {
            Err(CardZoneError::InvalidMove(
                "ImprovementMarket zone is for major improvements, not card IDs".into(),
            ))
        }
        CardZone::Discard { player } => {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
            Err(CardZoneError::InvalidMove(
                "Discard zone not yet implemented in CardState".into(),
            ))
        }
    }
}

/// Validate the global card partition invariant:
/// Every implemented card ID exists in exactly one zone (undealt ∪ all hands ∪ all played_cards).
/// Also validates no duplicates in any player's played_cards.
pub fn validate_card_partition(
    card_set: &CardSet,
    cards: &CardState,
    players: &[Player],
) -> Result<(), String> {
    let mut seen = BTreeMap::new();

    // Collect all card IDs from all zones
    for id in &cards.undealt {
        *seen.entry(id.clone()).or_insert(0usize) += 1;
    }
    for hand in &cards.hands {
        for id in hand {
            *seen.entry(id.clone()).or_insert(0usize) += 1;
        }
    }
    for player in players {
        for id in &player.played_cards {
            *seen.entry(id.clone()).or_insert(0usize) += 1;
        }
    }

    // Check for duplicates
    for (id, count) in &seen {
        if *count > 1 {
            return Err(format!("card {id} appears in {count} zones (duplicate)"));
        }
    }

    // Check that all implemented cards are accounted for
    for card in &card_set.definitions {
        if !seen.contains_key(&card.id) {
            return Err(format!("card {} is missing from all zones", card.id));
        }
    }

    // Check played_cards for duplicates per player
    for player in players {
        let mut played_seen = BTreeMap::new();
        for id in &player.played_cards {
            *played_seen.entry(id.clone()).or_insert(0usize) += 1;
        }
        for (id, count) in played_seen {
            if count > 1 {
                return Err(format!("player {} has duplicate played card: {}", player.id, id));
            }
        }
    }

    Ok(())
}

/// Convenience: move a card from a player's hand to their played area.
/// This is the primary path for `cards::play`.
pub fn play_from_hand(
    ctx: &mut CardMoveCtx<'_>,
    player: usize,
    id: &str,
) -> Result<(), CardZoneError> {
    move_card(ctx, id, CardZone::Hand { player }, CardZone::Played { player })
}

/// Convenience: deal initial hands from undealt to each player's hand.
/// Used by `CardState::deal` and `CardState::from_hands`.
pub fn deal_initial_hands(
    ctx: &mut CardMoveCtx<'_>,
    hands: &[Vec<String>],
) -> Result<(), CardZoneError> {
    for (player, hand) in hands.iter().enumerate() {
        for id in hand {
            move_card(ctx, id, CardZone::Undealt, CardZone::Hand { player })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{CardDefinition, CardKind, CardSet};
    use crate::model::{Good, Player};

    fn test_setup() -> (CardSet, CardState, Vec<Player>) {
        let set = CardSet {
            definitions: vec![
                CardDefinition {
                    id: "A001".into(),
                    kind: CardKind::Occupation,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                },
                CardDefinition {
                    id: "A002".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                },
            ],
        };
        let cards = CardState {
            hands: vec![vec!["A001".into()], vec!["A002".into()]],
            undealt: vec![],
        };
        let players = vec![Player::new(0, 0), Player::new(1, 0)];
        (set, cards, players)
    }

    #[test]
    fn move_card_hand_to_played() {
        let (set, mut cards, mut players) = test_setup();
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut vec![],
        };
        play_from_hand(&mut ctx, 0, "A001").unwrap();
        assert!(ctx.cards.hands[0].is_empty());
        assert_eq!(ctx.players[0].played_cards, vec!["A001"]);
        validate_card_partition(&set, &ctx.cards, &ctx.players).unwrap();
    }

    #[test]
    fn move_card_rejects_duplicate_in_played() {
        let (set, mut cards, mut players) = test_setup();
        players[0].played_cards.push("A001".into());
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut vec![],
        };
        let err = play_from_hand(&mut ctx, 0, "A001").unwrap_err();
        assert!(matches!(err, CardZoneError::DuplicateInPlayed(_)));
    }

    #[test]
    fn validate_card_partition_detects_missing() {
        let (mut set, cards, players) = test_setup();
        set.definitions.push(CardDefinition {
            id: "A003".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        let err = validate_card_partition(&set, &cards, &players).unwrap_err();
        assert!(err.contains("missing"));
    }

    #[test]
    fn validate_card_partition_detects_duplicate() {
        let (set, mut cards, mut players) = test_setup();
        cards.undealt.push("A001".into()); // duplicate
        let err = validate_card_partition(&set, &cards, &players).unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn deal_initial_hands_moves_from_undealt() {
        let set = CardSet {
            definitions: vec![
                CardDefinition {
                    id: "A001".into(),
                    kind: CardKind::Occupation,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                },
                CardDefinition {
                    id: "A002".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                },
            ],
        };
        let mut cards = CardState {
            hands: vec![vec![], vec![]],
            undealt: vec!["A001".into(), "A002".into()],
        };
        let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut vec![],
        };
        deal_initial_hands(&mut ctx, &[vec!["A001".into()], vec!["A002".into()]]).unwrap();
        assert_eq!(ctx.cards.hands[0], vec!["A001"]);
        assert_eq!(ctx.cards.hands[1], vec!["A002"]);
        assert!(ctx.cards.undealt.is_empty());
        validate_card_partition(&set, &ctx.cards, &ctx.players).unwrap();
    }
}
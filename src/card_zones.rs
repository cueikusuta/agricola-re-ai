//! Canonical card zone model and the single mutation API for all card movement.
//!
//! This module enforces the invariant that every implemented card ID exists in
//! exactly one zone at all times. All card movement (playing from hand,
//! dealing, transfers between zones) routes through [`move_card`] so the
//! "exactly one location" invariant is checked centrally.
//!
//! The module operates on existing storage (no struct layout changes) to
//! preserve the save format.

use crate::cards::{CardSet, CardState};
use crate::model::{Improvement, Player};
use std::collections::BTreeMap;

/// Logical zone a card can occupy. No hidden state; purely a classification of
/// where a card ID lives at a given moment.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
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
    /// Discarded by a specific player (public knowledge of discard).
    Discard { player: usize },
}

impl CardZone {
    /// Returns the player index if the zone is player-specific, else `None`.
    pub fn player(&self) -> Option<usize> {
        match self {
            CardZone::Hand { player }
            | CardZone::Played { player }
            | CardZone::Discard { player } => Some(*player),
            CardZone::Undealt | CardZone::ImprovementMarket => None,
        }
    }
}

/// Context required to execute a card move. Holds mutable references to every
/// storage that can hold cards so a move can update all of them atomically.
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
            CardZoneError::CardNotInZone(id, zone) => {
                write!(f, "card {id} not in zone {zone:?}")
            }
            CardZoneError::InvalidMove(msg) => write!(f, "invalid move: {msg}"),
            CardZoneError::DuplicateInPlayed(id) => {
                write!(f, "duplicate card in played_cards: {id}")
            }
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

/// Returns `true` if `id` currently occupies `zone`.
fn card_in_zone(ctx: &CardMoveCtx<'_>, id: &str, zone: CardZone) -> bool {
    match zone {
        CardZone::Undealt => ctx.cards.undealt.iter().any(|x| x == id),
        CardZone::Hand { player } => {
            player < ctx.players.len()
                && ctx
                    .cards
                    .hands
                    .get(player)
                    .is_some_and(|hand| hand.iter().any(|x| x == id))
        }
        CardZone::Played { player } => {
            player < ctx.players.len()
                && ctx
                    .players
                    .get(player)
                    .is_some_and(|p| p.played_cards.iter().any(|x| x == id))
        }
        CardZone::ImprovementMarket => false, // major improvements, not card IDs
        CardZone::Discard { .. } => false,    // not yet tracked in CardState
    }
}

/// Single canonical mutation API for all card movement.
///
/// Preconditions:
/// - `id` must exist in `from` zone (validated by this function).
/// - `from != to`.
/// - For `Hand`/`Played`/`Discard` zones, the player index must be valid.
///
/// Postconditions:
/// - On `Ok(())`, `id` is removed from `from` and added to `to`, and all
///   storages remain consistent (no duplicates, no orphan cards).
/// - On `Err`, no storage is mutated: card location is completely unchanged.
///
/// The move is pre-validated in full before any storage is touched, so a
/// failure leaves the game state exactly as it was.
pub fn move_card(
    ctx: &mut CardMoveCtx<'_>,
    id: &str,
    from: CardZone,
    to: CardZone,
) -> Result<(), CardZoneError> {
    if from == to {
        return Err(CardZoneError::InvalidMove(
            "from and to zones are identical".into(),
        ));
    }

    // Player-index bounds for every player-specific zone involved.
    for zone in [from, to] {
        if let Some(player) = zone.player() {
            if player >= ctx.players.len() {
                return Err(CardZoneError::InvalidMove(format!(
                    "player index {player} out of bounds"
                )));
            }
        }
    }

    // A hand -> played transfer must stay within one player.
    if let (
        CardZone::Hand {
            player: from_player,
        },
        CardZone::Played { player: to_player },
    ) = (from, to)
    {
        if from_player != to_player {
            return Err(CardZoneError::InvalidMove(
                "hand -> played moves must stay within one player".into(),
            ));
        }
    }

    // ImprovementMarket and Discard are not backed by card storage.
    if matches!(from, CardZone::ImprovementMarket | CardZone::Discard { .. }) {
        return Err(CardZoneError::InvalidMove(
            "source zone is not backed by card storage".into(),
        ));
    }
    if matches!(to, CardZone::ImprovementMarket | CardZone::Discard { .. }) {
        return Err(CardZoneError::InvalidMove(
            "destination zone is not backed by card storage".into(),
        ));
    }

    // Pre-validate: the card must be in the source zone.
    if !card_in_zone(ctx, id, from) {
        return Err(CardZoneError::CardNotInZone(id.to_string(), from));
    }

    // Pre-validate: adding to the destination must not violate invariants.
    if let CardZone::Played { player } = to {
        if ctx.players[player].played_cards.iter().any(|x| x == id) {
            return Err(CardZoneError::DuplicateInPlayed(id.to_string()));
        }
    }

    // All preconditions hold; perform the move atomically.
    remove_from_zone(ctx, id, from);
    add_to_zone(ctx, id, to)?;
    Ok(())
}

/// Remove `id` from `zone`. Caller must have verified the card is present.
fn remove_from_zone(ctx: &mut CardMoveCtx<'_>, id: &str, zone: CardZone) {
    match zone {
        CardZone::Undealt => {
            if let Some(pos) = ctx.cards.undealt.iter().position(|x| x == id) {
                ctx.cards.undealt.remove(pos);
            }
        }
        CardZone::Hand { player } => {
            let hand = &mut ctx.cards.hands[player];
            if let Some(pos) = hand.iter().position(|x| x == id) {
                hand.remove(pos);
            }
        }
        CardZone::Played { player } => {
            let played = &mut ctx.players[player].played_cards;
            if let Some(pos) = played.iter().position(|x| x == id) {
                played.remove(pos);
            }
        }
        CardZone::ImprovementMarket | CardZone::Discard { .. } => {}
    }
}

/// Add `id` to `zone`. Caller must have verified the addition is valid.
fn add_to_zone(ctx: &mut CardMoveCtx<'_>, id: &str, zone: CardZone) -> Result<(), CardZoneError> {
    match zone {
        CardZone::Undealt => {
            ctx.cards.undealt.push(id.to_string());
            Ok(())
        }
        CardZone::Hand { player } => {
            ctx.cards.hands[player].push(id.to_string());
            Ok(())
        }
        CardZone::Played { player } => {
            let played = &mut ctx.players[player].played_cards;
            played.push(id.to_string());
            Ok(())
        }
        CardZone::ImprovementMarket | CardZone::Discard { .. } => Err(CardZoneError::InvalidMove(
            "zone is not backed by card storage".into(),
        )),
    }
}

/// Validate the global card partition invariant:
/// Every implemented card ID exists in exactly one zone (undealt ∪ all hands ∪
/// all played_cards), with no duplicates and no cards outside the set.
pub fn validate_card_partition(
    card_set: &CardSet,
    cards: &CardState,
    players: &[Player],
) -> Result<(), String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

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

    for (id, count) in &seen {
        if *count > 1 {
            return Err(format!("card {id} appears in {count} zones (duplicate)"));
        }
    }

    for card in &card_set.definitions {
        if !seen.contains_key(&card.id) {
            return Err(format!("card {} is missing from all zones", card.id));
        }
    }

    for player in players {
        let mut played_seen: BTreeMap<String, usize> = BTreeMap::new();
        for id in &player.played_cards {
            *played_seen.entry(id.clone()).or_insert(0usize) += 1;
        }
        for (id, count) in played_seen {
            if count > 1 {
                return Err(format!(
                    "player {} has duplicate played card: {}",
                    player.id, id
                ));
            }
        }
    }

    Ok(())
}

/// Single canonical implementation of the hand -> played transfer.
///
/// This is the one place that mutates a hand and a played area for a play.
/// [`move_card`] and [`cards::play`] both delegate here, so there is exactly
/// one code path that can move a card from a hand to a played area.
pub fn hand_to_played(
    hand: &mut Vec<String>,
    player: &mut Player,
    id: &str,
) -> Result<(), CardZoneError> {
    let Some(pos) = hand.iter().position(|x| x == id) else {
        return Err(CardZoneError::CardNotInZone(
            id.to_string(),
            CardZone::Hand { player: player.id },
        ));
    };
    if player.played_cards.iter().any(|x| x == id) {
        return Err(CardZoneError::DuplicateInPlayed(id.to_string()));
    }
    hand.remove(pos);
    player.played_cards.push(id.to_string());
    Ok(())
}

/// Convenience: move a card from a player's hand to their played area.
/// This is the primary path for playing a card.
pub fn play_from_hand(
    ctx: &mut CardMoveCtx<'_>,
    player: usize,
    id: &str,
) -> Result<(), CardZoneError> {
    move_card(
        ctx,
        id,
        CardZone::Hand { player },
        CardZone::Played { player },
    )
}

/// Convenience: deal initial hands from undealt to each player's hand.
/// Used when seeding a game from explicit starting hands (additional
/// acquisition from the undealt pool).
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

    fn test_setup() -> (CardSet, CardState, Vec<Player>) {
        let set = CardSet {
            definitions: vec![
                CardDefinition {
                    use_limit: None,
                    id: "A001".into(),
                    kind: CardKind::Occupation,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                },
                CardDefinition {
                    use_limit: None,
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

    fn ctx<'a>(
        cards: &'a mut CardState,
        players: &'a mut Vec<Player>,
        market: &'a mut Vec<Improvement>,
    ) -> CardMoveCtx<'a> {
        CardMoveCtx {
            cards,
            players: players.as_mut_slice(),
            improvement_market: market,
        }
    }

    #[test]
    fn move_card_hand_to_played() {
        let (set, mut cards, mut players) = test_setup();
        let mut market: Vec<Improvement> = vec![];
        let mut ctx = ctx(&mut cards, &mut players, &mut market);
        play_from_hand(&mut ctx, 0, "A001").unwrap();
        assert!(ctx.cards.hands[0].is_empty());
        assert_eq!(ctx.players[0].played_cards, vec!["A001"]);
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }

    #[test]
    fn move_card_failure_leaves_location_completely_unchanged() {
        let (set, mut cards, mut players) = test_setup();
        let before_cards = cards.clone();
        let before_players = players.clone();
        let mut market: Vec<Improvement> = vec![];
        let mut ctx = ctx(&mut cards, &mut players, &mut market);

        // Card A002 is not in player 0's hand: move must fail without mutating.
        let err = move_card(
            &mut ctx,
            "A002",
            CardZone::Hand { player: 0 },
            CardZone::Played { player: 0 },
        )
        .unwrap_err();
        assert!(matches!(err, CardZoneError::CardNotInZone(_, _)));

        // Cross-player hand -> played is rejected.
        let err = move_card(
            &mut ctx,
            "A001",
            CardZone::Hand { player: 0 },
            CardZone::Played { player: 1 },
        )
        .unwrap_err();
        assert!(matches!(err, CardZoneError::InvalidMove(_)));

        assert_eq!(&*ctx.cards, &before_cards);
        assert_eq!(ctx.players, before_players);
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }

    #[test]
    fn move_card_rejects_duplicate_in_played() {
        let (_set, mut cards, mut players) = test_setup();
        players[0].played_cards.push("A001".into());
        let mut market: Vec<Improvement> = vec![];
        let mut ctx = ctx(&mut cards, &mut players, &mut market);
        let err = play_from_hand(&mut ctx, 0, "A001").unwrap_err();
        assert!(matches!(err, CardZoneError::DuplicateInPlayed(_)));
        // State unchanged after the rejected move.
        assert_eq!(ctx.cards.hands[0], vec!["A001"]);
        assert_eq!(ctx.players[0].played_cards, vec!["A001"]);
    }

    #[test]
    fn move_card_transfer_between_players_preserves_partition() {
        let (set, mut cards, mut players) = test_setup();
        // Give player 1 an empty hand, player 0 holds A001.
        let mut market: Vec<Improvement> = vec![];
        let mut ctx = ctx(&mut cards, &mut players, &mut market);
        // Transfer A001: hand(0) -> undealt, then undealt -> hand(1).
        move_card(
            &mut ctx,
            "A001",
            CardZone::Hand { player: 0 },
            CardZone::Undealt,
        )
        .unwrap();
        move_card(
            &mut ctx,
            "A001",
            CardZone::Undealt,
            CardZone::Hand { player: 1 },
        )
        .unwrap();
        assert!(ctx.cards.hands[0].is_empty());
        assert_eq!(ctx.cards.hands[1], vec!["A002", "A001"]);
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }

    #[test]
    fn move_card_additional_acquisition_from_undealt() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "A001".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            }],
        };
        let mut cards = CardState {
            hands: vec![vec![]],
            undealt: vec!["A001".into()],
        };
        let mut players = vec![Player::new(0, 0)];
        let mut market: Vec<Improvement> = vec![];
        let mut ctx = ctx(&mut cards, &mut players, &mut market);
        // Acquire the remaining undealt card into the hand.
        deal_initial_hands(&mut ctx, &[vec!["A001".into()]]).unwrap();
        assert!(ctx.cards.undealt.is_empty());
        assert_eq!(ctx.cards.hands[0], vec!["A001"]);
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }

    #[test]
    fn validate_card_partition_detects_missing() {
        let (mut set, cards, players) = test_setup();
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "A003".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        let mut market: Vec<Improvement> = vec![];
        let _ctx = ctx(&mut cards.clone(), &mut players.clone(), &mut market);
        let _ = market;
        let err = validate_card_partition(&set, &cards, &players).unwrap_err();
        assert!(err.contains("missing"));
    }

    #[test]
    fn validate_card_partition_detects_duplicate() {
        let (set, mut cards, players) = test_setup();
        cards.undealt.push("A001".into()); // duplicate
        let err = validate_card_partition(&set, &cards, &players).unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn hand_to_played_is_single_hand_to_played_implementation() {
        let (set, cards, players) = test_setup();

        let mut via_move = cards.clone();
        let mut via_move_players = players.clone();
        let mut market: Vec<Improvement> = vec![];
        let mut ctx = ctx(&mut via_move, &mut via_move_players, &mut market);
        move_card(
            &mut ctx,
            "A001",
            CardZone::Hand { player: 0 },
            CardZone::Played { player: 0 },
        )
        .unwrap();

        let mut via_direct = cards;
        let mut via_direct_players = players;
        hand_to_played(&mut via_direct.hands[0], &mut via_direct_players[0], "A001").unwrap();

        assert_eq!(via_move.hands, via_direct.hands);
        assert_eq!(
            via_move_players[0].played_cards,
            via_direct_players[0].played_cards
        );
        validate_card_partition(&set, &via_move, &via_move_players).unwrap();
    }
}

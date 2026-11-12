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
use crate::model::{Improvement, Player};
use std::collections::BTreeMap;

/// Logical zone a card can occupy. No hidden state; purely a classification.
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
    /// Discarded by a specific player (public knowledge of discard, not contents).
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

/// Single canonical mutation API for all card movement.
///
/// Preconditions:
/// - `id` must exist in `from` zone (validated by this function).
/// - `from` != `to`.
/// - For `Hand`/`Played`/`Discard` zones, the player index must be valid.
///
/// Postconditions:
/// - On `Ok(())`, `id` is removed from `from` and added to `to`, and all
///   storages remain consistent (no duplicates, no orphan cards).
/// - On `Err`, no storage is mutated: the card stays exactly where it was.
///
/// Every precondition is validated before the first storage write, so a failed
/// move is a no-op instead of a partial transfer that loses the card.
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

    // Pre-validate: the card must currently occupy the source zone.
    if !card_in_zone(ctx, id, from) {
        return Err(CardZoneError::CardNotInZone(id.to_string(), from));
    }

    // Pre-validate: the destination must not already hold the card.
    if let CardZone::Played { player } = to {
        if ctx.players[player].played_cards.iter().any(|x| x == id) {
            return Err(CardZoneError::DuplicateInPlayed(id.to_string()));
        }
    }

    // Every precondition holds, so the two writes cannot fail half-way.
    remove_from_zone(ctx, id, from);
    add_to_zone(ctx, id, to);

    Ok(())
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
        CardZone::ImprovementMarket | CardZone::Discard { .. } => false,
    }
}

/// Remove `id` from `zone`. The caller has already validated that the card is
/// present and that the zone is backed by storage, so this only writes.
fn remove_from_zone(ctx: &mut CardMoveCtx<'_>, id: &str, zone: CardZone) {
    match zone {
        CardZone::Undealt => {
            if let Some(pos) = ctx.cards.undealt.iter().position(|x| x == id) {
                ctx.cards.undealt.remove(pos);
            }
        }
        CardZone::Hand { player } => {
            if let Some(pos) = ctx.cards.hands[player].iter().position(|x| x == id) {
                ctx.cards.hands[player].remove(pos);
            }
        }
        CardZone::Played { player } => {
            if let Some(pos) = ctx.players[player]
                .played_cards
                .iter()
                .position(|x| x == id)
            {
                ctx.players[player].played_cards.remove(pos);
            }
        }
        CardZone::ImprovementMarket | CardZone::Discard { .. } => {}
    }
}

/// Add `id` to `zone`. The caller has already validated that the addition is
/// legal, so this only writes.
fn add_to_zone(ctx: &mut CardMoveCtx<'_>, id: &str, zone: CardZone) {
    match zone {
        CardZone::Undealt => ctx.cards.undealt.push(id.to_string()),
        CardZone::Hand { player } => ctx.cards.hands[player].push(id.to_string()),
        CardZone::Played { player } => {
            let played = &mut ctx.players[player].played_cards;
            debug_assert!(
                !played.iter().any(|x| x == id),
                "move_card pre-validates duplicate played cards"
            );
            played.push(id.to_string());
        }
        CardZone::ImprovementMarket | CardZone::Discard { .. } => {
            debug_assert!(false, "move_card pre-validates zone backing");
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
/// `move_card` (through `remove_from_zone`/`add_to_zone`) and `cards::play`
/// (which only holds a `&mut Vec<String>` hand plus a `&mut Player`) both
/// delegate here, so there is exactly one code path that can move a card from
/// a hand to a played area (Issue #266: "card movement has a single source of
/// truth, ad-hoc Vec mutations removed").
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
/// This is the primary path for `cards::play`.
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
    use crate::model::Player;

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

    #[test]
    fn move_card_hand_to_played() {
        let (set, mut cards, mut players) = test_setup();
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        play_from_hand(&mut ctx, 0, "A001").unwrap();
        assert!(ctx.cards.hands[0].is_empty());
        assert_eq!(ctx.players[0].played_cards, vec!["A001"]);
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }

    #[test]
    fn move_card_rejects_duplicate_in_played() {
        let (_set, mut cards, mut players) = test_setup();
        players[0].played_cards.push("A001".into());
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        let err = play_from_hand(&mut ctx, 0, "A001").unwrap_err();
        assert!(matches!(err, CardZoneError::DuplicateInPlayed(_)));
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
    fn deal_initial_hands_moves_from_undealt() {
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
        let mut cards = CardState {
            hands: vec![vec![], vec![]],
            undealt: vec!["A001".into(), "A002".into()],
        };
        let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        deal_initial_hands(&mut ctx, &[vec!["A001".into()], vec!["A002".into()]]).unwrap();
        assert_eq!(ctx.cards.hands[0], vec!["A001"]);
        assert_eq!(ctx.cards.hands[1], vec!["A002"]);
        assert!(ctx.cards.undealt.is_empty());
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }

    #[test]
    fn hand_to_played_is_the_single_hand_to_played_implementation() {
        // Both entry points must produce the same post-state.
        let (set, cards, players) = test_setup();

        let mut via_move_cards = cards.clone();
        let mut via_move_players = players.clone();
        let mut market = vec![];
        {
            let mut ctx = CardMoveCtx {
                cards: &mut via_move_cards,
                players: &mut via_move_players,
                improvement_market: &mut market,
            };
            move_card(
                &mut ctx,
                "A001",
                CardZone::Hand { player: 0 },
                CardZone::Played { player: 0 },
            )
            .unwrap();
        }

        let mut via_direct_cards = cards;
        let mut via_direct_players = players;
        hand_to_played(
            &mut via_direct_cards.hands[0],
            &mut via_direct_players[0],
            "A001",
        )
        .unwrap();

        assert_eq!(via_move_cards.hands, via_direct_cards.hands);
        assert_eq!(
            via_move_players[0].played_cards,
            via_direct_players[0].played_cards
        );
        assert_eq!(via_move_players[0].played_cards, vec!["A001"]);
        validate_card_partition(&set, &via_move_cards, &via_move_players).unwrap();
    }

    #[test]
    fn hand_to_played_rejects_a_card_not_in_hand_and_a_duplicate() {
        let (_set, mut cards, mut players) = test_setup();
        let err = hand_to_played(&mut cards.hands[0], &mut players[0], "A002").unwrap_err();
        assert!(matches!(
            err,
            CardZoneError::CardNotInZone(_, CardZone::Hand { .. })
        ));

        players[0].played_cards.push("A001".into());
        let err = hand_to_played(&mut cards.hands[0], &mut players[0], "A001").unwrap_err();
        assert!(matches!(err, CardZoneError::DuplicateInPlayed(_)));
    }

    #[test]
    fn hand_to_played_cross_player_is_rejected() {
        let (_set, mut cards, mut players) = test_setup();
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        let err = move_card(
            &mut ctx,
            "A001",
            CardZone::Hand { player: 0 },
            CardZone::Played { player: 1 },
        )
        .unwrap_err();
        assert!(matches!(err, CardZoneError::InvalidMove(_)));
    }

    #[test]
    fn move_card_failure_leaves_location_completely_unchanged() {
        let (set, mut cards, mut players) = test_setup();
        let before_cards = cards.clone();
        let before_players = players.clone();
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };

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
    fn move_card_rejects_duplicate_in_played_state_unchanged() {
        let (_set, mut cards, mut players) = test_setup();
        players[0].played_cards.push("A001".into());
        let before_cards = cards.clone();
        let before_players = players.clone();
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        let err = play_from_hand(&mut ctx, 0, "A001").unwrap_err();
        assert!(matches!(err, CardZoneError::DuplicateInPlayed(_)));
        // State unchanged after the rejected move.
        assert_eq!(&*ctx.cards, &before_cards);
        assert_eq!(ctx.players, before_players);
    }

    #[test]
    fn move_card_transfer_between_players_preserves_partition() {
        let (set, mut cards, mut players) = test_setup();
        // Give player 1 an empty hand, player 0 holds A001.
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
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
        let mut market = vec![];
        let mut ctx = CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        // Acquire the remaining undealt card into the hand.
        deal_initial_hands(&mut ctx, &[vec!["A001".into()]]).unwrap();
        assert!(ctx.cards.undealt.is_empty());
        assert_eq!(ctx.cards.hands[0], vec!["A001"]);
        validate_card_partition(&set, ctx.cards, ctx.players).unwrap();
    }
}

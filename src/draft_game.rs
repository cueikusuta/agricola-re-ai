//! M11 (#104): bridge a completed draft into a real [`Game`].
//!
//! The draft module ([`crate::draft`]) is deliberately card-effect agnostic and
//! only moves opaque ids. This module is the single join point between a
//! completed [`DraftState`] and the card subsystem. It checks that the drafted
//! vocabulary exists in the game's [`CardSet`], that every seat finished with
//! exactly seven occupations and seven minor improvements, and then seeds the
//! engine through [`Game::with_cards_and_hands`] without re-dealing,
//! re-shuffling, or re-randomising anything.
//!
//! Production (officially reviewed) and test-fixture pools stay separated by
//! the caller: this module can only accept ids that the supplied `CardSet`
//! actually defines, so a fixture id can never silently become a verified card.

use crate::cards::{CardEffect, CardKind as DefinitionKind, CardSet};
use crate::draft::{CardKind, DraftCardInfo, DraftPool, DraftState};
use crate::model::{Config, Game, Good};

/// Public, non-secret catalog for a card set. It is safe to attach to a
/// [`crate::draft::DraftObservation`] because it mirrors the vocabulary the
/// engine already exposes through `Observation::card_definitions`.
pub fn card_catalog(set: &CardSet) -> Vec<DraftCardInfo> {
    set.definitions
        .iter()
        .map(|card| DraftCardInfo {
            id: card.id.clone(),
            kind: to_draft_kind(card.kind),
            points: card.points,
            cost: card
                .cost
                .iter()
                .map(|(good, quantity)| (good_name(*good), *quantity))
                .collect(),
            effect_tags: {
                let mut tags: Vec<String> = card
                    .effects
                    .iter()
                    .map(CardEffect::public_tag)
                    .filter(|tag| *tag != "prerequisite")
                    .map(str::to_owned)
                    .collect();
                tags.sort();
                tags.dedup();
                tags
            },
        })
        .collect()
}

/// Build a kind-separated draft pool from a card set. The pool contains exactly
/// the implemented, supplied cards, so an official pool can never include a
/// fixture id and a fixture pool can never include an official card.
pub fn pool_from_card_set(set: &CardSet) -> DraftPool {
    let ids = |kind: DefinitionKind| {
        set.definitions
            .iter()
            .filter(|card| card.kind == kind)
            .map(|card| card.id.clone())
            .collect::<Vec<_>>()
    };
    DraftPool::new(
        ids(DefinitionKind::Occupation),
        ids(DefinitionKind::MinorImprovement),
    )
}

/// Validate a completed draft against a card set and return the per-seat hands
/// in seat order. Rejects incomplete drafts, unknown cards, and seats that did
/// not finish with 7 + 7 cards; the engine re-checks duplicates and counts.
pub fn hands_from_completed_draft(
    state: &DraftState,
    set: &CardSet,
) -> Result<Vec<Vec<String>>, String> {
    if !state.is_complete() {
        return Err("draft is not complete".into());
    }
    if !set.enabled() {
        return Err("cannot start a card game from an empty card set".into());
    }
    // The whole drafted vocabulary must belong to the game's card set, not only
    // the cards that happened to be picked. Otherwise the undealt partition
    // would silently diverge from what was dealt.
    for id in state.kind_of.keys() {
        if set.definition(id).is_none() {
            return Err(format!("draft card {id} is not in the game card set"));
        }
    }
    let mut hands = Vec::with_capacity(state.player_count);
    for (seat, (occupations, minors)) in state.final_hands().into_iter().enumerate() {
        if occupations.len() != 7 || minors.len() != 7 {
            return Err(format!(
                "seat {seat} did not finish the draft with 7 + 7 cards"
            ));
        }
        for id in occupations.iter().chain(minors.iter()) {
            let definition = set
                .definition(id)
                .ok_or_else(|| format!("card {id} is not in the game card set"))?;
            if state.card_kind(id) != Some(to_draft_kind(definition.kind)) {
                return Err(format!("card {id} has a different kind in the card set"));
            }
        }
        hands.push(occupations.into_iter().chain(minors).collect());
    }
    Ok(hands)
}

/// Start a real game from a completed draft's final hands.
///
/// This is the only supported path from draft to play: it validates the draft,
/// matches the player count, and calls [`Game::with_cards_and_hands_with_start_player`], which
/// bypasses the normal random deal. A rejected call never constructs or mutates
/// a game, so an invalid start request cannot leave a partial state behind.
pub fn start_game_from_draft(
    state: &DraftState,
    set: CardSet,
    config: Config,
) -> Result<Game, String> {
    if state.player_count != config.players {
        return Err("draft player count does not match the game configuration".into());
    }
    state.validate_invariants()?;
    let hands = hands_from_completed_draft(state, &set)?;
    Game::with_cards_and_hands_with_start_player(config, set, hands, state.start_player)
}

fn to_draft_kind(kind: DefinitionKind) -> CardKind {
    match kind {
        DefinitionKind::Occupation => CardKind::Occupation,
        DefinitionKind::MinorImprovement => CardKind::MinorImprovement,
    }
}

fn good_name(good: Good) -> String {
    serde_json::to_value(good)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

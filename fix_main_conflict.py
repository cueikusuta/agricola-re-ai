#!/usr/bin/env python3
"""Fix the main conflict in card_events.rs"""

with open("src/card_events.rs", "r") as f:
    content = f.read()

# The main conflict - replace the conflict markers with proper structure
old = """    }
            }
        }
    }
    Ok(())
}
<<<<<<< HEAD
    // Phase 4b: After — improvement-play dependent triggers owned by EARLIER played
    // cards. A109 小商人 (`AfterImprovementPlayedGain`): each time the owner plays
    // an improvement from hand (MinorImprovement or MajorImprovement via
    // renovation-combined action), it grants food. The new card is already last in
    // `played_cards` (the caller ran the zone move), so the scan covers exactly
    // the cards played "before this one".
    if let Some(new_card) = card_set.definition(new_card_id) {
        if new_card.kind == crate::cards::CardKind::MinorImprovement {
            let played = ctx.player.played_cards.clone();
            let owned = played.len().saturating_sub(1); // exclude the new card
            for id in &played[..owned] {
                let Some(card) = card_set.definition(id) else {
                    continue;
                };
                for effect in &card.effects {
                    let CardEffect::AfterImprovementPlayedGain { good, amount } = effect else {
                        continue;
                    };
                    consume_budget(ctx.event_budget)?;
                    let limit = ctx
                        .card_use_limits
                        .get(id.as_str())
                        .copied()
                        .unwrap_or(u32::MAX);
                    check_card_use(ctx.card_uses, id, limit)?;
                    let p = &mut ctx.player;
                    let next = p
                        .supply
                        .get(*good)
                        .checked_add(*amount)
                        .ok_or("resource overflow")?;
                    p.supply.set(*good, next);
=======
/// Dispatch AfterAnyWorkerGain / AfterAnyWorkerOthersGain effects for ALL players.
/// These effects trigger when ANY player uses a space, so we scan all players'
/// cards (not just the actor's). This is called from `dispatch_after_worker`
/// after the actor's own effects have resolved.
#[allow(clippy::too_many_arguments)]
fn dispatch_after_worker_any_player(
    card_set: &CardSet,
    _actor: usize,
    space: Space,
    _cards: &mut crate::cards::CardState,
    players: &mut [Player],
    _improvement_market: &mut Vec<crate::model::Improvement>,
    _delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    _round: u8,
    _used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
    _pending: &mut Option<PendingDecision>,
    trigger: &PhaseTriggerData,
    _all_players: &[Player],
) -> Result<(), String> {
    // For each player (as card owner), check their played cards for the new variants
    // Use index-based loop to avoid borrow checker issues with mutable access
    for card_owner in 0..players.len() {
        let played_cards = players[card_owner].played_cards.clone();
        for card_id in played_cards {
            let Some(card) = card_set.definition(&card_id) else {
                continue;
            };
            for effect in &card.effects {"""

new = """    }
            }
        }
    }
    // Phase 4b: After — improvement-play dependent triggers owned by EARLIER played
    // cards. A109 小商人 (`AfterImprovementPlayedGain`): each time the owner plays
    // an improvement from hand (MinorImprovement or MajorImprovement via
    // renovation-combined action), it grants food. The new card is already last in
    // `played_cards` (the caller ran the zone move), so the scan covers exactly
    // the cards played "before this one".
    if let Some(new_card) = card_set.definition(new_card_id) {
        if new_card.kind == crate::cards::CardKind::MinorImprovement {
            let played = ctx.player.played_cards.clone();
            let owned = played.len().saturating_sub(1); // exclude the new card
            for id in &played[..owned] {
                let Some(card) = card_set.definition(id) else {
                    continue;
                };
                for effect in &card.effects {
                    let CardEffect::AfterImprovementPlayedGain { good, amount } = effect else {
                        continue;
                    };
                    consume_budget(ctx.event_budget)?;
                    let limit = ctx
                        .card_use_limits
                        .get(id.as_str())
                        .copied()
                        .unwrap_or(u32::MAX);
                    check_card_use(ctx.card_uses, id, limit)?;
                    let p = &mut ctx.player;
                    let next = p
                        .supply
                        .get(*good)
                        .checked_add(*amount)
                        .ok_or("resource overflow")?;
                    p.supply.set(*good, next);
                }
            }
        }
    }
    Ok(())
}

/// Dispatch AfterAnyWorkerGain / AfterAnyWorkerOthersGain effects for ALL players.
/// These effects trigger when ANY player uses a space, so we scan all players'
/// cards (not just the actor's). This is called from `dispatch_after_worker`
/// after the actor's own effects have resolved.
#[allow(clippy::too_many_arguments)]
fn dispatch_after_worker_any_player(
    card_set: &CardSet,
    _actor: usize,
    space: Space,
    _cards: &mut crate::cards::CardState,
    players: &mut [Player],
    _improvement_market: &mut Vec<crate::model::Improvement>,
    _delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    _round: u8,
    _used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
    _pending: &mut Option<PendingDecision>,
    trigger: &PhaseTriggerData,
    _all_players: &[Player],
) -> Result<(), String> {
    // For each player (as card owner), check their played cards for the new variants
    // Use index-based loop to avoid borrow checker issues with mutable access
    for card_owner in 0..players.len() {
        let played_cards = players[card_owner].played_cards.clone();
        for card_id in played_cards {
            let Some(card) = card_set.definition(&card_id) else {
                continue;
            };
            for effect in &card.effects {"""

content = content.replace(old, new)

# Also fix the end of the conflict - there's a second part with <<<<<<< HEAD and =======
old2 = """                }
            }
        }
    }
<<<<<<< HEAD

=======
>>>>>>> origin/main
    Ok(())
}"""

new2 = """                }
            }
        }
    }
    Ok(())
}

/// Dispatch AfterWorker phase for a completed worker action."""

content = content.replace(old2, new2)

with open("src/card_events.rs", "w") as f:
    f.write(content)

print("Main conflict fixed!")
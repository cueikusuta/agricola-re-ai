import re

with open("src/card_events.rs", "r") as f:
    content = f.read()

# Fix 1: effect_phase - add AfterOccupationGain and keep AfterImprovementPlayedGain
old1 = """        // Fired only by the play dispatcher's dedicated after-play scan.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => TriggerPhase::After, // fired by the dedicated after-play scan
<<<<<<< HEAD
        // Improvement played from hand — After phase (fired by dispatch_play's after-play scan)
        CardEffect::AfterImprovementPlayedGain { .. } => TriggerPhase::After,
=======
        CardEffect::AfterOccupationGain { .. } => TriggerPhase::After, // fired by the dedicated after-play scan
>>>>>>> origin/main"""

new1 = """        // Fired only by the play dispatcher's dedicated after-play scan.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => TriggerPhase::After, // fired by the dedicated after-play scan
        CardEffect::AfterOccupationGain { .. } => TriggerPhase::After, // fired by the dedicated after-play scan
        // Improvement played from hand — After phase (fired by dispatch_play's after-play scan)
        CardEffect::AfterImprovementPlayedGain { .. } => TriggerPhase::After,"""

content = content.replace(old1, new1)
print("Fix 1 done:", old1 in content)

# Fix 2: resolve_phase_scan match - add both AfterImprovementPlayedGain and AfterOccupationGain
old2 = """                // Never fires via the generic phase scan (see
                // effect_condition_holds); dispatch_play's after-play scan owns it.
                CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,
<<<<<<< HEAD
                // Never fires via the generic phase scan; dispatch_play's after-play
                // scan for improvements owns it.
                CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,
=======
                CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,
>>>>>>> origin/main"""

new2 = """                // Never fires via the generic phase scan (see
                // effect_condition_holds); dispatch_play's after-play scan owns it.
                CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,
                // Never fires via the generic phase scan; dispatch_play's after-play
                // scan for improvements owns it.
                CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,
                CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,"""

content = content.replace(old2, new2)
print("Fix 2 done:", old2 in content)

# Fix 3: execute_chosen_triggers match - same as fix 2
old3 = """            // Never fires via the generic phase scan (see
            // effect_condition_holds); dispatch_play's after-play scan owns it.
            CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,
<<<<<<< HEAD
            // Never fires via the generic phase scan; dispatch_play's after-play
            // scan for improvements owns it.
            CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,
=======
            CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,
>>>>>>> origin/main"""

new3 = """            // Never fires via the generic phase scan (see
            // effect_condition_holds); dispatch_play's after-play scan owns it.
            CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,
            // Never fires via the generic phase scan; dispatch_play's after-play
            // scan for improvements owns it.
            CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,
            CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,"""

content = content.replace(old3, new3)
print("Fix 3 done:", old3 in content)

# Fix 4: effect_condition_holds - add both AfterImprovementPlayedGain and AfterOccupationGain
old4 = """        // C116 調度品職員 fires only from dispatch_play's dedicated after-play
        // scan, never from the generic phase scan.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => false,
<<<<<<< HEAD
        // A109 小商人 fires only from dispatch_play's dedicated after-play scan
        // for improvements.
        CardEffect::AfterImprovementPlayedGain { .. } => false,
=======
        // C068 書棚 fires only from dispatch_play's dedicated after-play
        // scan, never from the generic phase scan.
        CardEffect::AfterOccupationGain { .. } => false,
>>>>>>> origin/main"""

new4 = """        // C116 調度品職員 fires only from dispatch_play's dedicated after-play
        // scan, never from the generic phase scan.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => false,
        // A109 小商人 fires only from dispatch_play's dedicated after-play scan
        // for improvements.
        CardEffect::AfterImprovementPlayedGain { .. } => false,
        // C068 書棚 fires only from dispatch_play's dedicated after-play
        // scan, never from the generic phase scan.
        CardEffect::AfterOccupationGain { .. } => false,"""

content = content.replace(old4, new4)
print("Fix 4 done:", old4 in content)

# Fix 5: Main conflict - Phase 4b vs dispatch_after_worker_any_player
old5 = """    }
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
        let played_cards = players[card_owner].played_cards.clone().
        for card_id in played_cards {
            let Some(card) = card_set.definition(&card_id) else {
                continue;
            };
            for effect in &card.effects {
                // Only handle the two new variants
                let (effect_space, good, amount) = match effect {
                    CardEffect::AfterAnyWorkerGain {
                        space,
                        good,
                        amount,
                    } => (*space, *good, *amount),
                    CardEffect::AfterAnyWorkerOthersGain {
                        space,
                        good,
                        amount,
                    } => (*space, *good, *amount),
                    _ => continue,
                };

                // Check condition: space matches and event is WorkerPlaced
                if effect_space != space || trigger.event_kind != EventKind::WorkerPlaced {
                    continue;
                }

                // Consume event budget
                consume_budget(event_budget)?;

                // Check card use limit
                let limit = card_use_limits.get(&card_id).copied().unwrap_or(u32::MAX);
                check_card_use(card_uses, &card_id, limit)?;

                // Apply effect based on variant
                match effect {
                    CardEffect::AfterAnyWorkerGain { .. } => {
                        // Grant to card owner
                        let p = &mut players[card_owner];
                        let next = p
                            .supply
                            .get(good)
                            .checked_add(amount)
                            .ok_or("resource overflow")?;
                        p.supply.set(good, next);
                    }
                    CardEffect::AfterAnyWorkerOthersGain { .. } => {
                        // Grant to all players EXCEPT card owner
                        handle_after_any_worker_others_gain(
                            players,
                            card_owner,
                            &effect_space,
                            &good,
                            amount,
                            trigger,
                        )?;
                    }
                    _ => unreachable!(),
>>>>>>> origin/main
                }
            }
        }
    }
<<<<<<< HEAD

=======
>>>>>>> origin/main
    Ok(())
}"""

new5 = """    }
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
            let played = ctx.player.played_cards.clone().
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
                    check_card_use(ctx.card_uses, id, limit)?.
                    let p = &mut ctx.player;
                    let next = p
                        .supply
                        .get(*good)
                        .checked_add(*amount)
                        .ok_or("resource overflow")?.
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
        let played_cards = players[card_owner].played_cards.clone().
        for card_id in played_cards {
            let Some(card) = card_set.definition(&card_id) else {
                continue;
            };
            for effect in &card.effects {
                // Only handle the two new variants
                let (effect_space, good, amount) = match effect {
                    CardEffect::AfterAnyWorkerGain {
                        space,
                        good,
                        amount,
                    } => (*space, *good, *amount),
                    CardEffect::AfterAnyWorkerOthersGain {
                        space,
                        good,
                        amount,
                    } => (*space, *good, *amount),
                    _ => continue,
                };

                // Check condition: space matches and event is WorkerPlaced
                if effect_space != space || trigger.event_kind != EventKind::WorkerPlaced {
                    continue;
                }

                // Consume event budget
                consume_budget(event_budget)?.

                // Check card use limit
                let limit = card_use_limits.get(&card_id).copied().unwrap_or(u32::MAX).
                check_card_use(card_uses, &card_id, limit)?.

                // Apply effect based on variant
                match effect {
                    CardEffect::AfterAnyWorkerGain { .. } => {
                        // Grant to card owner
                        let p = &mut players[card_owner].
                        let next = p
                            .supply
                            .get(good)
                            .checked_add(amount)
                            .ok_or("resource overflow")?.
                        p.supply.set(good, next);
                    }
                    CardEffect::AfterAnyWorkerOthersGain { .. } => {
                        // Grant to all players EXCEPT card owner
                        handle_after_any_worker_others_gain(
                            players,
                            card_owner,
                            &effect_space,
                            &good,
                            amount,
                            trigger,
                        )?.
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(())
}

/// Dispatch AfterWorker phase for a completed worker action."""

content = content.replace(old5, new5)
print("Fix 5 done:", old5 in content)

# Fix 6: Second conflict - extra blank lines
old6 = """                    _ => unreachable!(),
>>>>>>> origin/main
                }
            }
        }
    }
<<<<<<< HEAD

=======
>>>>>>> origin/main
    Ok(())
}"""

new6 = """                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(())
}"""

content = content.replace(old6, new6)
print("Fix 6 done:", old6 in content)

# Write the fixed file
with open("src/card_events.rs", "w") as f:
    f.write(content)

print("All fixes applied!")
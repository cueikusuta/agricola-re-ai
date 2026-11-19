#!/usr/bin/env python3
"""Comprehensive fix for all merge conflicts"""

import re

def fix_card_events():
    with open("src/card_events.rs", "r") as f:
        content = f.read()
    
    # Fix 1: effect_phase
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
    
    # Fix 2: resolve_phase_scan match
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
    
    # Fix 3: execute_chosen_triggers match
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
    
    # Fix 4: effect_condition_holds
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
        let played_cards = players[card_owner].played_cards.clone();
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
                }
            }
        }
    }
    Ok(())
}

/// Dispatch AfterWorker phase for a completed worker action."""
    
    content = content.replace(old5, new5)
    
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
    
    with open("src/card_events.rs", "w") as f:
        f.write(content)
    
    print("card_events.rs fixed!")

def fix_cards():
    with open("src/cards.rs", "r") as f:
        content = f.read()
    
    # Fix 1: CardEffect enum - combine both variants
    old1 = """<<<<<<< HEAD
    /// A109 小商人 (Small Trader): when an improvement is played from hand
    /// via a Major/Minor Improvement action (including renovation-combined),
    /// gain `amount` of `good`. Fired from the play dispatcher's after-play
    /// scan for improvements; the legacy `cards::play` helper does not re-scan
    /// earlier cards and therefore never fires it.
    AfterImprovementPlayedGain {
        good: Good,
        amount: u32,
    },
=======
    /// C068 書棚 (Bookcase): each time the owner plays an occupation AFTER
    /// this card, gain `amount` of `good`. Fixed amount per occupation played,
    /// independent of the occupation's cost. Fired from the play dispatcher's
    /// after-play scan; the legacy `cards::play` helper does not re-scan earlier
    /// cards and therefore never fires it.
    AfterOccupationGain {
        good: Good,
        amount: u32,
    },
    /// C141 羊の世話人 / A050 ミルクがめ (self half): each time ANY player uses
    /// the specified space, the card owner gains `amount` of `good`. Unlike
    /// `AfterWorkerGain` which only fires when the card owner themselves uses
    /// the space, this variant fires for any player's use (including the owner).
    AfterAnyWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// A050 ミルクがめ (others half): each time ANY player uses the specified
    /// space, all players EXCEPT the card owner gain `amount` of `good`.
    /// This is a separate variant to keep the effect vocabulary focused and
    /// avoid widening `AfterAnyWorkerGain` with an `others_amount` field.
    AfterAnyWorkerOthersGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// D061 わらのたわら (Bale of Straw): at the start of each harvest, if the
    /// card owner has at least `threshold` fields sown with `crop`, gain
    /// `amount` of `good`. The condition is evaluated in the handler because
    /// it requires inspecting the player's farm state.
    HarvestStartGainIfFields {
        crop: Crop,
        threshold: u8,
        good: Good,
        amount: u32,
    },
    /// C083 若牛 (Early Cattle): on play, gain `amount` of `animal` (cattle).
    /// The player must provide an AnimalPlan for placement.
    OnPlayGainAnimal {
        animal: Animal,
        amount: u32,
    },
    /// C147 牛の飼育士 (Cowherd): each time the owner uses `space` (CattleMarket),
    /// gain `amount` of `animal` (cattle). The player must provide an AnimalPlan
    /// for placement.
    AfterWorkerGainAnimal {
        space: Space,
        animal: Animal,
        amount: u32,
    },
>>>>>>> origin/main"""
    
    new1 = """/// C068 書棚 (Bookcase): each time the owner plays an occupation AFTER
    /// this card, gain `amount` of `good`. Fixed amount per occupation played,
    /// independent of the occupation's cost. Fired from the play dispatcher's
    /// after-play scan; the legacy `cards::play` helper does not re-scan earlier
    /// cards and therefore never fires it.
    AfterOccupationGain {
        good: Good,
        amount: u32,
    },
    /// C141 羊の世話人 / A050 ミルクがめ (self half): each time ANY player uses
    /// the specified space, the card owner gains `amount` of `good`. Unlike
    /// `AfterWorkerGain` which only fires when the card owner themselves uses
    /// the space, this variant fires for any player's use (including the owner).
    AfterAnyWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// A050 ミルクがめ (others half): each time ANY player uses the specified
    /// space, all players EXCEPT the card owner gain `amount` of `good`.
    /// This is a separate variant to keep the effect vocabulary focused and
    /// avoid widening `AfterAnyWorkerGain` with an `others_amount` field.
    AfterAnyWorkerOthersGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// D061 わらのたわら (Bale of Straw): at the start of each harvest, if the
    /// card owner has at least `threshold` fields sown with `crop`, gain
    /// `amount` of `good`. The condition is evaluated in the handler because
    /// it requires inspecting the player's farm state.
    HarvestStartGainIfFields {
        crop: Crop,
        threshold: u8,
        good: Good,
        amount: u32,
    },
    /// C083 若牛 (Early Cattle): on play, gain `amount` of `animal` (cattle).
    /// The player must provide an AnimalPlan for placement.
    OnPlayGainAnimal {
        animal: Animal,
        amount: u32,
    },
    /// C147 牛の飼育士 (Cowherd): each time the owner uses `space` (CattleMarket),
    /// gain `amount` of `animal` (cattle). The player must provide an AnimalPlan
    /// for placement.
    AfterWorkerGainAnimal {
        space: Space,
        animal: Animal,
        amount: u32,
    },
    /// A109 小商人 (Small Trader): when an improvement is played from hand
    /// via a Major/Minor Improvement action (including renovation-combined),
    /// gain `amount` of `good`. Fired from the play dispatcher's after-play
    /// scan for improvements; the legacy `cards::play` helper does not re-scan
    /// earlier cards and therefore never fires it.
    AfterImprovementPlayedGain {
        good: Good,
        amount: u32,
    },"""
    
    content = content.replace(old1, new1)
    
    # Fix 2: public_tag match
    old2 = """<<<<<<< HEAD
            CardEffect::AfterImprovementPlayedGain { .. } => \"after_improvement_played_gain\",
=======
            CardEffect::OnPlayGainAnimal { .. } => \"on_play_gain_animal\",
            CardEffect::AfterWorkerGainAnimal { .. } => \"after_worker_gain_animal\",
            CardEffect::AfterAnyWorkerGain { .. } => \"after_any_worker_gain\",
            CardEffect::AfterAnyWorkerOthersGain { .. } => \"after_any_worker_others_gain\",
            CardEffect::HarvestStartGainIfFields { .. } => \"harvest_start_gain_if_fields\",
>>>>>>> origin/main"""
    
    new2 = """            CardEffect::OnPlayGainAnimal { .. } => \"on_play_gain_animal\",
            CardEffect::AfterWorkerGainAnimal { .. } => \"after_worker_gain_animal\",
            CardEffect::AfterAnyWorkerGain { .. } => \"after_any_worker_gain\",
            CardEffect::AfterAnyWorkerOthersGain { .. } => \"after_any_worker_others_gain\",
            CardEffect::HarvestStartGainIfFields { .. } => \"harvest_start_gain_if_fields\",
            CardEffect::AfterImprovementPlayedGain { .. } => \"after_improvement_played_gain\","""
    
    content = content.replace(old2, new2)
    
    # Fix 3: validate match for amount
    old3 = """<<<<<<< HEAD
                        | CardEffect::AfterImprovementPlayedGain { amount, .. } => *amount,
=======
                        | CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. } => *amount,
>>>>>>> origin/main"""
    
    new3 = """| CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. }
                        | CardEffect::AfterImprovementPlayedGain { amount, .. } => *amount,"""
    
    content = content.replace(old3, new3)
    
    with open("src/cards.rs", "w") as f:
        f.write(content)
    
    print("cards.rs fixed!")

def fix_policy():
    with open("src/policy.rs", "r") as f:
        content = f.read()
    
    old = """<<<<<<< HEAD
                                CardEffect::AfterImprovementPlayedGain { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                            };
=======
                                // New variants for market followup cards
                                CardEffect::AfterAnyWorkerGain { good, amount, .. }
                                | CardEffect::AfterAnyWorkerOthersGain { good, amount, .. } => {
                                    // Fires on any player's use of the space; approximate with per-round expectation
                                    card_good_value(*good) * i64::from(*amount) * horizon / 4
                                }
                                CardEffect::HarvestStartGainIfFields { good, amount, .. } => {
                                                                    // Conditional on field count; optimistic half-weight
                                                                    card_good_value(*good) * i64::from(*amount) * horizon / 2
                                                                }
                                                                CardEffect::OnPlayGainAnimal { animal, amount, .. }
                                                                | CardEffect::AfterWorkerGainAnimal { animal, amount, .. } => {
                                                                    card_animal_value(*animal) * i64::from(*amount) * horizon
                                                                }
                                                            };
>>>>>>> origin/main"""
    
    new = """                                // New variants for market followup cards
                                CardEffect::AfterAnyWorkerGain { good, amount, .. }
                                | CardEffect::AfterAnyWorkerOthersGain { good, amount, .. } => {
                                    // Fires on any player's use of the space; approximate with per-round expectation
                                    card_good_value(*good) * i64::from(*amount) * horizon / 4
                                }
                                CardEffect::HarvestStartGainIfFields { good, amount, .. } => {
                                                                    // Conditional on field count; optimistic half-weight
                                                                    card_good_value(*good) * i64::from(*amount) * horizon / 2
                                                                }
                                                                CardEffect::OnPlayGainAnimal { animal, amount, .. }
                                                                | CardEffect::AfterWorkerGainAnimal { animal, amount, .. } => {
                                                                    card_animal_value(*animal) * i64::from(*amount) * horizon
                                                                }
                                CardEffect::AfterImprovementPlayedGain { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                            };"""
    
    content = content.replace(old, new)
    
    with open("src/policy.rs", "w") as f:
        f.write(content)
    
    print("policy.rs fixed!")

def fix_registry():
    with open("src/registry.rs", "r") as f:
        content = f.read()
    
    # Fix 1: EventKind indexing
    old1 = """<<<<<<< HEAD
                crate::cards::CardEffect::AfterImprovementPlayedGain { .. } => {
                    EventKind::ImprovementPlayed
                }
=======
                crate::cards::CardEffect::OnPlayGainAnimal { .. } => EventKind::CardPlayed,
                crate::cards::CardEffect::AfterWorkerGainAnimal { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterAnyWorkerGain { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterAnyWorkerOthersGain { .. } => {
                    EventKind::WorkerPlaced
                }
                crate::cards::CardEffect::HarvestStartGainIfFields { .. } => EventKind::Harvest,
>>>>>>> origin/main"""
    
    new1 = """                crate::cards::CardEffect::OnPlayGainAnimal { .. } => EventKind::CardPlayed,
                crate::cards::CardEffect::AfterWorkerGainAnimal { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterAnyWorkerGain { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterAnyWorkerOthersGain { .. } => {
                    EventKind::WorkerPlaced
                }
                crate::cards::CardEffect::HarvestStartGainIfFields { .. } => EventKind::Harvest,
                crate::cards::CardEffect::AfterImprovementPlayedGain { .. } => {
                    EventKind::ImprovementPlayed
                }"""
    
    content = content.replace(old1, new1)
    
    # Fix 2: validate_structure match
    old2 = """<<<<<<< HEAD
                        | crate::cards::CardEffect::AfterImprovementPlayedGain { amount, .. } => {
=======
                        | crate::cards::CardEffect::OnPlayGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterWorkerGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerGain { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | crate::cards::CardEffect::HarvestStartGainIfFields { amount, .. } => {
>>>>>>> origin/main"""
    
    new2 = """| crate::cards::CardEffect::OnPlayGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterWorkerGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerGain { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | crate::cards::CardEffect::HarvestStartGainIfFields { amount, .. }
                        | crate::cards::CardEffect::AfterImprovementPlayedGain { amount, .. } => {"""
    
    content = content.replace(old2, new2)
    
    with open("src/registry.rs", "w") as f:
        f.write(content)
    
    print("registry.rs fixed!")

if __name__ == "__main__":
    fix_card_events()
    fix_cards()
    fix_policy()
    fix_registry()
    print("All files fixed!")
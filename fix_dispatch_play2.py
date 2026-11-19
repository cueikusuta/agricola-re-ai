with open('src/card_events.rs', 'r') as f:
    content = f.read()

# The issue is the Phase 4b is nested inside the C068 if block
# We need to move it outside - after the C068/C116 loops but before Ok(())

# Find the problematic section and fix it
# The structure should be:
#   if new_card.kind == Occupation {
#       // C116 loop
#       // C068 loop
#   }
#   // Phase 4b here (for MinorImprovement)
#   Ok(())

# Current broken structure has Phase 4b inside the C068 if block
# Let me find and fix it

old = '''                    }
                }
            }
        }
    }
    // Phase 4b: After — improvement-play dependent triggers owned by EARLIER played
    // cards. A109 小商人 (`AfterImprovementPlayedGain`): each time the owner plays
    // an improvement from hand via a Major/Minor Improvement action (including renovation-combined),
    // it grants food. The new card is already last in `played_cards` (the caller ran the zone move),
    // so the scan covers exactly the cards played "before this one".
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
                    }'''

new = '''                    }
                }
            }
        }
    }
    // Phase 4b: After — improvement-play dependent triggers owned by EARLIER played
    // cards. A109 小商人 (`AfterImprovementPlayedGain`): each time the owner plays
    // an improvement from hand via a Major/Minor Improvement action (including renovation-combined),
    // it grants food. The new card is already last in `played_cards` (the caller ran the zone move),
    // so the scan covers exactly the cards played "before this one".
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
}'''

if old in content:
    content = content.replace(old, new)
    with open('src/card_events.rs', 'w') as f:
        f.write(content)
    print("Fixed dispatch_play structure")
else:
    print("Pattern not found")
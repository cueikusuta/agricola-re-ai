import re

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Find the boundaries
fn_start = content.find('fn dispatch_after_worker_any_player(')
fn_end = content.find('pub fn dispatch_after_worker(')

# The correct version of dispatch_after_worker_any_player
correct_fn = '''/// Dispatch AfterAnyWorkerGain / AfterAnyWorkerOthersGain effects for ALL players.
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

/// Dispatch AfterWorker phase for a completed worker action.
/// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
/// for post-work chain integration (Issue #270).
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_after_worker('''

# Replace the section
new_content = content[:fn_start] + correct_fn + content[fn_end + len('pub fn dispatch_after_worker('):]

# But wait, we need to keep the rest of dispatch_after_worker function
# Let's find the end of the dispatch_after_worker signature
sig_end = content.find(') -> Result<Transition, String> {', fn_end)
if sig_end == -1:
    sig_end = content.find(') -> Result<Transition, String> {', fn_end)
    
print(f"Signature ends at: {sig_end}")

# Actually, let's just replace from fn_start to the opening brace of dispatch_after_worker
brace_pos = content.find('{', fn_end)
print(f"Opening brace at: {brace_pos}")

# The correct replacement should be from fn_start to brace_pos+1 (including the brace)
# But we need to keep the function body of dispatch_after_worker

# Let me just replace the whole section from fn_start to the line before "    // Clone players"
clone_pos = content.find('    // Clone players for all_players parameter', fn_end)
print(f"Clone comment at: {clone_pos}")

# Actually the simplest is to replace from fn_start to just before the dispatch_after_worker function body
# The function body starts after the opening brace
body_start = content.find('\n    // Clone players', fn_end)
if body_start == -1:
    body_start = content.find('\n    let all_players', fn_end)
print(f"Body starts at: {body_start}")

# Let's include the opening brace and the first line of body
# Find the opening brace of dispatch_after_worker
brace = content.find('{', fn_end)
next_newline = content.find('\n', brace)
# We want to keep from brace+1 onwards
# But we need to add our correct_fn and then the rest from body_start

# Actually, let's just do a simple string replacement for the whole messed up section
# The section from fn_start to the line before "    // Clone players" is wrong
# We replace it with correct_fn + the opening brace and body

# Find where the actual body of dispatch_after_worker starts (after the signature)
# The signature ends with "{"
# The body starts on the next line
rest = content[brace+1:]  # This includes the body

new_content = content[:fn_start] + correct_fn + rest

with open('src/card_events.rs', 'w') as f:
    f.write(new_content)

print("Fixed!")
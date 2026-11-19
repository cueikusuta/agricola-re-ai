with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Fix the specific problematic section
old = '''}
                            }
                        }
                        Ok(())
                        }

                        /// Dispatch AfterWorker phase for a completed worker action.
                        /// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
                        /// for post-work chain integration (Issue #270).
                        #[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
                        pub fn dispatch_after_worker(
                            card_set: &CardSet,
                            player: usize,
                            space: Space,
                            cards: &mut crate::cards::CardState,
                            players: &mut [Player],
                            improvement_market: &mut Vec<crate::model::Improvement>,
                            delayed_well: &mut BTreeMap<u8, Vec<usize>>,
                            round: u8,
                            used_spaces: &std::collections::BTreeSet<Space>,
                            event_budget: &mut u32,
                            card_uses: &mut BTreeMap<String, u32>,
                            card_use_limits: &BTreeMap<String, u32>,
                            pending: &mut Option<PendingDecision>,
                            allow_choose_trigger_order_pause: bool,
                        ) -> Result<Transition, String> {'''

new = '''}
                            }
                        }
                    }
                    Ok(())
}

/// Dispatch AfterWorker phase for a completed worker action.
/// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
/// for post-work chain integration (Issue #270).
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_after_worker(
    card_set: &CardSet,
    player: usize,
    space: Space,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {'''

if old in content:
    content = content.replace(old, new)
    with open('src/card_events.rs', 'w') as f:
        f.write(content)
    print('Fixed!')
else:
    print('Pattern not found')
    # Try to find it with regex
    import re
    matches = list(re.finditer(r'Ok\(\)\s*\}\s*\n\s*/// Dispatch AfterWorker', content))
    for m in matches:
        print(f'Found at {m.start()}: {repr(content[m.start():m.start()+200])}')
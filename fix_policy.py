with open('src/policy.rs', 'r') as f:
    content = f.read()

# Fix the conflict in policy.rs - combine both sides
old = '''                CardEffect::AfterHarvestBakeGain { amount } => i64::from(*amount) * horizon,
<<<<<<< HEAD
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
>>>>>>> origin/main
    }'''

new = '''                CardEffect::AfterHarvestBakeGain { amount } => i64::from(*amount) * horizon,
                                CardEffect::AfterImprovementPlayedGain { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
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
    }'''

if old in content:
    content = content.replace(old, new)
    print("Fixed policy.rs")
else:
    print("Pattern not found - trying alternative")
    # The semicolon placement might be different
    # Let's try with the exact content from the file
    import re
    match = re.search(r'CardEffect::AfterHarvestBakeGain \{ amount \} => i64::from\(\*amount\) \* horizon,[\s\S]*?\};\s*\n\s*\}', content)
    if match:
        print(f"Found at {match.start()}: {repr(match.group()[:300])}")

# Check for remaining conflicts
if '<<<<<<<' in content:
    print("WARNING: Conflict markers still remain!")
else:
    print("All conflicts resolved in policy.rs")

with open('src/policy.rs', 'w') as f:
    f.write(content)
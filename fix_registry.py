with open('src/registry.rs', 'r') as f:
    content = f.read()

# Fix 1: EventKind mapping in add_card (around line 308-320)
old1 = '''                crate::cards::CardEffect::AfterWorkerGainIfUsed { .. } => EventKind::WorkerPlaced,
<<<<<<< HEAD
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
>>>>>>> origin/main
                crate::cards::CardEffect::Passive { .. }'''

new1 = '''                crate::cards::CardEffect::AfterWorkerGainIfUsed { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterImprovementPlayedGain { .. } => {
                    EventKind::ImprovementPlayed
                }
                crate::cards::CardEffect::OnPlayGainAnimal { .. } => EventKind::CardPlayed,
                crate::cards::CardEffect::AfterWorkerGainAnimal { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterAnyWorkerGain { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::AfterAnyWorkerOthersGain { .. } => {
                    EventKind::WorkerPlaced
                }
                crate::cards::CardEffect::HarvestStartGainIfFields { .. } => EventKind::Harvest,
                crate::cards::CardEffect::Passive { .. }'''

# Fix 2: Amount validation in validate_structure (around line 422-430)
old2 = '''                        | crate::cards::CardEffect::OnPlayGainPerCount { amount, .. }
<<<<<<< HEAD
                        | crate::cards::CardEffect::AfterImprovementPlayedGain { amount, .. } => {
=======
                        | crate::cards::CardEffect::OnPlayGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterWorkerGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerGain { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | crate::cards::CardEffect::HarvestStartGainIfFields { amount, .. } => {
>>>>>>> origin/main
                            *amount
                        }'''

new2 = '''                        | crate::cards::CardEffect::OnPlayGainPerCount { amount, .. }
                        | crate::cards::CardEffect::AfterImprovementPlayedGain { amount, .. }
                        | crate::cards::CardEffect::OnPlayGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterWorkerGainAnimal { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerGain { amount, .. }
                        | crate::cards::CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | crate::cards::CardEffect::HarvestStartGainIfFields { amount, .. } => {
                            *amount
                        }'''

if old1 in content:
    content = content.replace(old1, new1)
    print("Fixed registry.rs conflict 1")
else:
    print("Conflict 1 pattern not found")

if old2 in content:
    content = content.replace(old2, new2)
    print("Fixed registry.rs conflict 2")
else:
    print("Conflict 2 pattern not found")

# Check for remaining conflicts
if '<<<<<<<' in content:
    print("WARNING: Conflict markers still remain!")
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print("All conflicts resolved in registry.rs")

with open('src/registry.rs', 'w') as f:
    f.write(content)
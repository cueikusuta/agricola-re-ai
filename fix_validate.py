with open('src/cards.rs', 'r') as f:
    content = f.read()

# The validate function is corrupted. Let me find and replace it with the correct version.
# From main, the validate function structure is correct. I need to use that but add
# the AfterImprovementPlayedGain variant to the amount match.

# Find the validate function
val_start = content.find('    pub fn validate(&self, players: usize) -> Result<(), String> {')
val_end = content.find('\n}\n\n/// Private: must never be serialized')
if val_end == -1:
    val_end = content.find('\n}\n\nfn safe_gain')
print(f"validate: {val_start} to {val_end}")

# The correct validate function from main, with AfterImprovementPlayedGain added
correct_validate = '''    pub fn validate(&self, players: usize) -> Result<(), String> {
        if !self.enabled() {
            return Ok(());
        }
        let mut ids = BTreeSet::new();
        for card in &self.definitions {
            if card.id.is_empty()
                || card.id.len() > 64
                || !card
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(card.id.clone())
            {
                return Err("invalid or duplicate card id".into());
            }
            if !(-10..=20).contains(&card.points)
                || card
                    .cost
                    .iter()
                    .any(|(_, quantity)| *quantity == 0 || *quantity > 30)
                || card.effects.iter().any(|effect| {
                    let amount = match effect {
                        CardEffect::OnPlayGain { amount, .. }
                        | CardEffect::AfterWorkerGain { amount, .. }
                        | CardEffect::OnPlayGainAnimal { amount, .. }
                        | CardEffect::AfterWorkerGainAnimal { amount, .. } => *amount,
                        CardEffect::EndGameBonus { .. }
                        | CardEffect::Prerequisite { .. }
                        | CardEffect::Passive { .. } => 1,
                        #[cfg(test)]
                        CardEffect::PendingProbe { .. } => 1,
                        CardEffect::OnPlayGainPerStored { amount, .. }
                        | CardEffect::OnPlayGainIfHouse { amount, .. }
                        | CardEffect::StartRoundGain { amount, .. }
                        | CardEffect::StartRoundGainIfFoodAtLeastRound {
                            amount,
                            ..
                        }
                        | CardEffect::StartRoundGainIfNoMinorPlayed {
                            amount,
                            ..
                        }
                        | CardEffect::HarvestFieldGainIfNoMinorPlayed {
                            amount,
                            ..
                        }
                        | CardEffect::OnPlayGainPerImprovementPlayed {
                            amount,
                            ..
                        }
                        | CardEffect::AfterOccupationGainPerFoodCost {
                            amount,
                            ..
                        }
                        | CardEffect::AfterOccupationGain { amount, .. }
                        | CardEffect::HarvestFieldGain { amount, .. }
                        | CardEffect::AfterWorkerGainIfUsed { amount, .. }
                        | CardEffect::AfterClayConstructionGain { amount, .. }
                        | CardEffect::AfterHarvestBakeGain { amount }
                        | CardEffect::OnPlayGainPerCount { amount, .. }
                        | CardEffect::AfterImprovementPlayedGain { amount, .. }
                        | CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. } => *amount,
                    };
                    amount == 0 || amount > 30
                })
                || card.effects.iter().any(|effect| {
                    matches!(effect, CardEffect::OnPlayGainPerStored { divisor: 0, .. })
                        || matches!(
                            effect,
                            CardEffect::HarvestFieldGain {
                                rule: HarvestFieldRule::Animals { sheep_per: 0, .. }
                                    | HarvestFieldRule::Animals { cattle_per: 0, .. },
                                ..
                            }
                        )
                })
            {
                return Err("unsupported card cost, points, or effect".into());
            }
        }
        for card in &self.definitions {
            for effect in &card.effects {
                if let CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(min),
                } = effect
                {
                    if *min == 0 || *min as usize > players {
                        return Err(format!(
                            "card {} is not eligible for this player count",
                            card.id
                        ));
                    }
                }
            }
        }
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let count = self.definitions.iter().filter(|c| c.kind == kind).count();
            if count < 7 * players {
                return Err(
                    "implemented card set needs seven cards of each kind per player".into(),
                );
            }
        }
        Ok(())
    }'''

if val_start >= 0 and val_end >= 0:
    new_content = content[:val_start] + correct_validate + content[val_end:]
    with open('src/cards.rs', 'w') as f:
        f.write(new_content)
    print("Fixed validate function")
else:
    print("Could not find validate function boundaries")

# Check for syntax
import subprocess
result = subprocess.run(['cargo', 'check'], cwd='C:/Users/cue/codex_private_project/agricola-wt-outband-missing', capture_output=True, text=True)
print(result.stdout[:2000] if result.stdout else result.stderr[:2000])
print(f"Exit code: {result.returncode}")
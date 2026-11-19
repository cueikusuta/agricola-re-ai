with open('src/cards.rs', 'r') as f:
    content = f.read()

# Since the file has a weird state with only <<<<<<< HEAD markers,
# let me just rebuild the three sections correctly based on the git diff output.

# The correct merged content for each section:

# Section 1: CardEffect enum - should have ALL variants
# From the git diff, the correct order after OnPlayGainPerCount is:
# 1. AfterImprovementPlayedGain (from HEAD)
# 2. AfterOccupationGain (from main)
# 3. AfterAnyWorkerGain (from main)
# 4. AfterAnyWorkerOthersGain (from main)
# 5. HarvestStartGainIfFields (from main)
# 6. OnPlayGainAnimal (from main)
# 7. AfterWorkerGainAnimal (from main)

# Section 2: public_tag match - should have all arms
# Section 3: validate function - should check all amount fields

# Let me just completely rewrite the three problematic regions
# by finding their boundaries and replacing with correct content.

import re

# Find the three conflict regions by looking for <<<<<<< HEAD
# Region 1: After OnPlayGainPerCount variant, before the closing } of CardEffect enum
# Region 2: In public_tag match, after OnPlayGainPerCount arm
# Region 3: In validate function, after OnPlayGainPerCount amount check

# Let's do a more surgical fix - replace from <<<<<<< HEAD to the next logical boundary

# Fix 1: CardEffect enum variants
# Pattern: everything from "<<<<<<< HEAD" after OnPlayGainPerCount to the "}" that closes the enum
# But since there's no >>>>>>>, we need to find where the enum ends

# Actually, let me just write the entire correct CardEffect enum based on the git diff
# The enum ends before "impl CardEffect {"

# Find the start of CardEffect enum
enum_start = content.find('pub enum CardEffect {')
enum_end = content.find('\n}\n\nimpl CardEffect {')
if enum_end == -1:
    enum_end = content.find('\n}\n\nimpl CardEffect')
print(f"Enum: {enum_start} to {enum_end}")

# The correct enum content (from git diff)
correct_enum = '''pub enum CardEffect {
    /// Test-only probe: an effect whose resolution requires a player decision.
    /// Compiled only in the crate's own test build, so the production card pool
    /// and `CardSet::content_hash` are unaffected. It exists so the dispatcher's
    /// Interrupt branch (handler -> `EffectResult::Pending` -> `PendingDecision`
    /// -> pause) is exercised end to end instead of being an untested path.
    #[cfg(test)]
    PendingProbe {
        options: Vec<(Good, u32)>,
    },
    OnPlayGain {
        good: Good,
        amount: u32,
    },
    AfterWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    EndGameBonus {
        rule: ScoreBonusRule,
    },
    Prerequisite {
        rule: CardRequirement,
    },
    OnPlayGainPerStored {
        stored: Good,
        divisor: u32,
        good: Good,
        amount: u32,
    },
    OnPlayGainIfHouse {
        house: House,
        rooms: u8,
        good: Good,
        amount: u32,
    },
    StartRoundGain {
        good: Good,
        amount: u32,
        house: Option<House>,
        rooms: Option<u8>,
    },
    HarvestFieldGain {
        rule: HarvestFieldRule,
        good: Good,
        amount: u32,
    },
    AfterWorkerGainIfUsed {
        space: Space,
        required_space: Space,
        good: Good,
        amount: u32,
    },
    Passive {
        rule: PassiveRule,
    },
    AfterClayConstructionGain {
        good: Good,
        amount: u32,
    },
    AfterHarvestBakeGain {
        amount: u32,
    },
    /// C111 小動物飼育員 (Small Animal Breeder): at each round start, if the
    /// owner's food stock is at least the current round number, gain `amount`
    /// of `good` (the printed card grants 1 food). The threshold is the round
    /// number, so this is expressed as its own variant instead of widening the
    /// serialized shape of `StartRoundGain` (existing saves and content hashes
    /// must keep their exact bytes).
    StartRoundGainIfFoodAtLeastRound {
        good: Good,
        amount: u32,
    },
    /// C116 調度品職人 (Furniture Maker): each time the owner plays an
    /// occupation AFTER this card, gain `amount` of `good` per food paid as
    /// that occupation's cost (printed cost + optional fee). Fired from the
    /// play dispatcher's after-play scan; the legacy `cards::play` helper does
    /// not re-scan earlier cards and therefore never fires it.
    AfterOccupationGainPerFoodCost {
        good: Good,
        amount: u32,
    },
    /// E111 世捨て人 (Recluse), round-start half: at each round start, if the
    /// owner has played no minor improvements, gain `amount` of `good`. The
    /// condition needs a card-definition lookup, so the generic condition
    /// scan reports it as triggerable and the handler performs the check.
    StartRoundGainIfNoMinorPlayed {
        good: Good,
        amount: u32,
    },
    /// E111 世捨て人 (Recluse), harvest half: at the start of each harvest
    /// (the field phase is the first step of the harvest), if the owner has
    /// played no minor improvements, gain `amount` of `good`. Like the round
    /// variant, the handler performs the definition-dependent check.
    HarvestFieldGainIfNoMinorPlayed {
        good: Good,
        amount: u32,
    },
    /// A117 木材調達人 (Wood Carrier): when playing this card, gain `amount`
    /// of `good` per improvement already in front of the owner (base
    /// improvements plus played minor-improvement cards; this card itself is
    /// an occupation and never counts).
    OnPlayGainPerImprovementPlayed {
        good: Good,
        amount: u32,
    },
    /// On-play gain scaled by a counted quantity: gain `amount` of `good` per
    /// unit of `count_kind` the player owns at play time.
    OnPlayGainPerCount {
        count_kind: CountKind,
        good: Good,
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
    },
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
}'''

# Replace the enum
new_content = content[:enum_start] + correct_enum + content[enum_end+1:]  # +1 to keep the closing brace

# Now fix the public_tag match
# Find the public_tag function
tag_start = new_content.find('pub fn public_tag(&self)')
tag_end = new_content.find('\n}\n\n/// What to count', tag_start)
if tag_end == -1:
    tag_end = new_content.find('\n}\n\n#[derive', tag_start)
print(f"public_tag: {tag_start} to {tag_end}")

correct_tag = '''impl CardEffect {
    /// Stable public name for this implemented effect category. Used to build
    /// the neutral draft-AI catalog (#104). It describes only effects the
    /// engine actually implements, never a guessed printed effect.
    pub fn public_tag(&self) -> &'static str {
        match self {
            #[cfg(test)]
            CardEffect::PendingProbe { .. } => "pending_probe",
            CardEffect::OnPlayGain { .. } => "on_play_gain",
            CardEffect::AfterWorkerGain { .. } => "after_worker_gain",
            CardEffect::EndGameBonus { .. } => "end_game_bonus",
            CardEffect::Prerequisite { .. } => "prerequisite",
            CardEffect::OnPlayGainPerStored { .. } => "on_play_gain_per_stored",
            CardEffect::OnPlayGainIfHouse { .. } => "on_play_gain_if_house",
            CardEffect::StartRoundGain { .. } => "start_round_gain",
            CardEffect::HarvestFieldGain { .. } => "harvest_field_gain",
            CardEffect::AfterWorkerGainIfUsed { .. } => "after_worker_gain_if_used",
            CardEffect::Passive { .. } => "passive",
            CardEffect::AfterClayConstructionGain { .. } => "after_clay_construction_gain",
            CardEffect::AfterHarvestBakeGain { .. } => "after_harvest_bake_gain",
            CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => {
                "start_round_gain_if_food_at_least_round"
            }
            CardEffect::AfterOccupationGainPerFoodCost { .. } => {
                "after_occupation_gain_per_food_cost"
            }
            CardEffect::AfterOccupationGain { .. } => "after_occupation_gain",
            CardEffect::StartRoundGainIfNoMinorPlayed { .. } => {
                "start_round_gain_if_no_minor_played"
            }
            CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => {
                "harvest_field_gain_if_no_minor_played"
            }
            CardEffect::OnPlayGainPerImprovementPlayed { .. } => {
                "on_play_gain_per_improvement_played"
            }
            CardEffect::OnPlayGainPerCount { .. } => "on_play_gain_per_count",
            CardEffect::AfterImprovementPlayedGain { .. } => "after_improvement_played_gain",
            CardEffect::OnPlayGainAnimal { .. } => "on_play_gain_animal",
            CardEffect::AfterWorkerGainAnimal { .. } => "after_worker_gain_animal",
            CardEffect::AfterAnyWorkerGain { .. } => "after_any_worker_gain",
            CardEffect::AfterAnyWorkerOthersGain { .. } => "after_any_worker_others_gain",
            CardEffect::HarvestStartGainIfFields { .. } => "harvest_start_gain_if_fields",
        }
    }
}'''

new_content = new_content[:tag_start] + correct_tag + new_content[tag_end:]

# Now fix the validate function
# Find the validate function's amount matching section
val_start = new_content.find('let amount = match effect {')
val_end = new_content.find('amount == 0 || amount > 30', val_start)
# Find the end of the match expression (the closing });)
brace_pos = new_content.find('};\n', val_end)
print(f"validate amount: {val_start} to {brace_pos}")

correct_val = '''                    let amount = match effect {
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
                    };'''

new_content = new_content[:val_start] + correct_val + new_content[brace_pos+2:]

# Check for remaining conflicts
if '<<<<<<<' in new_content:
    print("WARNING: Conflict markers still remain!")
    for idx, l in enumerate(new_content.split('\n')):
        if '<<<<<<<' in l or '=======' in l or '>>>>>>>' in l:
            print(f"Line {idx+1}: {l}")
else:
    print("All conflict markers removed!")

with open('src/cards.rs', 'w') as f:
    f.write(new_content)

print("Done fixing cards.rs")
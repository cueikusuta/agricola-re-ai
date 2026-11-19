with open('src/cards.rs', 'r') as f:
    content = f.read()

# Fix 1: CardEffect enum - need both AfterImprovementPlayedGain (HEAD) and the new variants from main
old1 = '''    /// A109 小商人 (Small Trader): when an improvement is played from hand
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
>>>>>>> origin/main
}'''

new1 = '''    /// A109 小商人 (Small Trader): when an improvement is played from hand
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

# Fix 2: public_tag match arms
old2 = '''            CardEffect::AfterImprovementPlayedGain { .. } => "after_improvement_played_gain",
=======
            CardEffect::OnPlayGainAnimal { .. } => "on_play_gain_animal",
            CardEffect::AfterWorkerGainAnimal { .. } => "after_worker_gain_animal",
            CardEffect::AfterAnyWorkerGain { .. } => "after_any_worker_gain",
            CardEffect::AfterAnyWorkerOthersGain { .. } => "after_any_worker_others_gain",
            CardEffect::HarvestStartGainIfFields { .. } => "harvest_start_gain_if_fields",
>>>>>>> origin/main
        }'''

new2 = '''            CardEffect::AfterImprovementPlayedGain { .. } => "after_improvement_played_gain",
            CardEffect::OnPlayGainAnimal { .. } => "on_play_gain_animal",
            CardEffect::AfterWorkerGainAnimal { .. } => "after_worker_gain_animal",
            CardEffect::AfterAnyWorkerGain { .. } => "after_any_worker_gain",
            CardEffect::AfterAnyWorkerOthersGain { .. } => "after_any_worker_others_gain",
            CardEffect::HarvestStartGainIfFields { .. } => "harvest_start_gain_if_fields",
        }'''

# Fix 3: validate function amount checking
old3 = '''                        | CardEffect::AfterImprovementPlayedGain { amount, .. } => *amount,
=======
                        | CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. } => *amount,
>>>>>>> origin/main
                    };'''

new3 = '''                        | CardEffect::AfterImprovementPlayedGain { amount, .. } => *amount,
                        | CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. } => *amount,
                    };'''

if old1 in content:
    content = content.replace(old1, new1)
    print("Fixed CardEffect enum")
else:
    print("CardEffect enum pattern not found")

if old2 in content:
    content = content.replace(old2, new2)
    print("Fixed public_tag match")
else:
    print("public_tag pattern not found")

if old3 in content:
    content = content.replace(old3, new3)
    print("Fixed validate function")
else:
    print("validate function pattern not found")

# Check for remaining conflicts
if '<<<<<<<' in content:
    print("WARNING: Conflict markers still remain!")
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print("All conflicts resolved in cards.rs")

with open('src/cards.rs', 'w') as f:
    f.write(content)
//! B-deck band card definitions.
use super::{
    CardDefinition, CardEffect, CardKind, CardRequirement, CountKind, HarvestFieldRule,
    PostPlayDestination, ScoreBonusRule,
};
use crate::model::{Animal, Good, Space};

/// B-deck band cards with typed effects (BGA Agricola card wiki;
/// B122 p=3135, B142 p=3207, B144 p=3113). Kept out of the A/B two-player
/// preset, same as the C/D/E band pools.
///
/// Chapter 1 (restack session 2026-11-23: `on_play` simple gains) adds cards
/// whose printed on-play effect is a single constant `gain_resource` step,
/// modelled with the existing `CardEffect::OnPlayGain` variant. Each of those
/// cards prints further effects on OTHER triggers; only the on-play half is
/// wired here, so their spec `implementation_status` stays `in_progress` and
/// the remaining halves belong to their own trigger chapters (recorded per
/// card below). Because every built-in review entry is fail-closed
/// `unverified`, no pool gate admits these cards until official eligibility
/// is recorded, and the experimental pool only ever grants the on-play half.
pub fn verified_band_b_cards() -> Vec<CardDefinition> {
    let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
        space,
        good,
        amount,
    };
    vec![
        // B122 鉱物学者 (Mineralogist): clay accumulation spaces (clay_pit, hollow)
        // grant 1 stone; stone accumulation spaces (western_quarry, eastern_quarry)
        // grant 1 clay. Available at all player counts.
        CardDefinition {
            use_limit: None,
            id: "B122".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                gain(Space::ClayPit, Good::Stone, 1),
                gain(Space::Hollow, Good::Stone, 1),
                gain(Space::WesternQuarry, Good::Clay, 1),
                gain(Space::EasternQuarry, Good::Clay, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B142 八百屋 (Greengrocer): grain_seeds grants 1 vegetable.
        // Only available at 3+ players.
        CardDefinition {
            use_limit: None,
            id: "B142".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                gain(Space::GrainSeeds, Good::Vegetable, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B144 坑夫 (Collier): clay accumulation spaces (clay_pit, hollow) grant
        // 1 wood; clay_pit additionally grants 1 reed.
        // Only available at 3+ players.
        CardDefinition {
            use_limit: None,
            id: "B144".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                gain(Space::ClayPit, Good::Wood, 1),
                gain(Space::Hollow, Good::Wood, 1),
                gain(Space::ClayPit, Good::Reed, 1),
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B143 粘土守 (Clay Warden): on another player's Hollow use, each
        // non-actor owner gains clay 1; 3p -> +1 clay (clay 2 total), 4p ->
        // +1 food (clay 1 + food 1). Only available at 3+ players.
        CardDefinition {
            use_limit: None,
            id: "B143".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3),
                },
                CardEffect::AfterAnyWorkerGainByCountIfNotActor {
                    space: Space::Hollow,
                    base_good: Good::Clay,
                    base_amount: 1,
                    extra_3p_good: Good::Clay,
                    extra_3p_amount: 1,
                    extra_4p_good: Good::Food,
                    extra_4p_amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B006 採石場見学 (Excursion to the Quarry): cost 2 food; prerequisite:
        // at least 1 occupation played; on play gain 1 stone per family member.
        CardDefinition {
            use_limit: None,
            id: "B006".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                CardEffect::OnPlayGainPerCount {
                    count_kind: CountKind::FamilyMembers,
                    good: Good::Stone,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B007 賃金 (Wages): cost 0 (free); on play gain 2 food + 1 food per
        // major improvement in the bottom row (Joinery / Pottery / Basketry).
        CardDefinition {
            use_limit: None,
            id: "B007".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 2,
                },
                CardEffect::OnPlayGainPerCount {
                    count_kind: CountKind::MajorImprovementsBottomRow,
                    good: Good::Food,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B037 大農場の称号 (Grange): minor improvement, no cost, 3 printed
        // points; on play the owner gains 1 food. Uses the existing
        // `OnPlayGain` variant only: no new effect variant, event kind or
        // saved field (agricola.cloudfree.jp ?p=3220 + /cardlist_en/, checked
        // 2026-11-09; data/bga-card-specs/B037.json).
        //
        // Hold (data/bga-card-specs/B037.json review.unresolved_fields, same
        // pattern as B007/C005): the spec's play prerequisites (field_count
        // n=6 + all_animal_types) are deliberately NOT enforced. The
        // `CardRequirement` vocabulary has no bare field-tile count and no
        // all-animal-types rule, and this chapter adds no new variant, so the
        // play stays unguarded until a vocabulary follow-up lands. The spec
        // test contract's `B037-not-triggered` case (play_rejected with 5
        // field tiles / 3 animal types) is pinned engine-side as
        // no-double-grant on later unrelated actions and on save/resume and
        // event replay instead; see the `b037_*` tests in
        // tests/ch03_onplay_per_count.rs.
        CardDefinition {
            use_limit: None,
            id: "B037".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 3,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B058 雑草掻き (Crack Weeder): cost 1 wood; on play gain 1 food, and
        // at each harvest field phase gain 1 extra food per vegetable taken
        // from the fields (BGA Agricola card wiki; agricola.cloudfree.jp
        // ?p=3199, cross-checked against the English list page). Same typed
        // vocabulary as C106 じゃがいも収穫者: OnPlayGain plus HarvestFieldGain
        // with HarvestFieldRule::VegetablesHarvested, so no new variant, event
        // kind or saved field is introduced. Eligible at all player counts.
        CardDefinition {
            use_limit: None,
            id: "B058".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::VegetablesHarvested,
                    good: Good::Food,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B073 贈り物かご (Gift Basket): cost 1 reed, 1 VP; prerequisite: at
        // least 3 occupations played. On play, if the owner has exactly
        // 2/3/4/5 rooms, gain 1 vegetable/food/grain/vegetable. Each table
        // row is one OnPlayGainIfRoomCount effect; only the row matching the
        // current room count exactly resolves. Eligible at all player counts
        // (spec eligibility 2/3/4 = yes).
        CardDefinition {
            use_limit: None,
            id: "B073".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Reed, 1)],
            points: 1,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(3),
                },
                CardEffect::OnPlayGainIfRoomCount {
                    rooms: 2,
                    good: Good::Vegetable,
                    amount: 1,
                },
                CardEffect::OnPlayGainIfRoomCount {
                    rooms: 3,
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::OnPlayGainIfRoomCount {
                    rooms: 4,
                    good: Good::Grain,
                    amount: 1,
                },
                CardEffect::OnPlayGainIfRoomCount {
                    rooms: 5,
                    good: Good::Vegetable,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B105 箱造り (Case Builder): cost 0; on play, for each of
        // food/grain/vegetable/reed/wood the owner already holds at least 2
        // of, gain 1 of that good. Goods below threshold 2 grant nothing.
        // Judged per good against the owner's own supply. Eligible at all
        // player counts (spec 2/3/4 = yes).
        CardDefinition {
            use_limit: None,
            id: "B105".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainEachWithAtLeast {
                goods: vec![
                    Good::Food,
                    Good::Grain,
                    Good::Vegetable,
                    Good::Reed,
                    Good::Wood,
                ],
                threshold: 2,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B153 寮長 (Housemaster): no cost, no printed points; at final
        // scoring, sum the major-improvement scores after doubling the
        // lowest single one (one only, even when the lowest score is tied)
        // and take 1/2/3/4 bonus points at a doubled total of 5/7/9/11 or
        // more (BGA Agricola card wiki; agricola.cloudfree.jp ?p=3104,
        // cross-checked against the English list page). Only available at
        // 4 players (spec eligibility 2/3 = no, 4 = yes); the 2/3-player
        // registry projection drops the card through the MinimumPlayers(4)
        // prerequisite, and no new saved field or PendingDecision is
        // introduced.
        CardDefinition {
            use_limit: None,
            id: "B153".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::MajorImprovementsLowestDoubledThresholds,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B054 肥料運搬車 (Tumbrel): cost 1 wood; on play gain 2 food.
        // Eligibility 2/3/4 = yes. Still prints an after_sow "1 food per
        // stable" step (follow-up; the harvest/sow chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B054".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 2,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B117 情報屋 (Informant): occupation, no cost; on play gain 1 wood.
        // Eligibility 2/3/4 = yes (85-126 rule). Still prints an
        // after_work_phase "more stone than clay => 1 wood" step (follow-up;
        // the round/work-phase chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B117".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B151 小百姓 (Little Peasant): occupation, no cost; on play gain
        // 1 stone. 4 players only (148-168 rule), so the effect is guarded by
        // MinimumPlayers(4) and projects only at 4 players. Still prints the
        // passive "all spaces except Meeting Place count as unoccupied while
        // living in a 2-room wooden house" rule (follow-up; the passive
        // chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B151".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::OnPlayGain {
                    good: Good::Stone,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B160 飲み屋の亭主 (Pub Owner): occupation, no cost; on play gain
        // 1 grain. 4 players only (148-168 rule) -> MinimumPlayers(4) guard.
        // Still prints the end_of_work_phase "Forest/Clay Pit/Reed Field all
        // occupied => 1 grain" step (follow-up; the round_end/work-phase
        // chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B160".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::OnPlayGain {
                    good: Good::Grain,
                    amount: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

/// B-deck chapter 1: bga-checked on-play cards (B002/B003/B005/B008/B009)
/// whose printed effects map onto the typed effect vocabulary, sourced from
/// the BGA Agricola card wiki (agricola.cloudfree.jp; B002 p=3255, B003
/// p=3254, B005 p=3252, B008 p=3249, B009 p=3248, cross-checked against the
/// English list). Every one of these cards is printed "pass left", expressed
/// as `post_play: LeftNeighborHand`.
///
/// These are deliberately kept OUT of `verified_basic_card_set` (same as the
/// band pools): the built-in A/B two-player preset must keep its pool size,
/// distribution digest and recorded replay hashes unchanged, and B003/B005's
/// hand-dependent effects are not modelled by the hand-less `cards::play`
/// projection the AI evaluators use.
pub fn verified_b_onplay_simple_cards() -> Vec<CardDefinition> {
    let minor = CardKind::MinorImprovement;
    vec![
        // B002 小牧場 (Mini Pasture): cost 2 food; on play fence one empty
        // farmyard cell into a pasture at no wood cost, adjacent to an
        // existing pasture when the player owns one.
        CardDefinition {
            use_limit: None,
            id: "B002".to_owned(),
            kind: minor,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![CardEffect::OnPlayBuildPasture { spaces: 1 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B003 密造酒 (Moonshine): no cost; on play take one occupation from
        // hand and play it for 2 food, or pass it to the left neighbour.
        CardDefinition {
            use_limit: None,
            id: "B003".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B005 経験の蓄積 (Store of Experience): no cost; on play gain 1 good
        // by the number of occupations still in hand: 0-4 stone, 5 reed,
        // 6 clay, 7 wood.
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B008 露店 (Market Stall): cost 1 grain; on play exchange 1 grain
        // for 1 vegetable (the printed cost is paid separately by the play).
        CardDefinition {
            use_limit: None,
            id: "B008".to_owned(),
            kind: minor,
            cost: vec![(Good::Grain, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayExchangeGain {
                give: Good::Grain,
                give_amount: 1,
                receive: Good::Vegetable,
                receive_amount: 1,
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        // B009 むち (Beating Rod): no cost; on play gain 1 reed, or exchange
        // 1 reed for 1 cattle (deterministic engine policy: exchange when a
        // reed is available).
        CardDefinition {
            use_limit: None,
            id: "B009".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainOrExchangeAnimal {
                gain_good: Good::Reed,
                gain_amount: 1,
                pay_good: Good::Reed,
                pay_amount: 1,
                animal: Animal::Cattle,
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
    ]
}

/// ---- Chapter 1 (restack 2026-11-23): on-play simple gains ----
/// Cards below print exactly one constant gain_resource step on their
/// on_play trigger; each is modelled with the existing OnPlayGain
/// variant (no new variant / EventKind / saved field). Their remaining
/// printed effects live on other triggers and stay unwired here
/// (spec implementation_status = in_progress; follow-up noted inline).
///
/// B054 肥料運搬車 (Tumbrel): cost 1 wood; on play gain 2 food.
/// Eligibility 2/3/4 = yes. Still prints an after_sow "1 food per
/// stable" step (follow-up; the harvest/sow chapter owns that trigger).
pub fn verified_b_onplay_simple_cards_extended() -> Vec<CardDefinition> {
    let mut cards = verified_b_onplay_simple_cards();
    cards.extend(vec![
        CardDefinition {
            use_limit: None,
            id: "B054".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 2,
            }],
            // post_play is left at PlayedArea: this card prints no pass-left
            // effect yet (implementation_status = in_progress).
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B117 情報屋 (Informant): occupation, no cost; on play gain 1 wood.
        // Eligibility 2/3/4 = yes (85-126 rule). Still prints an
        // after_work_phase "more stone than clay => 1 wood" step (follow-up;
        // the round/work-phase chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B117".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 1,
            }],
            // post_play is left at PlayedArea: this card prints no pass-left
            // effect yet (implementation_status = in_progress).
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B151 小百姓 (Little Peasant): occupation, no cost; on play gain
        // 1 stone. 4 players only (148-168 rule), so the effect is guarded by
        // MinimumPlayers(4) and projects only at 4 players. Still prints the
        // passive "all spaces except Meeting Place count as unoccupied while
        // living in a 2-room wooden house" rule (follow-up; the passive
        // chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B151".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::OnPlayGain {
                    good: Good::Stone,
                    amount: 1,
                },
            ],
            // post_play is left at PlayedArea: this card prints no pass-left
            // effect yet (implementation_status = in_progress).
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B160 飲み屋の亭主 (Pub Owner): occupation, no cost; on play gain
        // 1 grain. 4 players only (148-168 rule) -> MinimumPlayers(4) guard.
        // Still prints the end_of_work_phase "Forest/Clay Pit/Reed Field all
        // occupied => 1 grain" step (follow-up; the round_end/work-phase
        // chapter owns that trigger).
        CardDefinition {
            use_limit: None,
            id: "B160".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::OnPlayGain {
                    good: Good::Grain,
                    amount: 1,
                },
            ],
            // post_play is left at PlayedArea: this card prints no pass-left
            // effect yet (implementation_status = in_progress).
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]);
    cards
}

/// Test module for verified_band_b_cards.
/// B-deck chapter 1: after_worker_space simple gain cards (B017, B047, B051, B060, B063).
/// These are minor improvements with bga_checked after_worker_space triggers.
/// Kept out of the A/B two-player preset, same as the band pools.
pub fn verified_b_afterworker_simple_cards() -> Vec<CardDefinition> {
    let minor = CardKind::MinorImprovement;
    vec![
        // B017 森林鋤 (Forest Plow): cost 1 wood; after using Forest/Copse/Grove,
        // may pay 2 wood to plow 1 field. The wood stays on the accumulation space.
        CardDefinition {
            use_limit: None,
            id: "B017".to_owned(),
            kind: minor,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::AfterWorkerSpendPlowField {
                    space: Space::Forest,
                    good: Good::Wood,
                    amount: 2,
                    plow: 1,
                },
                CardEffect::AfterWorkerSpendPlowField {
                    space: Space::Copse,
                    good: Good::Wood,
                    amount: 2,
                    plow: 1,
                },
                CardEffect::AfterWorkerSpendPlowField {
                    space: Space::Grove,
                    good: Good::Wood,
                    amount: 2,
                    plow: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B047 ニシン鍋 (Herring Pot): cost 1 clay; after using Fishing,
        // place 1 food on each of the next 3 round spaces.
        CardDefinition {
            use_limit: None,
            id: "B047".to_owned(),
            kind: minor,
            cost: vec![(Good::Clay, 1)],
            points: 0,
            effects: vec![CardEffect::AfterWorkerScheduleFood {
                space: Space::Fishing,
                rounds: 3,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B051 採掘シャベル (Digging Spade): cost 1 wood; playable from round 7;
        // after using Clay Pit, gain food equal to the number of wild boars.
        CardDefinition {
            use_limit: None,
            id: "B051".to_owned(),
            kind: minor,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumRound(7),
                },
                CardEffect::AfterWorkerGainPerAnimal {
                    space: Space::ClayPit,
                    good: Good::Food,
                    per: 1,
                },
            ],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B060 仕込み水 (Brewing Water): free cost; after using Fishing,
        // may pay 1 grain to place 1 food on each of the next 6 round spaces.
        CardDefinition {
            use_limit: None,
            id: "B060".to_owned(),
            kind: minor,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterWorkerPayToScheduleFood {
                space: Space::Fishing,
                pay_good: Good::Grain,
                pay_amount: 1,
                rounds: 6,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
        // B063 味見 (Tasting): cost 2 wood; 1 point; after using Lessons,
        // optionally exchange 1 grain for 4 food.
        CardDefinition {
            use_limit: None,
            id: "B063".to_owned(),
            kind: minor,
            cost: vec![(Good::Wood, 2)],
            points: 1,
            effects: vec![CardEffect::AfterWorkerOptionalExchange {
                space: Space::Lessons,
                give: Good::Grain,
                give_amount: 1,
                receive: Good::Food,
                receive_amount: 4,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        },
    ]
}

#[cfg(test)]
mod verified_band_b_tests {
    use super::super::*;

    fn applied(card_id: &str, spaces: &[Space], _players: usize) -> Player {
        let definitions = verified_band_b_cards();
        let mut player = Player::new(0, 0);
        player.played_cards.push(card_id.to_string());
        let set = CardSet { definitions };
        for &space in spaces {
            after_worker(&set, &mut player, space).unwrap();
        }
        player
    }

    #[test]
    fn b122_stone_from_clay_spaces_clay_from_stone_spaces() {
        // B122: clay_pit/hollow -> stone; western/eastern_quarry -> clay
        let p = applied(
            "B122",
            &[
                Space::ClayPit,
                Space::Hollow,
                Space::WesternQuarry,
                Space::EasternQuarry,
            ],
            2,
        );
        assert_eq!(p.supply.stone, 2); // clay_pit + hollow
        assert_eq!(p.supply.clay, 2); // western_quarry + eastern_quarry
        assert_eq!(p.supply.wood, 0);
        assert_eq!(p.supply.reed, 0);
    }

    #[test]
    fn b122_not_triggered_by_other_spaces() {
        let p = applied(
            "B122",
            &[
                Space::Forest,
                Space::ReedBank,
                Space::Fishing,
                Space::DayLaborer,
            ],
            2,
        );
        assert_eq!(p.supply.stone, 0);
        assert_eq!(p.supply.clay, 0);
    }

    #[test]
    fn b122_playable_at_all_player_counts() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B122").unwrap();
        // No MinimumPlayers prerequisite, so playable at 2, 3, 4
        for players in [2, 3, 4] {
            let p = Player::new(0, 0);
            assert!(
                can_play(&set, card, &p, 1, 0),
                "B122 should be playable at {players} players"
            );
        }
    }

    #[test]
    fn b142_vegetable_from_grain_seeds_only() {
        let p = applied(
            "B142",
            &[Space::GrainSeeds, Space::Farmland, Space::GrainUtilization],
            3,
        );
        assert_eq!(p.supply.vegetable, 1); // only grain_seeds triggers
        assert_eq!(p.supply.grain, 0);
        assert_eq!(p.supply.food, 0);
    }

    #[test]
    fn b142_not_triggered_by_other_spaces() {
        let p = applied(
            "B142",
            &[
                Space::Forest,
                Space::ClayPit,
                Space::VegetableSeeds,
                Space::Cultivation,
            ],
            3,
        );
        assert_eq!(p.supply.vegetable, 0);
    }

    #[test]
    fn b142_requires_three_players() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B142").unwrap();
        let has_min_players = card.effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3)
                }
            )
        });
        assert!(
            has_min_players,
            "B142 should have MinimumPlayers(3) prerequisite"
        );
        let p = Player::new(0, 0);
        assert!(
            can_play(&set, card, &p, 1, 0),
            "B142 should be playable via can_play"
        );
    }

    #[test]
    fn b144_not_triggered_by_other_spaces() {
        let p = applied(
            "B144",
            &[
                Space::Forest,
                Space::WesternQuarry,
                Space::ReedBank,
                Space::Fishing,
            ],
            3,
        );
        assert_eq!(p.supply.wood, 0);
        assert_eq!(p.supply.reed, 0);
    }

    #[test]
    fn b144_wood_from_clay_spaces_reed_from_clay_pit() {
        // B144: clay_pit/hollow -> wood; clay_pit -> reed
        let p = applied("B144", &[Space::ClayPit, Space::Hollow, Space::ClayPit], 3);
        assert_eq!(p.supply.wood, 3); // clay_pit + hollow + clay_pit
        assert_eq!(p.supply.reed, 2); // clay_pit + clay_pit
        assert_eq!(p.supply.clay, 0);
        assert_eq!(p.supply.stone, 0);
    }

    #[test]
    fn b144_requires_three_players() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B144").unwrap();
        let has_min_players = card.effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(3)
                }
            )
        });
        assert!(
            has_min_players,
            "B144 should have MinimumPlayers(3) prerequisite"
        );
        let p = Player::new(0, 0);
        assert!(
            can_play(&set, card, &p, 1, 0),
            "B144 should be playable via can_play"
        );
    }

    #[test]
    fn band_b_cards_are_absent_from_the_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        // A155 sits in the A-deck band pool rather than the B-deck one, but it
        // is covered here too: neither band pool may leak into the A/B
        // two-player preset.
        for id in ["B122", "B142", "B144", "B058", "A155", "B153"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }

    #[test]
    fn b058_is_minor_improvement_costing_wood_one_with_no_prerequisite() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B058").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
        assert!(
            !card
                .effects
                .iter()
                .any(|e| matches!(e, CardEffect::Prerequisite { .. })),
            "B058 has no prerequisite (spec: prerequisites [])"
        );
        // Exactly the C106 vocabulary: OnPlayGain + HarvestFieldGain
        // (VegetablesHarvested); no new variant, event kind or saved field.
        assert_eq!(
            card.effects,
            vec![
                CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::HarvestFieldGain {
                    rule: HarvestFieldRule::VegetablesHarvested,
                    good: Good::Food,
                    amount: 1,
                },
            ]
        );
    }

    /// B058-normal: the play path grants food 1 and pays the wood 1 cost.
    #[test]
    fn b058_play_grants_one_food_and_pays_wood_one() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        let mut hand = vec!["B058".to_owned()];
        play(
            &set,
            &mut hand,
            &mut p,
            "B058",
            CardKind::MinorImprovement,
            1,
            0,
        )
        .unwrap();
        assert_eq!(p.supply.food, 1);
        assert_eq!(p.supply.wood, 0, "cost wood 1 is paid");
        assert_eq!(p.played_cards, vec!["B058"]);
    }

    /// B058-harvest: two growing vegetable fields grant 2 extra food; a grain
    /// field and vegetables already standing in the supply must not count.
    #[test]
    fn b058_harvest_grants_one_food_per_taken_vegetable() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("B058".into());
        p.supply.vegetable = 5; // supply stock must never be miscounted
                                // 野菜畑2（生育中）＋麦畑1 → 野菜2に対して食料2。
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 2, "two vegetable fields -> +2 food");
        assert_eq!(p.supply.vegetable, 5, "supply vegetables are untouched");
        assert_eq!(p.supply.grain, 0, "a grain field yields nothing for B058");
    }

    /// B058-zero: a field phase that takes no vegetables grants no food, and
    /// a player without the card never gains the owner's bonus.
    #[test]
    fn b058_harvest_grants_zero_without_taken_vegetables() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("B058".into());
        p.supply.vegetable = 4; // only on supply, not in fields
        p.farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        p.farm[2].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        harvest_field_rewards(&set, &mut p).unwrap();
        assert_eq!(p.supply.food, 0, "grain/empty fields -> no B058 food");
        let mut q = Player::new(1, 0);
        q.farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        harvest_field_rewards(&set, &mut q).unwrap();
        assert_eq!(q.supply.food, 0, "a non-owner never gains B058 food");
    }

    /// B153 寮長 (Housemaster): occupation, no cost, no printed points, the
    /// MinimumPlayers(4) prerequisite plus the doubled-lowest end-game bonus.
    #[test]
    fn b153_is_four_player_occupation_with_doubled_lowest_bonus() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let card = set.definition("B153").unwrap();
        assert_eq!(card.kind, CardKind::Occupation);
        assert_eq!(card.cost, vec![]);
        assert_eq!(card.points, 0);
        assert_eq!(
            card.effects,
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                },
                CardEffect::EndGameBonus {
                    rule: ScoreBonusRule::MajorImprovementsLowestDoubledThresholds,
                },
            ]
        );
    }

    /// Thresholds 5/7/9/11 pay 1/2/3/4 on the doubled total; the ordinary
    /// major-improvement points are not touched by the bonus.
    #[test]
    fn b153_thresholds_five_seven_nine_eleven() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        // All-1-point majors: n copies double the lowest one, so the doubled
        // total is n + 1 (4/6/8/10 copies land exactly on 5/7/9/11).
        let bonus = |count: usize| {
            let mut p = Player::new(0, 0);
            p.played_cards.push("B153".into());
            p.improvements = vec![Improvement::Fireplace2; count];
            end_game_bonus(&set, &p)
        };
        assert_eq!(bonus(0), 0, "no major improvement -> no bonus");
        assert_eq!(bonus(3), 0, "doubled total 4 < 5");
        assert_eq!(bonus(4), 1, "doubled total 5 -> +1");
        assert_eq!(bonus(5), 1, "doubled total 6 still +1");
        assert_eq!(bonus(6), 2, "doubled total 7 -> +2");
        assert_eq!(bonus(7), 2, "doubled total 8 still +2");
        assert_eq!(bonus(8), 3, "doubled total 9 -> +3");
        assert_eq!(bonus(9), 3, "doubled total 10 still +3");
        assert_eq!(bonus(10), 4, "doubled total 11 -> +4");
    }

    /// The spec's [2, 3, 4] example: double the lowest score (2 -> 4), the
    /// doubled total is 11, and the stacked thresholds pay 4 points.
    #[test]
    fn b153_doubles_the_lowest_score_and_pays_the_top_threshold() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("B153".into());
        p.improvements.extend([
            Improvement::ClayOven,
            Improvement::StoneOven,
            Improvement::Well,
        ]);
        assert_eq!(end_game_bonus(&set, &p), 4, "doubled total 11 -> +4");
    }

    /// A tied lowest score is doubled exactly once (BGA plays it that way:
    /// Well + ClayOven + Basketry scores 4+4+2 = 10 -> +3, not 12 -> +4).
    #[test]
    fn b153_doubles_only_one_of_the_tied_lowest_scores() {
        let set = CardSet {
            definitions: verified_band_b_cards(),
        };
        let mut p = Player::new(0, 0);
        p.played_cards.push("B153".into());
        p.improvements.extend([
            Improvement::Well,
            Improvement::ClayOven,
            Improvement::Basketry,
        ]);
        assert_eq!(end_game_bonus(&set, &p), 3, "4+4+2 = 10 -> +3");
        // Two tied lowest scores: ClayOven 2 + Basketry 2 tie, StoneOven 3;
        // only one of the tied 2s doubles, so 4+2+3 = 9 -> +3. Doubling
        // both tied scores would give 4+4+3 = 11 -> +4, which BGA does not
        // pay (the wiki play report for 4+2+2 paid 3, not 4).
        let mut q = Player::new(0, 0);
        q.played_cards.push("B153".into());
        q.improvements.extend([
            Improvement::ClayOven,
            Improvement::Basketry,
            Improvement::StoneOven,
        ]);
        assert_eq!(end_game_bonus(&set, &q), 3, "4+2+3 = 9 -> +3");
        // A lone 1-point major doubles to 2: Well 4 + Fireplace2 -> 4+2 = 6,
        // which clears only the 5 threshold (6 < 7).
        let mut r = Player::new(0, 0);
        r.played_cards.push("B153".into());
        r.improvements
            .extend([Improvement::Well, Improvement::Fireplace2]);
        assert_eq!(end_game_bonus(&set, &r), 1, "4+2 = 6 -> +1");
    }
}

/// Test module for verified_b_afterworker_simple_cards (B017, B047, B051, B060, B063).
#[cfg(test)]
mod verified_b_afterworker_simple_tests {
    use super::super::*;
    use crate::card_events::dispatch_after_worker;
    use crate::cards::CardState;
    use std::collections::BTreeMap;

    fn set() -> CardSet {
        CardSet {
            definitions: verified_b_afterworker_simple_cards(),
        }
    }

    /// Drive the after-worker dispatch and resolve the optional decision
    /// (B017/B060/B063) with the caller's choice. The contract's
    /// "use the card" cases accept; the declined cases pass `false`.
    /// Returns the public accumulation map so the B017 accumulation-return
    /// contract (space_wood_delta +2) can be asserted.
    fn after_worker_simple_choice(
        set: &CardSet,
        player: &mut Player,
        space: Space,
        confirmed: bool,
    ) -> BTreeMap<Space, u32> {
        let mut cards = CardState::default();
        let mut players = vec![player.clone()];
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        let mut well = BTreeMap::new();
        let mut mirror = BTreeMap::new();
        let mut accumulations = BTreeMap::new();
        dispatch_after_worker(
            set,
            0,
            space,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            1,
            &used_spaces,
            &mut accumulations,
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut mirror,
        )
        .unwrap();
        if let Some(decision) = pending {
            crate::card_events::apply_after_worker_optional_decision(
                &decision,
                &mut players[0],
                &mut well,
                &mut mirror,
                Some(&mut accumulations),
                1,
                confirmed,
            )
            .expect("optional decision resolution applies");
        }
        *player = players[0].clone();
        accumulations
    }

    fn after_worker_simple(set: &CardSet, player: &mut Player, space: Space) {
        // The accepted case: the owner chooses to use the optional effect.
        after_worker_simple_choice(set, player, space, true);
    }

    // B017 森林鋤 (Forest Plow): cost 1 wood; after using Forest/Copse/Grove,
    // may pay 2 wood to plow 1 field. The wood stays on the accumulation space.
    #[test]
    fn b017_plows_field_on_forest_when_paying_wood() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 3; // 2 for effect + 1 to have some
        let accums = after_worker_simple_choice(&s, &mut p, Space::Forest, true);
        assert_eq!(p.supply.wood, 1); // 3 - 2 paid
                                      // Field should be plowed
        assert!(p.farm.iter().any(|c| matches!(c.tile, Tile::Field { .. })));
        // The paid wood stays on the used accumulation space (spec
        // `return_to: accumulation_space`; contract B017-normal
        // space_wood_delta +2). Only the 2 paid wood returns.
        assert_eq!(accums.get(&Space::Forest).copied(), Some(2));
    }

    // Contract data/bga-card-tests/B017.json B017-normal: setup wood 2 ->
    // wood_delta -2, fields_delta +1, space_wood_delta +2.
    #[test]
    fn b017_paying_wood_returns_it_to_the_accumulation_space() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 2;
        let accums = after_worker_simple_choice(&s, &mut p, Space::Forest, true);
        assert_eq!(p.supply.wood, 0, "wood_delta -2");
        let fields = p
            .farm
            .iter()
            .filter(|c| matches!(c.tile, Tile::Field { .. }))
            .count();
        assert_eq!(fields, 1, "fields_delta +1");
        assert_eq!(
            accums.get(&Space::Forest).copied(),
            Some(2),
            "space_wood_delta +2"
        );
    }

    // Contract B017-declined: setup wood 2, use_card false -> wood_delta 0.
    // The owner can pay but declines: nothing is paid, plowed or accumulated.
    #[test]
    fn b017_declined_when_paying_is_possible() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 2;
        let accums = after_worker_simple_choice(&s, &mut p, Space::Forest, false);
        assert_eq!(p.supply.wood, 2, "a decline pays nothing");
        assert!(
            !p.farm.iter().any(|c| matches!(c.tile, Tile::Field { .. })),
            "a decline plows nothing"
        );
        assert!(
            accums.get(&Space::Forest).copied().unwrap_or(0) == 0,
            "a decline accumulates nothing"
        );
    }

    #[test]
    fn b017_plows_field_on_copse_when_paying_wood() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 3;
        after_worker_simple(&s, &mut p, Space::Copse);
        assert_eq!(p.supply.wood, 1);
        assert!(p.farm.iter().any(|c| matches!(c.tile, Tile::Field { .. })));
    }

    #[test]
    fn b017_plows_field_on_grove_when_paying_wood() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 3;
        after_worker_simple(&s, &mut p, Space::Grove);
        assert_eq!(p.supply.wood, 1);
        assert!(p.farm.iter().any(|c| matches!(c.tile, Tile::Field { .. })));
    }

    #[test]
    fn b017_declined_when_insufficient_wood() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 1; // Not enough to pay 2
        after_worker_simple(&s, &mut p, Space::Forest);
        assert_eq!(p.supply.wood, 1); // No wood spent
                                      // No field plowed
        assert!(!p.farm.iter().any(|c| matches!(c.tile, Tile::Field { .. })));
    }

    #[test]
    fn b017_not_triggered_by_other_spaces() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B017".into());
        p.supply.wood = 5;
        after_worker_simple(&s, &mut p, Space::ClayPit);
        after_worker_simple(&s, &mut p, Space::Fishing);
        after_worker_simple(&s, &mut p, Space::DayLaborer);
        assert_eq!(p.supply.wood, 5); // No wood spent
        assert!(!p.farm.iter().any(|c| matches!(c.tile, Tile::Field { .. })));
    }

    #[test]
    fn b017_cost_and_points() {
        let s = set();
        let card = s.definition("B017").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
    }

    // B047 ニシン鍋 (Herring Pot): cost 1 clay; after using Fishing,
    // place 1 food on each of the next 3 round spaces.
    #[test]
    fn b047_schedules_food_on_next_three_rounds() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B047".into());
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![p];
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5, // round 5
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        // Food scheduled on rounds 6, 7, 8
        assert_eq!(well.get(&6), Some(&vec![0]));
        assert_eq!(well.get(&7), Some(&vec![0]));
        assert_eq!(well.get(&8), Some(&vec![0]));
        assert_eq!(well.get(&9), None);
    }

    #[test]
    fn b047_twice_accumulates() {
        let s = set();
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("B047".into());
        let mut used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        // First Fishing at round 5
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        used_spaces.clear();
        // Second Fishing at round 5 (same round)
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        // Each use schedules 3 food, so 2 uses = 6 food on each of rounds 6,7,8
        assert_eq!(well.get(&6), Some(&vec![0, 0]));
        assert_eq!(well.get(&7), Some(&vec![0, 0]));
        assert_eq!(well.get(&8), Some(&vec![0, 0]));
    }

    // P1-2 (PR #425 Round 1 review): a card-scheduled well reservation must
    // land in BOTH `delayed_well` (the source of truth) and the public mirror
    // `well_food_schedule` (PR #265 Finding A — buy_improvement writes both),
    // and `start_round` must grant the real food from the source of truth.
    #[test]
    fn b047_schedule_writes_both_well_maps_and_grants_food() {
        use crate::model::{Config, Game};

        let s = set();
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("B047".into());
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        let mut well = BTreeMap::new();
        let mut mirror = BTreeMap::new();
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5, // round 5 -> reserves rounds 6, 7, 8
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut mirror,
        )
        .unwrap();

        // Both maps carry the identical reservation (no public-only divergence).
        for round in [6u8, 7, 8] {
            assert_eq!(
                well.get(&round),
                Some(&vec![0]),
                "delayed_well must hold round {round}"
            );
            assert_eq!(
                mirror.get(&round),
                Some(&vec![0]),
                "public mirror must hold round {round}"
            );
        }
        assert_eq!(well, mirror, "mirror must equal the source of truth");

        // start_round grants the real food from the source of truth and consumes
        // both entries (same pattern as
        // mcts::tests::start_round_preserves_well_and_card_limits).
        let mut game = Game::with_cards(
            Config {
                seed: 7,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        game.public.round = 5;
        game.delayed_well = well;
        game.public.well_food_schedule = mirror;
        let food_before = game.public.players[0].supply.food;
        game.start_round().expect("start_round must succeed");
        assert_eq!(game.public.round, 6);
        assert_eq!(
            game.public.players[0].supply.food,
            food_before + 1,
            "the reserved well food must be granted when round 6 starts"
        );
        assert!(!game.delayed_well.contains_key(&6));
        assert!(!game.public.well_food_schedule.contains_key(&6));
    }
    #[test]
    fn b047_not_triggered_by_other_spaces() {
        let s = set();
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("B047".into());
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_after_worker(
            &s,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        assert!(well.is_empty());
    }

    #[test]
    fn b047_cost_and_points() {
        let s = set();
        let card = s.definition("B047").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Clay, 1)]);
        assert_eq!(card.points, 0);
    }

    // B051 採掘シャベル (Digging Spade): cost 1 wood; playable from round 7;
    // after using Clay Pit, gain food equal to the number of wild boars.
    #[test]
    fn b051_gains_food_per_boar_on_clay_pit() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B051".into());
        // Add 3 wild boars
        p.pastures.push(crate::model::Pasture {
            id: 0,
            cells: vec![2, 3, 4],
            animal: Some(Animal::Boar),
            quantity: 3,
        });
        after_worker_simple(&s, &mut p, Space::ClayPit);
        assert_eq!(p.supply.food, 3); // 3 boars * 1
    }

    #[test]
    fn b051_zero_when_no_boars() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B051".into());
        after_worker_simple(&s, &mut p, Space::ClayPit);
        assert_eq!(p.supply.food, 0);
    }

    #[test]
    fn b051_round_prerequisite_enforced() {
        let s = set();
        let card = s.definition("B051").unwrap();
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        // Round 6 -> cannot play (minimum round 7)
        assert!(!can_play(&s, card, &p, 6, 0));
        // Round 7 -> can play
        assert!(can_play(&s, card, &p, 7, 0));
    }

    #[test]
    fn b051_too_early_cannot_play() {
        let s = set();
        let card = s.definition("B051").unwrap();
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        // Round 1 -> cannot play
        assert!(!can_play(&s, card, &p, 1, 0));
    }

    #[test]
    fn b051_not_triggered_by_other_spaces() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B051".into());
        p.pastures.push(crate::model::Pasture {
            id: 0,
            cells: vec![2, 3, 4],
            animal: Some(Animal::Boar),
            quantity: 2,
        });
        after_worker_simple(&s, &mut p, Space::Forest);
        after_worker_simple(&s, &mut p, Space::Fishing);
        assert_eq!(p.supply.food, 0);
    }

    #[test]
    fn b051_cost_and_points() {
        let s = set();
        let card = s.definition("B051").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 1)]);
        assert_eq!(card.points, 0);
    }

    // B060 仕込み水 (Brewing Water): free cost; after using Fishing,
    // may pay 1 grain to place 1 food on each of the next 6 round spaces.
    #[test]
    fn b060_schedules_food_on_next_six_rounds_when_paying_grain() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B060".into());
        p.supply.grain = 1;
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![p];
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5, // round 5
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        if let Some(decision) = pending {
            // B060 is optional: the accepted contract cases confirm the pay.
            crate::card_events::apply_after_worker_optional_decision(
                &decision,
                &mut players[0],
                &mut well,
                &mut std::collections::BTreeMap::new(),
                None,
                5,
                true,
            )
            .expect("optional decision resolution applies");
        }
        // Food scheduled on rounds 6, 7, 8, 9, 10, 11
        for r in 6..=11 {
            assert_eq!(well.get(&r), Some(&vec![0]), "round {r} should have food");
        }
        assert_eq!(well.get(&12), None);
        // Grain should be spent
        assert_eq!(players[0].supply.grain, 0);
    }

    // Contract data/bga-card-tests/B060.json B060-decline: setup grain 1,
    // pay_grain false -> rounds_with_food 0. The owner can pay but declines:
    // nothing is paid and nothing is scheduled.
    #[test]
    fn b060_declined_when_paying_is_possible() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B060".into());
        p.supply.grain = 1;
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![p];
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        let decision = pending.expect("an affordable owner gets the choice");
        crate::card_events::apply_after_worker_optional_decision(
            &decision,
            &mut players[0],
            &mut well,
            &mut std::collections::BTreeMap::new(),
            None,
            5,
            false, // declined
        )
        .expect("optional decision resolution applies");
        assert!(well.is_empty(), "rounds_with_food 0");
        assert_eq!(players[0].supply.grain, 1, "a decline pays nothing");
    }

    #[test]
    fn b060_declined_when_insufficient_grain() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B060".into());
        p.supply.grain = 0; // Cannot pay
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![p];
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        assert!(well.is_empty(), "no food scheduled when grain not paid");
    }

    #[test]
    fn b060_late_round_schedules_only_up_to_14() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B060".into());
        p.supply.grain = 1;
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![p];
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        // Round 12 -> schedules on 13, 14 only (15+ don't exist)
        dispatch_after_worker(
            &s,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            12,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        if let Some(decision) = pending {
            // B060 is optional: the accepted contract cases confirm the pay.
            crate::card_events::apply_after_worker_optional_decision(
                &decision,
                &mut players[0],
                &mut well,
                &mut std::collections::BTreeMap::new(),
                None,
                12,
                true,
            )
            .expect("optional decision resolution applies");
        }
        assert_eq!(well.get(&13), Some(&vec![0]));
        assert_eq!(well.get(&14), Some(&vec![0]));
        assert_eq!(well.get(&15), None);
        assert_eq!(well.get(&16), None);
    }

    #[test]
    fn b060_not_triggered_by_other_spaces() {
        let s = set();
        let mut well = BTreeMap::new();
        let mut cards = CardState::default();
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("B060".into());
        players[0].supply.grain = 1;
        let used_spaces = std::collections::BTreeSet::new();
        let mut event_budget = 100;
        let mut card_uses = BTreeMap::new();
        let card_use_limits = BTreeMap::new();
        let mut pending = None;
        dispatch_after_worker(
            &s,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut vec![],
            &mut well,
            5,
            &used_spaces,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            true,
            &mut std::collections::BTreeMap::new(),
        )
        .unwrap();
        assert!(well.is_empty());
    }

    #[test]
    fn b060_cost_and_points() {
        let s = set();
        let card = s.definition("B060").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert!(card.cost.is_empty());
        assert_eq!(card.points, 0);
    }

    // B063 味見 (Tasting): cost 2 wood; 1 point; after using Lessons,
    // optionally exchange 1 grain for 4 food.
    #[test]
    fn b063_exchanges_grain_for_food_on_lessons() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B063".into());
        p.supply.grain = 1;
        after_worker_simple(&s, &mut p, Space::Lessons);
        assert_eq!(p.supply.grain, 0);
        assert_eq!(p.supply.food, 4);
    }

    // Contract data/bga-card-tests/B063.json B063-decline: setup grain 1,
    // exchange false -> grain/food delta 0. The owner can exchange but
    // declines: both goods stay put.
    #[test]
    fn b063_declined_when_exchange_is_possible() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B063".into());
        p.supply.grain = 1;
        after_worker_simple_choice(&s, &mut p, Space::Lessons, false);
        assert_eq!(p.supply.grain, 1, "a decline gives no grain");
        assert_eq!(p.supply.food, 0, "a decline receives no food");
    }

    #[test]
    fn b063_declined_when_no_grain() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B063".into());
        p.supply.grain = 0;
        after_worker_simple(&s, &mut p, Space::Lessons);
        assert_eq!(p.supply.grain, 0);
        assert_eq!(p.supply.food, 0);
    }

    #[test]
    fn b063_not_triggered_by_other_spaces() {
        let s = set();
        let mut p = Player::new(0, 0);
        p.played_cards.push("B063".into());
        p.supply.grain = 5;
        after_worker_simple(&s, &mut p, Space::Forest);
        after_worker_simple(&s, &mut p, Space::ClayPit);
        after_worker_simple(&s, &mut p, Space::Fishing);
        assert_eq!(p.supply.grain, 5);
        assert_eq!(p.supply.food, 0);
    }

    #[test]
    fn b063_cost_and_points() {
        let s = set();
        let card = s.definition("B063").unwrap();
        assert_eq!(card.kind, CardKind::MinorImprovement);
        assert_eq!(card.cost, vec![(Good::Wood, 2)]);
        assert_eq!(card.points, 1);
    }

    #[test]
    fn afterworker_simple_cards_absent_from_two_player_preset() {
        let preset = verified_basic_card_set(2).unwrap();
        for id in ["B017", "B047", "B051", "B060", "B063"] {
            assert!(
                preset.definition(id).is_none(),
                "{id} must stay out of the A/B two-player preset"
            );
        }
    }
}

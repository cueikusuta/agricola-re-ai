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
        // B132 地主 (Estate Master): cost 0; at every harvest field phase, if
        // every farmyard space is used, gain 1 bonus point per vegetable
        // harvested this field phase (agricola.cloudfree.jp ?p=3125).
        // Eligible at 3+ players per the BGA wiki; no engine prerequisite is
        // expressed on the printed card.
        CardDefinition {
            use_limit: None,
            id: "B132".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::HarvestFieldGainPointsIfFarmyardFull {
                points_per_vegetable: 1,
            }],
            // Merge integration (PR #471 R1): `post_play` became a required
            // `CardDefinition` field on main; B132 地主 is an Occupation that
            // stays in the owner's played area (it does not pass left), so the
            // default PlayedArea preserves the reviewed semantics exactly.
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
                can_play(&set, card, &p, 0),
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
            can_play(&set, card, &p, 0),
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
            can_play(&set, card, &p, 0),
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

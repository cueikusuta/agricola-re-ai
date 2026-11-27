//! A-deck after_worker_space simple gain cards (A015/A018/A021/A023/A024).
//! Ported from the deleted monolithic `src/cards.rs` into the module
//! layout main adopted; definitions and helpers are unchanged apart from
//! the `post_play` field main's `CardDefinition` now requires.
use super::{CardDefinition, CardEffect, CardKind, CardRequirement};
use crate::cards::PostPlayDestination;
use crate::model::{Good, Space};

/// Aデッキ after_worker_space 単純獲得カード 5枚 (A015/A018/A021/A023/A024)。
/// 全て MinorImprovement で、ワーカー配置後に特定スペース使用時に発動する効果を持つ。
pub fn verified_after_worker_space_cards() -> Vec<CardDefinition> {
    fn minor(
        id: &str,
        cost: Vec<(Good, u32)>,
        points: i32,
        effects: Vec<CardEffect>,
    ) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            post_play: PostPlayDestination::PlayedArea,
            id: id.into(),
            kind: CardKind::MinorImprovement,
            cost,
            points,
            effects,
        }
    }

    vec![
        // A015 大工の手斧 (Carpenter's Axe)
        // Cost: 1 wood. After using Forest/Copse/Grove with 7+ wood in supply: optionally build 1 stable for 1 wood.
        minor(
            "A015",
            vec![(Good::Wood, 1)],
            0,
            vec![CardEffect::AfterWorkerSpaceBuildStable {
                spaces: vec![Space::Forest, Space::Copse, Space::Grove],
                min_wood: 7,
                wood_cost: 1,
            }],
        ),
        // A018 車輪鋤 (Wheel Plow)
        // Cost: 2 wood. Once per game: when first person of round uses Farmland or Cultivation, plow 2 fields.
        minor(
            "A018",
            vec![(Good::Wood, 2)],
            0,
            vec![CardEffect::AfterWorkerSpacePlowFields {
                spaces: vec![Space::Farmland, Space::Cultivation],
                amount: 2,
            }],
        ),
        // A021 家族向け住宅 (Family Friendly Home)
        // Cost: 0. Prerequisite: 1+ occupation. After using Build Rooms while rooms > people: Family Growth + 1 food.
        // Note: Build Rooms is Space::FarmExpansion (not FamilyGrowthWithRoom).
        minor(
            "A021",
            vec![],
            0,
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                CardEffect::AfterWorkerSpaceFamilyGrowth {
                    space: Space::FarmExpansion,
                    food_amount: 1,
                },
            ],
        ),
        // A023 採石事業所 (Stone Company)
        // Cost: 2 clay + 1 reed. After using Western/Eastern Quarry: Improvement action that must spend 1+ stone.
        minor(
            "A023",
            vec![(Good::Clay, 2), (Good::Reed, 1)],
            1,
            vec![CardEffect::AfterWorkerSpaceImprovementAction {
                spaces: vec![Space::WesternQuarry, Space::EasternQuarry],
                must_spend_stone: true,
            }],
        ),
        // A024 脱穀板 (Threshing Board)
        // Cost: 1 wood. Prerequisite: 2+ occupations. After using Farmland or Cultivation: additional Bake Bread action.
        minor(
            "A024",
            vec![(Good::Wood, 1)],
            1,
            vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(2),
                },
                CardEffect::AfterWorkerSpaceBakeBread {
                    spaces: vec![Space::Farmland, Space::Cultivation],
                },
            ],
        ),
    ]
}

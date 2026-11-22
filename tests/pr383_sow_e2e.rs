//! End-to-end tests for the sow-family cards A065 ペレット種子 and E050 野草,
//! driven exclusively through production entry points (`Game::apply`,
//! `Game::apply_with_plans` → `Game::resolve_pending_and_resume`).
//!
//! These replace the synthetic-dispatch tests that fed `Detail::Sow` into
//! `Space::GrainSeeds` / `Space::VegetableSeeds` (PR #383 review finding (d)):
//! those spaces take `require_none(detail)` in `execute_work`, so a
//! `Detail::Sow` payload there can never occur in production and the old tests
//! were proving behaviour on an input the engine refuses. The reachable sow
//! sites are the revealed round-card spaces `GrainUtilization` (Sow /
//! SowDetailed) and `Cultivation` (Cultivate); every test below enters through
//! one of them.
//!
//! What is pinned here:
//! - (c) A065 fires BEFORE the core sow: a player with zero grain still sows,
//!   because the gain lands in time to be spent by that same action.
//! - (b) A065's printed prerequisite 畑3 blocks play below three field tiles.
//! - (e) E050 does not trigger, spend budget, or count a use on a plow-only
//!   Farmland action (the leak), while still firing for every real sow.

use agricola_re_ai::cards::{
    can_play, verified_band_a_cards, verified_band_e_cards, CardDefinition, CardEffect, CardKind,
    CardRequirement, CardSet,
};
use agricola_re_ai::engine::{Action, Detail, Plant};
use agricola_re_ai::model::{Config, Crop, Game, RoundCard, Space, Tile, DEFAULT_EVENT_BUDGET};

/// A/B + E band definitions padded to the 14-per-kind minimum `validate(2)`
/// enforces, with A065 and E050 forced into seat 0's hand.
fn band_set() -> CardSet {
    let mut definitions = verified_band_a_cards();
    definitions.extend(verified_band_e_cards());
    // A155 is a four-player card; drop it so the set validates at two players.
    definitions.retain(|c| {
        !c.effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(min),
                } if *min > 2
            )
        })
    });
    let count = |defs: &[CardDefinition], kind| defs.iter().filter(|c| c.kind == kind).count();
    let mut i = 0;
    while count(&definitions, CardKind::Occupation) < 14
        || count(&definitions, CardKind::MinorImprovement) < 14
    {
        let kind = if count(&definitions, CardKind::Occupation) < 14 {
            CardKind::Occupation
        } else {
            CardKind::MinorImprovement
        };
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("FILLER_{i}"),
            kind,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        i += 1;
    }
    CardSet { definitions }
}

/// Player 0 owns the given cards, `fields` empty field tiles and the given
/// supplies. Round cards that open the sow spaces are revealed explicitly
/// because `start_round` only turns one card per round.
fn game_with(played: &[&str], fields: &[usize], grain: u32, vegetable: u32) -> Game {
    let set = band_set();
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::MinorImprovement && !played.contains(&c.id.as_str()))
        .map(|c| c.id.clone())
        .collect();
    // `from_hands` demands exactly 7 occupations + 7 minor improvements per
    // seat, so the owned cards occupy part of seat 0's minor allowance.
    let mut hand0: Vec<String> = occs.iter().take(7).cloned().collect();
    hand0.extend(played.iter().map(|id| (*id).to_string()));
    hand0.extend(minors.iter().take(7 - played.len()).cloned());
    let mut hand1: Vec<String> = occs.iter().skip(7).take(7).cloned().collect();
    hand1.extend(minors.iter().skip(7 - played.len()).take(7).cloned());
    assert_eq!(hand0.len(), 14, "seat 0 hand");
    assert_eq!(hand1.len(), 14, "seat 1 hand");
    let mut game = Game::with_cards_and_hands(
        Config {
            seed: 7,
            players: 2,
            additional_action_space_enabled: false,
        },
        set,
        vec![hand0, hand1],
    )
    .expect("game creation");
    game.public.active_player = 0;
    let p = &mut game.public.players[0];
    p.played_cards
        .extend(played.iter().map(|id| (*id).to_string()));
    p.supply.grain = grain;
    p.supply.vegetable = vegetable;
    for &cell in fields {
        p.farm[cell].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
    }
    let mut deck = std::mem::take(&mut game.hidden_round_deck);
    deck.retain(|c| !matches!(c, RoundCard::GrainUtilization | RoundCard::Cultivation));
    for card in [RoundCard::GrainUtilization, RoundCard::Cultivation] {
        if !game.public.revealed.contains(&card) {
            game.public.revealed.push(card);
        }
    }
    game.hidden_round_deck = deck;
    game
}

fn sow_grain_at(cell: usize) -> Detail {
    Detail::SowDetailed {
        plantings: vec![Plant {
            cell,
            crop: Crop::Grain,
        }],
        baking: vec![],
    }
}

/// (c) 発火タイミング: A065's spec trigger is `before_sow`. With zero grain the
/// sow must still succeed, because the gained grain arrives before `sow()`
/// spends it. Under the previous post-sow reading this exact apply returned
/// `Err("insufficient grain")`.
#[test]
fn a065_gain_lands_before_the_core_sow_so_zero_grain_still_sows() {
    let mut game = game_with(&["A065"], &[1, 2, 3], 0, 0);
    assert_eq!(game.public.players[0].supply.grain, 0);
    game.apply(Action {
        space: Space::GrainUtilization,
        detail: sow_grain_at(1),
    })
    .expect("A065 must supply the sowing grain before the sow pays it");
    let p = &game.public.players[0];
    assert_eq!(p.supply.grain, 0, "gained 1 then spent 1 on the sowing");
    assert!(
        matches!(
            p.farm[1].tile,
            Tile::Field {
                crop: Some(Crop::Grain),
                ..
            }
        ),
        "the field must actually be sown"
    );
}

/// (d) 合成入力の実行確定: the seed spaces are NOT sow spaces in production.
/// `execute_work` gives `Space::GrainSeeds` / `Space::VegetableSeeds` a
/// `require_none(detail)` guard, so the sow payload the removed tests fed them
/// is refused through `Game::apply` itself — that is what made those tests
/// synthetic (PR #383 review finding (d)). The reachable sow sites are
/// `GrainUtilization` (Sow / SowDetailed) and `Cultivation` (Cultivate), which
/// is all this file drives.
#[test]
fn seed_spaces_refuse_sow_payloads_in_production() {
    for space in [Space::GrainSeeds, Space::VegetableSeeds] {
        let mut game = game_with(&[], &[1, 2, 3], 3, 0);
        let err = game
            .apply(Action {
                space,
                detail: sow_grain_at(1),
            })
            .expect_err("the seed spaces must refuse a sow payload");
        assert!(
            !err.is_empty(),
            "the refusal must carry a reason, got {err:?}"
        );
    }
}

/// (c) 対照: without A065 the same zero-grain action is impossible, which is
/// what makes the test above evidence for the before-sow timing rather than
/// for a lenient sow rule.
#[test]
fn without_a065_the_same_zero_grain_action_is_rejected() {
    let mut game = game_with(&[], &[1, 2, 3], 0, 0);
    let err = game
        .apply(Action {
            space: Space::GrainUtilization,
            detail: sow_grain_at(1),
        })
        .expect_err("no grain and no A065: the sow must fail");
    assert!(
        err.to_lowercase().contains("grain"),
        "the refusal must be about the missing grain, got {err:?}"
    );
}

/// (c) GrainUtilization can also sow and bake in one action. A065 carries the
/// `sow_unconditional` condition, so a sow that also bakes is NOT an
/// unconditional sow and must not grant grain.
#[test]
fn a065_does_not_fire_when_the_sow_also_bakes() {
    let mut game = game_with(&["A065"], &[1, 2, 3], 3, 0);
    game.public.players[0]
        .improvements
        .push(agricola_re_ai::model::Improvement::Fireplace2);
    let food_before = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::GrainUtilization,
        detail: Detail::SowDetailed {
            plantings: vec![Plant {
                cell: 1,
                crop: Crop::Grain,
            }],
            baking: vec![agricola_re_ai::engine::BakeAllocation {
                improvement: agricola_re_ai::model::Improvement::Fireplace2,
                grain: 1,
            }],
        },
    })
    .expect("sow + bake is a legal GrainUtilization action");
    let p = &game.public.players[0];
    // 3 grain: -1 sown -1 baked, +2 food. A065 must not add its grain.
    assert_eq!(p.supply.grain, 1, "no A065 grain for a conditional sow");
    assert_eq!(
        p.supply.food,
        food_before + 2,
        "the bake itself still resolves"
    );
}

/// (b) 畑3: the printed 条件 row (card wiki p=3360) requires three field tiles.
/// The prerequisite must gate play, not merely be recorded.
#[test]
fn a065_requires_three_fields_to_play() {
    let set = band_set();
    let card = set.definition("A065").expect("A065 in the A band");
    let requirement = card
        .effects
        .iter()
        .find_map(|e| match e {
            CardEffect::Prerequisite {
                rule: rule @ CardRequirement::MinimumFields(_),
            } => Some(rule.clone()),
            _ => None,
        })
        .expect("A065 must carry the 畑3 prerequisite");
    assert_eq!(requirement, CardRequirement::MinimumFields(3));

    let mut player = agricola_re_ai::model::Player::new(0, 0);
    for cell in [1, 2] {
        player.farm[cell].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
    }
    assert!(
        !can_play(&set, card, &player, 0),
        "two fields must not satisfy 畑3"
    );
    player.farm[3].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    assert!(can_play(&set, card, &player, 0), "three fields satisfy 畑3");
}

/// (e) 塞ぎ: a plow-only Farmland action sows nothing, so E050 must not fire,
/// must not consume the event budget and must not count a card use. Before the
/// fix the condition only required `sow_detail.is_some()`, which a
/// `Detail::Plow` payload satisfied: the handler then granted 0 food but the
/// budget and use count had already been spent.
#[test]
fn e050_does_not_leak_on_a_plow_only_farmland_action() {
    let mut game = game_with(&["E050"], &[1, 2, 3], 0, 0);
    assert!(matches!(game.public.players[0].farm[6].tile, Tile::Empty));
    let food_before = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 6 },
    })
    .expect("plow-only Farmland is legal");
    let p = &game.public.players[0];
    assert_eq!(
        p.supply.food, food_before,
        "E050 granted nothing and must not"
    );
    assert!(
        matches!(p.farm[6].tile, Tile::Field { crop: None, .. }),
        "the plow still happened"
    );
    assert_eq!(
        game.public.card_uses.get("E050").copied().unwrap_or(0),
        0,
        "E050 must not record a use on a plow-only action"
    );
    assert_eq!(
        game.event_budget, DEFAULT_EVENT_BUDGET,
        "no trigger may consume budget on a plow-only action"
    );
}

/// (e) 陽性対照: a real sow still fires E050 once per distinct crop, and the
/// use/budget accounting moves with it.
#[test]
fn e050_fires_once_per_distinct_crop_sown() {
    let mut game = game_with(&["E050"], &[1, 2, 3], 1, 1);
    let food_before = game.public.players[0].supply.food;
    // Cultivation without a plow: sow grain and vegetable in one action.
    game.apply(Action {
        space: Space::Cultivation,
        detail: Detail::Cultivate {
            plow: None,
            plantings: vec![
                Plant {
                    cell: 1,
                    crop: Crop::Grain,
                },
                Plant {
                    cell: 2,
                    crop: Crop::Vegetable,
                },
            ],
        },
    })
    .expect("Cultivation sow of two crops is legal");
    let p = &game.public.players[0];
    assert_eq!(
        p.supply.food,
        food_before + 2,
        "two distinct crops -> two food"
    );
    assert_eq!(
        game.public.card_uses.get("E050").copied().unwrap_or(0),
        1,
        "one action, one use"
    );
}

/// (e) 境界: the same crop sown twice counts as one distinct crop.
#[test]
fn e050_counts_distinct_crop_types_not_plantings() {
    let mut game = game_with(&["E050"], &[1, 2, 3], 2, 0);
    let food_before = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::Cultivation,
        detail: Detail::Cultivate {
            plow: None,
            plantings: vec![
                Plant {
                    cell: 1,
                    crop: Crop::Grain,
                },
                Plant {
                    cell: 2,
                    crop: Crop::Grain,
                },
            ],
        },
    })
    .expect("Cultivation sow of two grain fields is legal");
    assert_eq!(game.public.players[0].supply.food, food_before + 1);
}

/// A065 and E050 together on one sow action: the grain must arrive before the
/// sow (A065) and the food after it (E050), and both must be charged to the
/// same action.
#[test]
fn a065_and_e050_resolve_on_opposite_sides_of_the_sow() {
    let mut game = game_with(&["A065", "E050"], &[1, 2, 3], 0, 0);
    let food_before = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::GrainUtilization,
        detail: sow_grain_at(1),
    })
    .expect("A065 supplies the grain, E050 rewards the sowing");
    let p = &game.public.players[0];
    assert_eq!(p.supply.grain, 0, "A065 gained 1, the sow spent 1");
    assert_eq!(
        p.supply.food,
        food_before + 1,
        "E050 counted 1 distinct crop"
    );
    assert_eq!(game.public.card_uses.get("A065").copied().unwrap_or(0), 1);
    assert_eq!(game.public.card_uses.get("E050").copied().unwrap_or(0), 1);
}

/// (e) 境界（複合アクション）: a Cultivation that BOTH plows and sows is a real
/// sow. E050 still counts only the distinct sown crops (the plow contributes
/// none), A065 still grants its grain before the sow spends it, and the plowed
/// field joins the farm. This is the positive counterpart of the plow-only leak
/// test: what separates the two is the plantings, not the space — the same
/// Cultivation space, one payload sows and the other does not.
#[test]
fn e050_fires_and_a065_gains_when_cultivation_plows_and_sows_together() {
    // Grain still starts at zero (A065 must supply it), but the vegetable in
    // hand is already there: only the grain does the before/after work here.
    let mut game = game_with(&["A065", "E050"], &[1, 2, 3], 0, 1);
    assert!(
        matches!(game.public.players[0].farm[6].tile, Tile::Empty),
        "cell 6 starts empty"
    );
    let food_before = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::Cultivation,
        detail: Detail::Cultivate {
            plow: Some(6),
            plantings: vec![
                Plant {
                    cell: 1,
                    crop: Crop::Grain,
                },
                Plant {
                    cell: 2,
                    crop: Crop::Vegetable,
                },
            ],
        },
    })
    .expect("A065 supplies the grain for a plow-and-sow Cultivation");
    let p = &game.public.players[0];
    assert!(
        matches!(p.farm[6].tile, Tile::Field { crop: None, .. }),
        "the plow part of the combined action resolved"
    );
    assert_eq!(p.supply.grain, 0, "A065 gained 1, the sow spent 1");
    assert_eq!(
        p.supply.food,
        food_before + 2,
        "two distinct sown crops -> two food; the plow contributes none"
    );
    assert_eq!(game.public.card_uses.get("A065").copied().unwrap_or(0), 1);
    assert_eq!(game.public.card_uses.get("E050").copied().unwrap_or(0), 1);
}

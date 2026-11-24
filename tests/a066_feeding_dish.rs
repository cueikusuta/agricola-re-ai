//! A066 餌皿 (Feeding Dish): after using a sheep/pig/cattle market while
//! already keeping that animal type, gain 1 grain.
//!
//! Dispatcher tests drive `dispatch_after_worker` directly with a
//! pre-populated pasture to model `snapshot = after_event`: animals kept
//! before the market count (and animals taken at that same market resolve
//! before the after-worker scan, so they count too).
//! The engine test plays the card through `Game::apply` and pins
//! save/load/replay so the grant is never doubled on replay.

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{
    verified_band_a_cards, verified_basic_card_set, CardDefinition, CardKind, CardSet,
};
use agricola_re_ai::engine::{Action, AnimalPlan, Detail};
use agricola_re_ai::model::{Animal, Config, Game, Pasture, Player, RoundCard, Space};
use std::collections::{BTreeMap, BTreeSet};

fn set_with_a066() -> CardSet {
    CardSet {
        definitions: verified_band_a_cards()
            .into_iter()
            .filter(|card| card.id == "A066")
            .collect(),
    }
}

fn a066_definition() -> CardDefinition {
    set_with_a066()
        .definitions
        .into_iter()
        .next()
        .expect("A066 must exist in band A")
}

fn dispatch(
    set: &CardSet,
    played_by: &[usize],
    actor: usize,
    space: Space,
    mut players: Vec<Player>,
) -> Vec<Player> {
    let mut cards = Default::default();
    for &owner in played_by {
        players[owner].played_cards.push("A066".to_string());
    }
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 100;
    let mut uses = BTreeMap::new();
    let mut uses_game = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    let used = BTreeSet::new();
    let accumulations = BTreeMap::new();
    dispatch_after_worker(
        set,
        actor,
        space,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        actor,
        &used,
        &accumulations,
        &mut budget,
        &mut uses,
        &mut uses_game,
        &limits,
        &mut pending,
        false,
    )
    .expect("dispatch_after_worker must succeed");
    players
}

fn pasture_with(animal: Animal, quantity: u32) -> Pasture {
    Pasture {
        id: 0,
        cells: vec![],
        animal: Some(animal),
        quantity,
    }
}

#[test]
fn a066_normal_sheep_market_grants_one_grain() {
    // A066-normal: sheep 保有時に SheepMarket 使用で grain+1。
    let set = set_with_a066();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    players[0].pastures.push(pasture_with(Animal::Sheep, 1));
    let players = dispatch(&set, &[0], 0, Space::SheepMarket, players);
    assert_eq!(
        players[0].supply.grain, 1,
        "sheep owner gains 1 grain at SheepMarket"
    );
    assert_eq!(players[1].supply.grain, 0, "no card, no grant");
}

#[test]
fn a066_not_triggered_without_animals() {
    // A066-not-triggered: animals なしでは grain+0。
    let set = set_with_a066();
    let players = vec![Player::new(0, 0)];
    let players = dispatch(&set, &[0], 0, Space::SheepMarket, players);
    assert_eq!(players[0].supply.grain, 0, "no animals means no grant");
}

#[test]
fn a066_not_triggered_by_other_animal_types() {
    // 別種だけの保有では発火しない。
    let set = set_with_a066();

    let mut players = vec![Player::new(0, 0)];
    players[0].pastures.push(pasture_with(Animal::Boar, 1));
    let players = dispatch(&set, &[0], 0, Space::SheepMarket, players);
    assert_eq!(
        players[0].supply.grain, 0,
        "boar does not trigger at SheepMarket"
    );

    let mut players = vec![Player::new(0, 0)];
    players[0].pastures.push(pasture_with(Animal::Sheep, 1));
    let players = dispatch(&set, &[0], 0, Space::PigMarket, players);
    assert_eq!(
        players[0].supply.grain, 0,
        "sheep does not trigger at PigMarket"
    );

    let mut players = vec![Player::new(0, 0)];
    players[0].pastures.push(pasture_with(Animal::Sheep, 1));
    let players = dispatch(&set, &[0], 0, Space::CattleMarket, players);
    assert_eq!(
        players[0].supply.grain, 0,
        "sheep does not trigger at CattleMarket"
    );
}

#[test]
fn a066_each_market_matches_its_own_animal() {
    // 3市場の Space→Animal 対応。
    let set = set_with_a066();

    let mut players = vec![Player::new(0, 0)];
    players[0].pastures.push(pasture_with(Animal::Sheep, 1));
    let players = dispatch(&set, &[0], 0, Space::SheepMarket, players);
    assert_eq!(players[0].supply.grain, 1);

    let mut players = vec![Player::new(0, 0)];
    players[0].pastures.push(pasture_with(Animal::Boar, 3));
    let players = dispatch(&set, &[0], 0, Space::PigMarket, players);
    assert_eq!(
        players[0].supply.grain, 1,
        "any positive count grants exactly 1"
    );

    let mut players = vec![Player::new(0, 0)];
    players[0].pastures.push(pasture_with(Animal::Cattle, 1));
    let players = dispatch(&set, &[0], 0, Space::CattleMarket, players);
    assert_eq!(players[0].supply.grain, 1);
}

#[test]
fn a066_only_actor_and_owner_benefit() {
    // owner 以外への付与なし。
    let set = set_with_a066();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    players[0].pastures.push(pasture_with(Animal::Sheep, 1));
    // Both players own A066, but only the actor's own played cards are scanned.
    let players = dispatch(&set, &[0, 1], 1, Space::SheepMarket, players);
    assert_eq!(players[0].supply.grain, 0, "non-actor owner gets nothing");
    assert_eq!(
        players[1].supply.grain, 0,
        "actor without the animal gets nothing"
    );

    // Cards are already registered; re-registering would double the effect.
    let players = dispatch(&set, &[], 0, Space::SheepMarket, players);
    assert_eq!(players[0].supply.grain, 1, "actor owner gains 1 grain");
    assert_eq!(players[1].supply.grain, 0, "other player gets nothing");
}

#[test]
fn a066_only_matching_space_fires() {
    // 無関係なアクションスペースでは発火しない。
    let set = set_with_a066();
    for space in [Space::Forest, Space::DayLaborer, Space::GrainSeeds] {
        let mut players = vec![Player::new(0, 0)];
        players[0].pastures.push(pasture_with(Animal::Sheep, 1));
        let players = dispatch(&set, &[0], 0, space, players);
        assert_eq!(
            players[0].supply.grain, 0,
            "{space:?} must not trigger A066"
        );
    }
}

#[test]
fn a066_engine_play_market_replay_no_double_grant() {
    // 初取得後の after_event 保有判定と resume/replay での二重付与なし。
    // ラウンド1：seat0 が Forest で木を得、MeetingPlace で A066 をプレイ（コスト wood1）。
    // ラウンド2：revealed に SheepMarket が残り、animal_accumulations が +1 されるので、
    // seat0 が SheepMarket でペットとして羊を1頭迎える。after_event スナップショットに
    // 同じアクションで迎えた羊が含まれるため grain+1。
    let mut defs: Vec<CardDefinition> = verified_basic_card_set(2)
        .unwrap()
        .definitions
        .into_iter()
        .filter(|c| c.id != "A066")
        .collect();
    let occs: Vec<String> = defs
        .iter()
        .filter(|c| matches!(c.kind, CardKind::Occupation))
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = defs
        .iter()
        .filter(|c| matches!(c.kind, CardKind::MinorImprovement) && c.id != "A066")
        .map(|c| c.id.clone())
        .collect();
    defs.push(a066_definition());
    let card_set = CardSet { definitions: defs };
    let mut hands = vec![occs[..7].to_vec(), occs[7..14].to_vec()];
    hands[0].push("A066".to_string());
    hands[0].extend(minors[0..6].iter().cloned());
    hands[1].extend(minors[7..14].iter().cloned());
    let config = Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: false,
    };
    let mut game = Game::with_cards_and_hands_with_start_player(config, card_set.clone(), hands, 0)
        .expect("a066 engine setup");

    fn apply0(g: &mut Game, space: Space, detail: Detail) {
        let player = g.public.active_player;
        let round = g.public.round;
        g.apply(Action { space, detail }).unwrap_or_else(|e| {
            panic!("round {round} player {player} {space:?} must be legal: {e}")
        });
    }

    // ラウンド1: 2アクションずつで終了し、ラウンド2へ自動進む。
    apply0(&mut game, Space::Forest, Detail::None); // seat0: 木（A066 コスト分）
    apply0(&mut game, Space::ReedBank, Detail::None); // seat1
    apply0(
        &mut game,
        Space::MeetingPlace,
        Detail::PlayMinor {
            card_id: "A066".to_string(),
        },
    ); // seat0: A066 プレイ
    apply0(&mut game, Space::ClayPit, Detail::None); // seat1: ReedBank is once-per-round, use ClayPit instead
                                                     // ラウンド2へ（全ワーカー配置でラウンド自動進行。start_round は二重に進めない）。
    assert_eq!(game.public.round, 2);
    assert_eq!(game.public.active_player, 0);
    assert_eq!(game.public.players[0].played_cards, &["A066"]);
    assert_eq!(game.public.players[0].supply.grain, 0);

    // ラウンド2：SheepMarket で羊を迎える（ラウンド開始蓄積が2頭）。after-event
    // スナップショットに羊が含まれるので grain+1。
    apply0(
        &mut game,
        Space::SheepMarket,
        Detail::Animals(AnimalPlan {
            animal: Animal::Sheep,
            to_pastures: vec![],
            to_stables: vec![],
            pet: true,
            cook: 0,
            discard: 1,
        }),
    );
    assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 1);
    assert_eq!(
        game.public.players[0].supply.grain, 1,
        "newly acquired animal at the same market triggers A066"
    );
    assert_eq!(
        game.public.players[1].supply.grain, 0,
        "only the card owner benefits"
    );

    // resume/replay: 二重付与なし。
    let final_hash = game.state_hash();
    let final_grain = game.public.players[0].supply.grain;
    let loaded = Game::load_json(&game.save_json().expect("save")).expect("load");
    assert_eq!(loaded.state_hash(), final_hash);
    assert_eq!(loaded.public.players[0].supply.grain, final_grain);

    let replayed = Game::replay_with_setup_and_start_player(
        loaded.config.clone(),
        (*loaded.card_set).clone(),
        loaded.log.initial_hands.clone(),
        loaded.log.initial_start_player_override,
        &loaded.log.events,
    )
    .expect("logged actions replay");
    assert_eq!(replayed.state_hash(), final_hash);
    assert_eq!(
        replayed.public.players[0].supply.grain, final_grain,
        "replay must not double-grant A066"
    );
    assert_eq!(replayed.public.players[1].supply.grain, 0);
}

#[test]
fn a066_round_card_is_sheep_market_seed0() {
    // seed 0 のラウンド1 revealed カードは SheepMarket（エンジンテストの前提）。
    let game = Game::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: false,
    });
    assert_eq!(game.public.revealed, vec![RoundCard::SheepMarket]);
}

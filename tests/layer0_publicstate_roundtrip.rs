//! Layer 0 正本化の受け入れテスト (設計判断 1 / 決定1)。
//!
//! 1. `save_json` → `load_json` → `replay` の往復で `state_hash()` が完全一致する。
//! 2. `Observation` 射影が情報損失なし: 観測の `public` が正本 `PublicState` と
//!    完全一致し、観測から再構築した探索用ゲーム (MCTS `sample_game`) の公開状態も
//!    元の正本と完全一致する。
//! 3. `state_hash` は v5 契約どおり集約欄 (`well_food_schedule`, `card_uses`) の
//!    値を含まず、`integrity_hash` がそれらを検出する。

use agricola_re_ai::cards::{
    verified_basic_card_set, CardDefinition, CardEffect, CardKind, PostPlayDestination,
};
use agricola_re_ai::mcts::sample_game;
use agricola_re_ai::model::{Config, Game, Good, Space};
use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, RandomPolicy, SpacePolicy};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// `seed` で初期化したゲームを `steps` 手ランダム進行する (通常 apply 経路のみ)。
fn play_random_actions(game: &mut Game, steps: usize, rng_seed: u64) {
    let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
    for _ in 0..steps {
        if game.public.finished || game.has_pending() {
            break;
        }
        let id = game.public.active_player;
        let obs = game.observation(id).expect("valid player");
        let legal = game.legal_spaces();
        if legal.is_empty() {
            break;
        }
        let Some(space) = RandomPolicy.select(&obs, &legal, &mut rng) else {
            break;
        };
        let Some(action) = CardChoiceDetails.produce(&obs, space) else {
            break;
        };
        if game.apply(action).is_err() {
            break;
        }
    }
}

fn replay_from_loaded(loaded: &Game) -> Result<Game, String> {
    let config = loaded.config.clone();
    let card_set = (*loaded.card_set).clone();
    let hands = loaded.log.initial_hands.clone();
    let starter = loaded.log.initial_start_player_override;
    let events = loaded.log.events.clone();
    match (hands, starter) {
        (Some(hands), Some(starter)) => Game::replay_with_setup_and_start_player(
            config,
            card_set,
            Some(hands),
            Some(starter),
            &events,
        ),
        (Some(hands), None) => Game::replay_with_setup(config, card_set, Some(hands), &events),
        (None, None) => Game::replay_with_cards(config, card_set, &events),
        (None, Some(_)) => Err("start player override requires draft hands".into()),
    }
}

/// 受け入れ条件 1: save → load → replay の往復で state_hash 完全一致。
/// カードなし/カードあり × 2/3/4 人 × 複数 seed で確認する。
/// (同梱の検証済みカードプールは 2 人用のみのため、3/4 人はカードなしで往復する。)
#[test]
fn save_load_replay_roundtrip_matches_state_hash() {
    for (players, seed, cards) in [
        (2usize, 3u64, false),
        (2, 11, true),
        (3, 5, false),
        (4, 7, false),
        (4, 21, false),
    ] {
        let config = Config {
            players,
            seed,
            additional_action_space_enabled: false,
        };
        let mut game = if cards {
            Game::with_cards(config.clone(), verified_basic_card_set(players).unwrap()).unwrap()
        } else {
            Game::new(config.clone())
        };
        play_random_actions(&mut game, 9, seed ^ 0xA5A5);

        let original_hash = game.state_hash();
        let original_public = game.public.clone();
        let json = game.save_json().expect("save");

        let loaded = Game::load_json(&json).expect("load");
        assert_eq!(
            loaded.state_hash(),
            original_hash,
            "save→load で state_hash が不一致 (players={players}, seed={seed}, cards={cards})"
        );
        assert_eq!(
            loaded.public, original_public,
            "save→load で正本 PublicState が不一致"
        );
        loaded.validate().expect("loaded game must validate");

        let replayed = replay_from_loaded(&loaded).expect("replay");
        assert_eq!(
            replayed.state_hash(),
            original_hash,
            "replay で state_hash が不一致 (players={players}, seed={seed}, cards={cards})"
        );
        assert_eq!(replayed.public, original_public);

        // 観測も往復で同一 (Observation は正本の射影 + 自分手札のみ)。
        let id = game.public.active_player;
        assert_eq!(
            loaded.observation(id).unwrap(),
            game.observation(id).unwrap()
        );
    }
}

/// 受け入れ条件 2: Observation 射影の情報損失なし。
/// 正本に公開状態 (井戸予約・カード使用台帳) を注入し、観測と
/// 観測から再構築した探索用ゲームの公開状態が元の正本と完全一致することを確認する。
#[test]
fn observation_projection_preserves_public_state() {
    let mut game = Game::with_cards(
        Config {
            seed: 77,
            ..Config::default()
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();
    play_random_actions(&mut game, 3, 0x51EED);

    // 井戸購入と同等の公開状態 (予約: round 3 の全ラウンド分) を正本へ直接注入。
    game.delayed_well.entry(3).or_default().push(0);
    game.public.well_food_schedule.entry(3).or_default().push(0);
    // カード使用台帳も正本へ直接注入 (プレイは公開行動なので公開状態)。
    game.public.card_uses.insert("TEST_CARD".into(), 2);

    let obs = game.observation(0).expect("valid player");
    assert_eq!(
        obs.public, game.public,
        "Observation.public は正本 PublicState と完全一致しなければならない"
    );

    // 観測から探索用ゲームを再構築し、公開状態が完全一致する (非公開欄は要らない)。
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF);
    let reconstructed = sample_game(&obs, &mut rng).expect("reconstruction must succeed");
    assert_eq!(
        reconstructed.public, game.public,
        "Observation から再構築したゲームの公開状態が正本と不一致 — 射影が情報を損失している"
    );
    assert_eq!(
        reconstructed.delayed_well, game.delayed_well,
        "公開ミラー (well_food_schedule) から delayed_well が復元できる"
    );

    // 旧観測ミラー欄は Observation に存在しない (単一正本)。use limits は
    // card_definitions から導出できる。
    let json = serde_json::to_string(&obs).unwrap();
    assert!(
        !json.contains("\"card_use_limits\""),
        "Observation に card_use_limits ミラーが残存している"
    );
}

/// 受け入れ条件 3 (v5 ピン): 集約欄の値は `state_hash` に反映されず、
/// `integrity_hash` には反映される。
#[test]
fn state_hash_ignores_aggregated_public_mirrors() {
    let mut game = Game::new(Config {
        players: 2,
        seed: 9,
        additional_action_space_enabled: false,
    });
    let base_state_hash = game.state_hash();
    let base_integrity = game.integrity_hash();

    game.public.card_uses.insert("A001".into(), 1);
    game.public.well_food_schedule.entry(4).or_default().push(1);

    assert_eq!(
        game.state_hash(),
        base_state_hash,
        "集約欄の変更が state_hash に影響した — v5 ピンが壊れている"
    );
    assert_ne!(
        game.integrity_hash(),
        base_integrity,
        "集約欄の変更が integrity_hash に反映されていない"
    );
}

/// 集約欄を含む PublicState が serde 往復で保存・復元できる
/// (旧セーブは `#[serde(default)]` で欠落欄を既定値として読む)。
#[test]
fn public_state_with_aggregated_fields_round_trips_through_serde() {
    let mut game = Game::with_cards(
        Config {
            seed: 15,
            ..Config::default()
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();
    game.public.card_uses.insert("A002".into(), 3);
    game.public.well_food_schedule.entry(6).or_default().push(0);

    let text = serde_json::to_string(&game.public).unwrap();
    let back: agricola_re_ai::model::PublicState = serde_json::from_str(&text).unwrap();
    assert_eq!(back, game.public);
    assert_eq!(back.card_uses.get("A002"), Some(&3));
    assert_eq!(back.well_food_schedule.get(&6), Some(&vec![0]));

    // 旧形式 (集約欄なし) の JSON も既定値で読める。
    let legacy: agricola_re_ai::model::PublicState =
        serde_json::from_str("{\"round\":2,\"active_player\":0,\"start_player\":0,\"used\":[],\"revealed\":[],\"accumulations\":{},\"animal_accumulations\":{},\"players\":[],\"improvement_market\":[],\"work_quota\":[2,2],\"workers_placed\":[0,0],\"additional_action_space_enabled\":false,\"finished\":false}").unwrap();
    assert!(legacy.card_uses.is_empty());
    assert!(legacy.well_food_schedule.is_empty());
    let _ = (CardKind::Occupation, Good::Wood, Space::DayLaborer); // keep imports honest for cfg variations
}

/// 合成 use_limit カード: 台帳が正本に載り、保存復元と観測射影を通る。
#[test]
fn use_limit_ledger_survives_save_load_as_canonical_state() {
    let mut set = verified_basic_card_set(2).unwrap();
    set.definitions.push(CardDefinition {
        id: "TEST_LIMITED".into(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::AfterWorkerGain {
            space: Space::DayLaborer,
            good: Good::Wood,
            amount: 1,
        }],
        use_limit: Some(1),
        post_play: PostPlayDestination::default(),
    });
    // 明示手配で TEST_LIMITED を player 0 の手札に置き、実際のエンジン経路で
    // カードをプレイしてから DayLaborer で使用する (Lessons → 相手が別枠 →
    // DayLaborer の順。スペースは各ラウンド 1 回のみ)。
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation && c.id != "TEST_LIMITED")
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == agricola_re_ai::cards::CardKind::MinorImprovement)
        .map(|c| c.id.clone())
        .collect();
    let first_hand = [
        vec!["TEST_LIMITED".to_string()],
        occupations.clone().into_iter().take(6).collect(),
        minors.iter().take(7).cloned().collect(),
    ]
    .concat();
    let second_hand = [
        occupations.into_iter().skip(6).take(7).collect::<Vec<_>>(),
        minors.iter().skip(7).take(7).cloned().collect(),
    ]
    .concat();
    let mut game = Game::with_cards_and_hands_with_start_player(
        Config {
            seed: 31,
            ..Config::default()
        },
        set,
        vec![first_hand, second_hand],
        0,
    )
    .unwrap();

    use agricola_re_ai::engine::Detail;
    game.apply(agricola_re_ai::engine::Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "TEST_LIMITED".into(),
        },
    })
    .unwrap_or_else(|e| panic!("lessons must accept the occupation: {e}"));
    game.apply(agricola_re_ai::engine::Action {
        space: Space::GrainSeeds,
        detail: Detail::None,
    })
    .unwrap_or_else(|e| panic!("grain seeds must be playable: {e}"));
    game.apply(agricola_re_ai::engine::Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    })
    .unwrap_or_else(|e| panic!("day laborer must be playable at start: {e}"));
    assert_eq!(
        game.public.card_uses.get("TEST_LIMITED"),
        Some(&1),
        "use_limit カードの使用が正本台帳に記録される"
    );

    let hash = game.state_hash();
    let loaded = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(
        loaded.public.card_uses, game.public.card_uses,
        "台帳は保存復元で正本ごと一致する"
    );

    // 観測射影にも台帳が乗り、再構築ゲームで一致する。
    let obs = game.observation(0).unwrap();
    assert_eq!(obs.public.card_uses.get("TEST_LIMITED"), Some(&1));
    let mut rng = ChaCha8Rng::seed_from_u64(0xBEEF);
    let reconstructed = sample_game(&obs, &mut rng).expect("reconstruction must succeed");
    assert_eq!(reconstructed.public.card_uses, game.public.card_uses);
    assert_eq!(reconstructed.card_use_limits.get("TEST_LIMITED"), Some(&1));
}

/// `start_round` に pending decision が残っているときは Err を返し、
/// 状態 (state_hash) を変更しないことを確認する (P1: ガード順序の回帰テスト)。
#[test]
fn start_round_rejects_pending_and_preserves_state_hash() {
    let mut game = Game::new(Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    });
    // Game::new() は内部で start_round() を呼ぶため、初期状態は round=1。
    // さらに 1 回 start_round() を呼んで round=2 にする。
    game.start_round().expect("second start_round must succeed");

    // state_hash を取得
    let hash_before = game.state_hash();

    // 手動で pending decision を注入 (ChooseTriggerOrder を模倣)
    use agricola_re_ai::card_events::TriggerPhase;
    use agricola_re_ai::pending::{PendingDecision, PendingKind};
    let decision = PendingDecision::new(
        1, // id
        0, // player
        PendingKind::ChooseTriggerOrder {
            phase: TriggerPhase::After,
            triggers: vec![("fake_card".into(), 0, "fake_trigger".into())],
        },
        hash_before.clone(), // state_hash_before は現在の hash と一致
        serde_json::json!({}),
    );
    game.pending = Some(decision);

    // start_round が Err を返すことを確認
    let err = game
        .start_round()
        .expect_err("start_round must fail when pending exists");
    assert!(
        err.contains("pending decision must be resolved before round start"),
        "error message should mention pending decision: {err}"
    );

    // state_hash が変わっていないことを確認 (部分的な状態変更が起きていない)
    assert_eq!(
        game.state_hash(),
        hash_before,
        "state_hash must remain unchanged when start_round fails due to pending decision"
    );

    // 追加で確認: round が進んでいない (2 のまま)、revealed が増えていない、used/card_uses が clear されていない
    assert_eq!(game.public.round, 2, "round must not advance on error");
    assert_eq!(
        game.public.revealed.len(),
        2,
        "revealed must not grow on error"
    );
    // used と card_uses は start_round 成功時に clear されるため、失敗時は元のまま
    // (初期状態では空なので変化なし。ここでは hash 一致で十分)
}

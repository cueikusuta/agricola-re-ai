## 概要

Aデッキの `after_worker_space` ミニ改善 5 枚（A015 大工の手斧 / A018 車輪鋤 / A021 家族向け住宅 / A023 採石事業所 / A024 脱穀板）を `AfterWorkerSpace*` 語彙＋**typed な pending/resolution** で実装する。独立レビュー round 1（review card `t_50a7e8b7`、対象 HEAD `6d639e7`）の REQUEST CHANGES 4 件（P1-1〜P1-4）をすべて修正し、`origin/main` をマージで統合した上で再レビューを依頼する。

## 実装内容（カードごと）

正本は各 `data/bga-card-specs/*.json`（BGA 参照 URL は spec の `bga.references`）。

| カード | spec 費用 | 発火スペース | 効果 | spec 条件/頻度 |
|---|---|---|---|---|
| A015 大工の手斧 | wood 1 | Forest/Copse/Grove | 木材 7 以上あれば木材 1 で厩 1 建設（任意） | `spaces`=wood 系、`min_wood: 7` |
| A018 車輪鋤 | wood 2 | Farmland/Cultivation | 畑 2 枚を耕す | `first_person_of_round` + `once_per_game` |
| A021 家族向け住宅 | wood 1 | Build Rooms（= `Space::FarmExpansion`） | 部屋数 > 家族人数なら Family Growth + food 1 | `rooms_exceed_people` |
| A023 採石事業所 | stone 2 | Quarry/Day Labor 等 stone 系 | 石を 1 以上支払う改進歩アクション | `must_spend_stone` |
| A024 脱穀板 | stone 1 | Farmland/Cultivation | Bake Bread アクションを付与 | — |

### 変更ファイル

- `src/cards.rs`: `CardEffect::AfterWorkerSpace{BuildStable,PlowFields,FamilyGrowth,ImprovementAction,BakeBread}` 追加、`verified_after_worker_space_cards()` 追加、`public_tag` / gain-amount 検証 / policy 語彙対応
- `src/card_events.rs`: `EventKind::WorkerPlaced` ハンドラ 5 本、`DispatchContext` に `start_player` / `card_uses_game` を追加し first_person_of_round・once_per_game を実状態で判定、pending 生成
- `src/pending.rs`: **typed** `PendingKind::{BuildStable,PlowFields,ImprovementAction,BakeBread}` と `PendingResolution` 同種、`default_resolution()`（headless/AI 用既定解決）、`ImprovementOption`（typed ID + 合法コスト）、`resolve_*` の状態遷移・合法性検証
- `src/engine.rs`: `commit_pause_decision()`（pause commit 時の state hash 再付印・decision id 採番・work completion 構築）、headless 既定解決の反復継続
- `src/model.rs`: resume 時の `(PendingKind, PendingResolution)` マッチ（stable 建設・plow・improvement action・bake bread の実適用）、save/load・再演 hash
- `src/registry.rs`: イベントインデックス登録、`verified_after_worker_space_cards()` を default registry 組込
- `src/policy.rs`: ヒューリスティック価値（AI 評価）

## 曖昧なルールと解釈（決定内容）

1. **A023「石を1以上支払う」の候補**: `available_improvements` を整形文字列ではなく typed `ImprovementOption { card_id, label, cost }` で表現し、市場在庫にあり**実際に石を支払える**合法コストの改進歩のみ候補にする（支払えないものは pending 生成条件から除外）。`format!("{:?}", imp)` 文字列は廃止。
2. **A024 の `has_baking` / `grain > 0` ゲート（spec conditions に明示なし）**: **残す**。理由: Bake Bread はオーブン系改進歩がなければ実行不能で、小麦 0 ではパン焼きの結果が何も生じないため、効果の前提として必要（解釈）。spec にない追加条件で発火を潰す方向ではない（gate は「実行可能性」の判定）。レビューで異議があれば Issue 分離する。
3. **A018 の「ラウンド最初の配置者」**: ラウンド開始プレイヤー（`start_player`）が最初に worker を配置したアクション、として実装。`DispatchContext.start_player` を全 dispatch 経路で供給。
4. **A021 の発火スペース**: エンジンの Build Rooms = `Space::FarmExpansion`（`Detail::Build { rooms, stables }`）。`Space::FamilyGrowthWithRoom` では発火しない。

## 独立レビュー（round 1）指摘への対応

レビュー対象 HEAD `6d639e720aa313eabe001dc0024d0760a9920a36`（review card `t_50a7e8b7`）。

### P1-1: A021 の発火スペースが仕様と逆（`Space::FamilyGrowthWithRoom` 誤指定）

- 修正: `src/card_events.rs` の A021 ハンドラを `Space::FarmExpansion` 発火に変更（spec `worker_space_used.space = "build_rooms"`、エンジンでは Build Rooms = `Space::FarmExpansion`）。
- 回帰テスト: `a021_build_rooms_rooms_exceed_people_triggers_family_growth_and_food`（正常系）、`a021_rooms_not_exceed_people_does_not_trigger`（否定系）、`a021_family_growth_space_does_not_trigger`（FamilyGrowthWithRoom で不発）、`a021_non_build_space_does_not_trigger`。

### P1-2: A018 `first_person_of_round` / `once_per_game` 未実装

- 修正: `DispatchContext` に `start_player` とゲーム全域の `card_uses_game` カウンタを追加。A018 ハンドラは (a) `start_player == actor` かつラウンド中最初の配置、(b) `card_uses_game["A018"] == 0` を**実状態で検証**してから pending を生成。発火時にカウンタを増加。
- 回帰テスト: `a018_first_person_triggers_plow_pending`（初回）、`a018_non_starter_does_not_trigger`（非該当者）、`a018_once_per_game_second_use_does_not_trigger`（2回目不発）、`a018_resolve_plow_fields_actually_plows_two_fields`（2畑耕作の実結果）、`a018_too_few_plow_sites_does_not_trigger`、`a018_does_not_trigger_on_non_farmland_space`。

### P1-3: Custom pending の no-op（効果未適用・headless 破壊）

旧実装の `(PendingKind::Custom, PendingResolution::Custom)` は `Ok(())` のみで、`PendingResolution::Custom` を生成する実装が存在せず、handler 直接 pending の `state_hash_before = ""` により resolve は hash mismatch で失敗していた。

- 修正: 4 カードすべてを **typed pending/resolution** へ移行（上記 `src/pending.rs`）。engine の `commit_pause_decision` が commit 時に hash を再付印し decision id を採番。`default_resolution()` により AI/headless（`apply_with_harvest_policy` / `play_to_end_with_budget` 経路）が既定解決で継続できる。
- 回帰テスト（resolution 後の実状態まで断言）:
  - A015: `a015_resolve_build_stable_actually_builds_and_pays_wood`（厩 +1、木材 -1）、`a015_decline_leaves_farm_and_wood_untouched`、`a015_does_not_trigger_below_7_wood`
  - A018: `a018_resolve_plow_fields_actually_plows_two_fields`（耕地 2 枚）
  - A023: `a023_resolve_improvement_action_plays_stone_improvement`（市場/手持ち改進歩 + 石支払い）、`a023_no_payable_stone_improvement_does_not_trigger`（支払可能候補がなければ不発）
  - A024: `a024_resolve_bake_bread_actually_bakes`（小麦→パン）、`a024_no_grain_does_not_trigger` / `a024_no_baking_improvement_does_not_trigger`
  - headless 継続: `a015_headless_apply_builds_stable_and_continues` / `a018_headless_apply_plows_two_fields` / `a023_headless_apply_plays_stone_improvement` / `a024_headless_apply_bakes_bread`（`apply_with_harvest_policy` の paused エラーを廃止）
  - pending 中の `game.legal_spaces()` 空・`apply` の paused エラー・tamper された hash/counter の load 時 reject: `a015_pending_blocks_apply_and_spaces_empty` / `pending_state_hash_mismatch_is_rejected` / `tampered_once_per_game_counter_is_rejected_on_load`
  - 保存・復元・再演: `a015_logged_play_survives_save_load_and_replay` / `a018_logged_once_per_game_survives_save_load_and_replay`

### P1-4: テスト契約が実装を検査していない

`src/cards.rs` の legacy `after_worker` は `AfterWorkerGain` のみ処理し `AfterWorkerSpace*` は無視するため、`is_ok()` のみのテストは新実装を削除しても通る状態だった。

- 修正: `tests/ch03_afterworker_space_cards.rs` の Step 2 テストを**実際の engine/dispatch 経路で効果完了まで断言する行動テスト**へ置き換えた（テスト名と実内容を一致させる）。指摘された 4 テスト（`a015_triggers_on_wood_spaces_with_7plus_wood` / `a018_triggers_on_farmland_cultivation` / `a021_only_triggers_on_build_rooms` / `a023_only_triggers_on_quarry_spaces`）と `mutation_verification::mutation_a015_wood_requirement_detected` を削除・再構成。
- 破壊確認（mutation）: 各ハンドラの発火スペース・しきい値・条件を意図的に壊した場合に当該テストが赤になることを個別に確認済み（wood 要件、first_person 条件、once_per_game 条件、FarmExpansion 指定、stone 支払い可能性、grain/oven ゲート）。

## 追加テスト一覧（`tests/ch03_afterworker_space_cards.rs`、45 件）

dispatch レベル正常系/否定系/回数制限/解決後状態/headless 継続/save-load-replay:
`ch03_cards_reachable_via_default_registry`, `ch03_cards_registry_event_index_populated`,
`ch03_cards_are_implemented_but_not_officially_verified`, `pool_excludes_all_ch03_cards_due_to_unverified_status`,
A015×6、A018×7、A021×4、A023×5、A024×5、pending/hash×2、headless×4、save/replay×2。

## main 統合

`mergeable: CONFLICTING` だったため `origin/main` を **merge（force push なし）** で統合。衝突は `src/card_events.rs`・`src/cards.rs`・`src/engine.rs`・`src/model.rs`・`src/policy.rs`・`src/registry.rs` の 6 ファイル。方針: origin/main 側の新規要素（round-start 語彙、`accumulations`、`c066_take_food_instead`、A118/A145 テスト改修、P1-3 `previous_player` 事前キャプチャ修正）をすべて保存し、本 PR の `AfterWorkerSpace*` 語彙・typed pending・`verified_after_worker_space_cards()` 組込を重ね合わせた。main 側で default registry の review 構築が「eligibility = player-count Yes/No」方式に変わったため、`ch03_cards_are_implemented_but_not_officially_verified` は merged セマンティクス（review_status = Unverified / eligibility = Yes@2,3,4）に更新。テストファイルの Step 2 テスト以外に本 PR 差分の改変はない。

## ゲート確認（ローカル実測）

- `cargo fmt` / `cargo fmt --check` ✅
- `cargo clippy --all-targets --all-features --locked -- -D warnings` ✅
- `cargo test --lib` ✅（469 passed）
- `cargo test --test ch03_afterworker_space_cards` ✅（45 passed）
- `cargo test` 全体 ✅（integration targets すべて green）
- Python gates: `scripts/validate_bga_card_specs.py` ✅ / `scripts/card_verification_blockers.py` ✅ / `scripts/validate_card_catalog.py` ✅
- プール数・MCTS digest は未変更のため `m6_verified_pool` / digest テストは追加実行対象外（CI の通常 `cargo test` で包含）。

## Actions CI（最新 HEAD）

- HEAD SHA: `fbe821f` — 全必須チェック緑（下表、Actions 実測）
| チェック名 | 結論 | Run URL |
|---|---|---|
| CI required (aggregate) | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746662/job/137311177086 |
| cargo test | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746662/job/137310505503 |
| rustfmt | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746662/job/137310505506 |
| clippy | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746662/job/137310505530 |
| M1/M2/M7 seeded performance | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746662/job/137310505467 |
| Detect changed files | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746662/job/137310457232 |
| Pipeline gates (spec/catalog/blockers) | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746776/job/137310457491 |
| Python unittest discovery | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746761/job/137310457561 |
| test (aggregate) | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45055746652/job/137310456709 |


**全必須ゲート緑**（aggregate / shard は skipped: 通常 PR では大量検証ジョブは実行対象外）。ローカル測定値ではなく Actions の実測値・URL を記録。

### 追記: docs コミット HEAD の再実行確認

CI 証跡行の追加のみを行った HEAD `a3d0175`（fbe821f + PR_BODY.md のみ）でも同一ゲートを再実行し全必須チェック緑:

- CI required: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633105/job/137313971791
- cargo test: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633105/job/137313404133
- rustfmt: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633105/job/137313404079
- clippy: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633105/job/137313404073
- M1/M2/M7 seeded performance: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633105/job/137313404166
- Pipeline gates (spec/catalog/blockers): https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633071/job/137313203467
- Python unittest discovery: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633058/job/137313210886
- test (aggregate): https://github.com/cueikusuta/agricola-re-ai/actions/runs/45056633037/job/137313203103

### 追記2: origin/main 再統合 merge HEAD の再実行確認

origin/main が前回 merge 以降に前進した（#434 D065 / #435 C116 / #436 B102 / #437 B073 / #440 replay gate-2 fix）ため、**merge（force push なし）**で再統合。衝突は `src/policy.rs` の 1 ファイルのみで、本 PR の `AfterWorkerSpace*` policy arm と main の D065 `HarvestFieldGainIfGrainHarvestedAtLeast` arm を union 解決した（上記「main 統合」節および fbe821f/a3d0175 の記録は歴代証跡として保持）。merge コミット HEAD `d6b63f87ddc8ec6006b7c528b08e606760ef4490` で全必須チェックを再実行し緑:

- CI required: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896796/job/137483514105
- cargo test: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896796/job/137482847829
- rustfmt: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896796/job/137482847791
- clippy: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896796/job/137482848024
- M1/M2/M7 seeded performance: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896796/job/137482848069
- Pipeline gates (spec/catalog/blockers): https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896887/job/137482765130
- Python unittest discovery: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896857/job/137482764806
- test (aggregate): https://github.com/cueikusuta/agricola-re-ai/actions/runs/45110896971/job/137482764957

merge 後も mergeable: MERGEABLE。ローカルでも `cargo test --lib` 480 passed・`cargo test --test ch03_afterworker_space_cards` 45 passed・main 側改修ファイルのテスト（b073 / ch_b102 / harvest_feeding / p1_2_player_projection）・Python gates 3 本を確認。A021 発火スペースの mutation probe（`Space::FarmExpansion` → `Space::FamilyGrowthWithRoom` に戻すと `a021_build_rooms_rooms_exceed_people_triggers_family_growth_and_food` と `mutation_verification::mutation_a021_space_binding_detected` が赤になること）もマージ後ツリーで再確認済み。

### 追記3: 本文更新 docs HEAD の再実行確認

上記追記2の追加のみを行った HEAD `9232886d0d52a400385b071eb1830417e8f67648`（branch tip）でも同一ゲートを再実行し全必須チェック緑（mergeable: MERGEABLE）:

- CI required: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111563991/job/137485521166
- cargo test: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111563991/job/137484930413
- rustfmt: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111563991/job/137484930363
- clippy: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111563991/job/137484930333
- M1/M2/M7 seeded performance: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111563991/job/137484930365
- Pipeline gates (spec/catalog/blockers): https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111564056/job/137484852532
- Python unittest discovery: https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111564087/job/137484853104
- test (aggregate): https://github.com/cueikusuta/agricola-re-ai/actions/runs/45111564021/job/137484852141

## 宣言

- 1 Issue = 1 目的 = 1 ブランチ = 1 PR。変更は A015/A018/A021/A023/A024 の実装とレビュー対応に限定。
- 公式 PDF・カード画像・カード原文データは含めない。記述は自作コードと自分の言葉のみ。
- AI 観測に未公開カード・内部 seed は露出しない（pending 情報は `Observation` から排除）。

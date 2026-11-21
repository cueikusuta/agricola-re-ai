## 概要

帯内未実装 4 枚（A109/A118/C088/C115）＋ 実装済み記録漏れ 2 枚（A088/A110）について、
外部レビュー（Arena 読み取り専用）の指摘 6 件を実コード・PR・ボードで照合し、
記録の補完と A109 の実装を行った。

## Step 1: 指摘 6 件の照合結果

| ID | (a) src/実装 | (b) spec JSON | (c) 保留記録 | (d) 台帳/一覧表 | 判定 | 根拠 |
|---|---|---|---|---|---|---|
| **A109 小商人** | **あり** (本 PR) | あり (`bga_checked`, `not_started`) | なし | あり (ledger, catalog) | **実在** | `git grep A109 src/` = 0; `data/bga-card-specs/A109.json` 存在・status確認; ledger行あり; catalog行あり (難易度3) |
| **A118 樹木庭師** | なし | あり (`bga_checked`, `not_started`) | なし | あり (ledger, catalog) | **実在** | `git grep A118 src/` = 0; `data/bga-card-specs/A118.json` 存在・status確認; ledger行あり; catalog行あり (難易度3) |
| **C088 見習い大工** | なし | **なし** (spec JSON 存在せず) | なし | あり (ledger, catalog) | **実在** | `git grep C088 src/` = 0; `data/bga-card-specs/` に C088.json なし; ledger行あり; catalog行あり (難易度3) |
| **C115 種まき人** | なし | **なし** (spec JSON 存在せず) | なし | あり (ledger, catalog) | **実在** | `git grep C115 src/` = 0; `data/bga-card-specs/` に C115.json なし; ledger行あり; catalog行あり (難易度3) |
| **A088 生け垣管理人** | あり (`PassiveRule::FenceWoodDiscount(3)`) | あり (`implemented`) | なし | あり (ledger, catalog) | **一部実在** (実装済み・記録欠落) | spec.implementation_status=implemented, ledger.implementation_status=implemented; 「文書に未記載」は実装欠落ではなく記録側の欠落 |
| **A110 仕立屋** | あり (`AfterClayConstructionGain { food, 3 }`) | あり (`implemented`) | なし | あり (ledger, catalog) | **一部実在** (実装済み・記録欠落) | 同上 |

## Step 2: 記録の補完

新規ドキュメント `docs/outband-unimplemented-records.md` を作成し、6 枚すべての個別記録を記録した。
- A109/A118/C088/C115: 未実装の理由・必要前提・次の一手
- A088/A110: 実装済みであることを正式記録（DEEPAI_PROGRESS.md への記載は open PR 6 本と衝突するため行わない）

## Step 3: A109 実装（最優先）

**仕様**: 改進歩（大進歩/小進歩）が手札からプレイされるたび、3 食料を獲得。

**変更内容**:
1. `EventKind::ImprovementPlayed` 追加 (`src/registry.rs`)
2. `CardEffect::AfterImprovementPlayedGain { good, amount }` 追加 (`src/cards.rs`)
3. `effect_phase` / `effect_condition_holds` / `execute_chosen_triggers` 対応 (`src/card_events.rs`)
4. `dispatch_play` に改進歩プレイ後の専用スキャン追加 (`src/card_events.rs`)
3. イベントインデックス登録追加 (`src/registry.rs`)
4. `verified_band_a_cards()` に A109 定義追加 (`src/cards.rs`)
5. `src/policy.rs` ヒューリスティック対応
6. **テスト追加** (`tests/ch04_after_improvement_played.rs`, 6 件):
   - レジストリ到達性
   - `apply_on_play_effects` 経路（on-play 効果なし）
   - `dispatch_play` 経路: 正系（小進歩プレイで発火）
   - `dispatch_play` 経路: 不発（職業プレイでは発火しない）
   - `dispatch_play` 経路: 複数回発火
   - `dispatch_play` 経路: 境界（A109 より前にプレイされた改進歩では発火しない）

## 既存カード `t_49f032aa` との切り分け

- A118 の実装は `t_49f032aa` (worker-kilo, ready, branch `m10/hermes-ch-harvest-followup`) の担当。本 PR では **A118 を実装しない**。
- 記録のみを行う。

## C088 / C115 の扱い

- いずれも **spec JSON が存在しない** ため、仕様作成 (`data/bga-card-specs/<ID>.json` + tests JSON、`scripts/validate_bga_card_specs.py` 緑) が前提。
- C115 はカード上在庫 (`card_accumulations`, PR #371 **merged 2026-11-19T06:20Z**) が前提（**main マージ済み**）。
- C088 は費用軽減の順序記録 (LOG) が前提。
- 基盤が main にないまま近似実装しない方針のため、**本 PR では実装せず**、記録に「仕様未作成のため実装不可」と理由を記載し、PR 本文の残件欄に送る。

## ゲート確認

- `cargo fmt --check` ✅
- `cargo clippy --all-targets --all-features --locked -- -D warnings` ✅
- 追加テスト `cargo test --test ch04_after_improvement_played` ✅ (8/8 passed)
- 既存テスト `cargo test --lib` ✅ (414/414 passed)
- `cargo test --test m6_verified_pool` ✅ (プール数・MCTS ダイジェスト・`state_hash` 不変)
- `cargo test --lib mcts::tests::sample_game_distribution_digest_is_stable` ✅
- `band_a_cards_are_absent_from_the_two_player_preset` ✅ (A109 は 2 人用プリセットに入らない)

## 残件

| カード | 状態 | ブロッカー |
|---|---|---|
| A118 | 記録のみ | `t_49f032aa` / PR #385 担当、仕様 JSON 修正（`frequency`, 購入上限）が前提 |
| C088 | 未実装 | 仕様 JSON なし、LOG 基盤なし |
| C115 | 未実装 | 仕様 JSON なし（カード上在庫 PR #371 は 2026-11-19T06:20Z に main マージ済み） |
| A088 | 記録補正のみ | 実装済み・テスト済み |
| A110 | 記録補正のみ | 実装済み・テスト済み (仕様データ残件 R-A4 は別 PR) |

## PR 状態確認（Step 0）

- PR #370: open, head=7204d1f, base=research/deepai-allstages-readiness
- PR #375: open, head=176c9400, base=main
- PR #376: open, head=6d97269a, base=research/deepai-allstages-merged (stacked on #370)
- PR #371: **merged 2026-11-19T06:20Z**, merge commit `181253d4e179239c6b3128d60b242dcdd68095fb` (card_accumulations)
- いずれも main 未マージ。本 PR が触るファイル (`src/cards.rs`, `src/card_events.rs`, `src/registry.rs`, `tests/`) に触る同一関心の open PR はなし。

**マージしない。レビュー待ちで完了。**
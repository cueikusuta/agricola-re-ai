# 修正: Ch3 OnPlayGainPerCount 7枚 — 独立レビュー REQUEST CHANGES 対応 (cost/prereq 6件誤り・verified_band_b_cards 重複・B004 推定値・テスト0件)

## 概要
レビューカード `t_1d1a94f7` の REQUEST CHANGES（P0×4 / P1×2 / P2×3 / P3×1）への対応。
`git rebase origin/main`（base `65f2c2b` → `7b49b22`）の上に修正を積む。`src/policy.rs`/`src/registry.rs` の rebase 競合は解消済み、`src/cards.rs` の `verified_band_b_cards` 2重定義は main 側既定義への統合で解消。

## 対象カード（実装・登録 5 枚：C004/C005/D005/B006/B007；B004 は保留）
- C004 筆記板 (Writing Boards)、C005 改修 (Remodeling)、D005 畑の粘土 (Clay from Fields)、B006 採石場見学 (Excursion to the Quarry)、B007 賃金 (Wages)、B004 薪の山 (Firewood Pile・保留)

## 主な変更

### P0-1: cost / prerequisite を spec JSON 正本に合わせ（6枚中6枚が誤りだった）
| カード | 従前 cost | 正 cost | 前提 |
|---|---|---|---|
| C004 | wood 1 | food 1 | なし |
| C005 | clay 2 | food 1 | なし |
| D005 | なし | food 1 | なし |
| B006 | food 1 | food 2 | occupation_count n=1 (`CardRequirement::MinimumOccupations(1)`、base 既存) |
| B007 | wood 1 | なし（コスト0） | なし |

### P0-2: `verified_band_b_cards()` の重複定義を解消
main の既定義（B122/B142/B144、`src/cards.rs:3195`）に B006/B007 を追記。分支側で新規作成した 2 番目の `pub fn verified_band_b_cards()` と重複する `add_cards_inner(...verified_band_b_cards()...)` 呼び出しを削除。定義・呼び出しは各 1 か所。

### P0-3: B004 を実装から外して保留
`workers_on_accumulation_spaces()` = `family.saturating_sub(2)` は推計値で spec（"1 wood per own person standing on accumulation spaces"、B004-normal 2→wood_delta 2）を満たさず、`DispatchContext` に累積スペース上の人数は露出していない。B004 をカードプール（`verified_band_b_cards` / C/D帯プール）から削除し、`CountKind::WorkersOnAccumulationSpaces` を除去。
`data/bga-card-specs/B004.json`: `review.status: "needs_resolution"`（語彙ルール5、無効果のダミー登録なし）、`effects: null`、`unresolved_fields: ["person_on_accumulation_space の累積スペース別人数が PublicState に露出していない"]`。`implementation_status` は `not_started`（スキーマ enum の範囲内）。保留理由は別 Issue で人間決定に委ねる。

### P0-4: テスト 19 件新規（`tests/ch03_onplay_per_count.rs`）
`data/bga-card-tests/*.json` の全ケースを supply 増減で検証（文言スナップショットではない）:
- C004 normal (occupations 3 → wood_delta 3) / zero (0 → 0)
- C005 normal (clay_rooms 2 + improvement 1 → clay_delta 3) / zero (0 → 0)
- D005 normal (sown_fields 2 → clay_delta 2) / zero (0 → 0)
- B006 normal (family 3 → stone_delta 3) / boundary (family 2 → stone_delta 2) / 前提未達（occupation 0）で `play` 不発
- B007 normal (bottom row 2 → food_delta 4) / zero (0 → food_delta 2)
+ `build_default_registry()` 到達可能性（5 ID すべて `registry.definition()` で引ける、B004 は登録されない）、`card_events::dispatch_play` 経路での supply 増減。

### P1-5: 台帳・進捗記録の整合
- spec JSON の `review.implementation_status`: 5 枚を `tested`、`test_case_ids` を実装ケースに合わせ。B004 は `not_started` / `review.status: "needs_resolution"`。
- `DEEPAI_PROGRESS.md` に「第3章（A/B帯）: OnPlayGainPerCount 拡張 — C004/C005/D005/B006/B007 5枚実装・B004 保留」を記録（`### 第2章（A/B帯）` の形式に準拠）。
- `python scripts/validate_bga_card_specs.py`: errors []（RC=0）。`collect_bga_card_data.py` の `implemented` 集合（`src/cards.rs` 内のカード ID リテラル）と spec の implementation_status が一致（5 枚 tested / B004 未登録）。

### P2-6: `src/policy.rs` 字下げ
`OnPlayGainPerCount` arm を周囲と同 16 スペース match-arm 字下げに（`cargo fmt --check` 対応済み）。

### P2-7: カウントロジックの単一化
`cards.rs` の `clay_room_count`/`sown_field_count`/`major_improvements_bottom_row` と `card_events.rs::handle_on_play_gain_per_count` の二重実装を単一化（`pub(crate)` 公開後、`card_events` 側が `cards.rs` の共通ヘルパーを呼ぶ）。
`major_improvements_bottom_row` = {Joinery, Pottery, Basketry} の 3 種（2016 Revised 供給ボードの最下行）。根拠は repo 内 spec データからは導出不能（公式ボード照合が別途必要）。

### P2-8: C005 `improvement` カウント語義
`improvement_count_in_front` = プレイ済み小進歩 + 所有大進歩（A117 先行）と解釈し、1 major + 1 minor → clay_delta 2 のテストで語義を固定。spec notes の "major improvement owned" との齟齬は未解決のまま、PR 記載の曖昧ルールとして残す。

## 検証実測値（HEAD vs origin/main `7b49b22`）
- `git merge-tree` / rebase 完了後：コンフリクト 0、重複定義 0
- `cargo fmt --check`: rc=0
- `cargo clippy --all-targets`: rc=0、警告 0
- `cargo test`: 全群 green (RC=0)。新規テスト 19 件（`tests/ch03_onplay_per_count`）含む。lib 単体のみではない全スイート。
- `python scripts/validate_bga_card_specs.py`: errors []

## 典拠（第三者 BGA ウィキ起こし、official verification とは区別）
- C004: https://agricola.cloudfree.jp/?p=3085
- C005: https://agricola.cloudfree.jp/?p=3084
- D005: https://agricola.cloudfree.jp/?p=2916
- B004: https://agricola.cloudfree.jp/?p=3253
- B006: https://agricola.cloudfree.jp/?p=3251
- B007: https://agricola.cloudfree.jp/?p=3250
- 相互照合: https://agricola.cloudfree.jp/cardlist_en/

## オーケストレータへの連絡
子カード `t_6106f137`（第4章）が C004/C005/D005/B004/B006 を再実装対象にしており、本実装と重複定義・重複登録になる。`t_6106f137` のスコープ調整（A008＋未実装分のみ）をオーケストレータにて確認されたい。

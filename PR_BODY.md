## 実装: B049 天秤 (Scales)

### 概要
B049 天秤 (Minor Improvement, cost 1 wood, points 0) を実装。
- 前提: 職業を出していない (MaximumOccupations(0))
- 効果: 職業または進歩を配置した後、進歩数（基礎進歩＋played minor improvement）と職業数が同数なら food 2
- 既存の専用 after-play scan（職業用と improvement 用）から発火

### P2/P3 指摘の対応（PR #442 レビュー Round 1 / 追記）
- **P2 永続テスト欠落**: `tests/b049_scales.rs` 新規 (10 件): レジストリ到達性・バンド定義、`can_play` 前提ゲート、dispatcher 職業スキャン (同数 +2 / 不同数 +0)、dispatcher 改進歩スキャン (同数 +2 / 不同数 +0)、後続無関係行動での再付与なし、B049 自身プレイ不発火、legacy 経路 negative control
- **P2 fixture 数値不整合**: `data/bga-card-tests/B049.json` の expected を配置後計数に整合 (balance-occ {imp1/occ0}→2, balance-imp {imp1/occ2}→2, unbalanced {imp1/occ1}→0)
- **P3 implementation_status 未更新**: `data/bga-card-specs/B049.json` と `data/bga-card-base-ledger.csv` の B049 を `not_started` → `implemented`

### 変更ファイル
- tests/b049_scales.rs: 新規 (B049 挙動の永続 Rust テスト 10 件)
- src/cards.rs: 新 variant AfterPlacementGainIfImprovementsEqualOccupations、B049 定義を verified_band_b_cards() に追加
- src/card_events.rs: handle_after_placement_gain_if_improvements_equal_occupations ハンドラ、dispatch_play の両スキャンに統合
- src/registry.rs: EventKind::CardPlayed への索引付け、検証用 amount 抽出
- src/policy.rs: ヒューリスティック値の計算
- data/bga-card-tests/B049.json: fixture expected 値の修正
- data/bga-card-specs/B049.json: `implementation_status` → `implemented`
- data/bga-card-base-ledger.csv: B049 行 `implementation_status` → `implemented`
- docs/bga-card-effect-vocabulary.md: improvements_equal_occupations 条件追加

### テスト・ゲート（ローカル実測、HEAD `a6c5c3e` + 本修正）
- cargo fmt --check ✅
- cargo clippy --all-targets --all-features --locked -- -D warnings ✅ (warning 0 件)
- cargo test --lib ✅ (480 passed, うち b049_scales 新規 10 件)
- cargo test --test b049_scales ✅ (10 passed)
- python scripts/validate_bga_card_specs.py (errors: []) ✅
- python scripts/validate_card_catalog.py (errors: []) ✅
- cargo test --test m6_verified_pool (3/3 passed) ✅
- cargo test --lib mcts::tests::sample_game_distribution_digest_is_stable ✅

### Actions チェック結果（exact HEAD `e9a9a8c` の実測値、全必須チェック success）

| チェック名 | 結論 | Run URL |
|---|---|---|
| CI required (aggregate) | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611740/job/137738550076 |
| cargo test | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611740/job/137737286487 |
| rustfmt | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611740/job/137737286361 |
| clippy | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611740/job/137737286418 |
| M1/M2/M7 seeded performance | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611740/job/137737286351 |
| Pipeline gates (spec/catalog/blockers) | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611540/job/137737206846 |
| Detect changed files | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611740/job/137737207123 |
| bga-spec-contract | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611445/job/137737206759 |
| test (Paired AI Evaluation aggregate) | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611856/job/137737206331 |
| Python unittest discovery | **success** | https://github.com/cueikusuta/agricola-re-ai/actions/runs/45187611454/job/137737206317 |
| shard / aggregate (Paired AI Evaluation) | skipped | — |

### 完了条件
- P2/P3 指摘の全修正を本ブランチにコミット・push（force push なし、新規 PR なし、マージなし）
- exact HEAD で必須 Actions が全 success（PR merge check 緑）
- PR 本文に Actions run URL と実測結果を記録（別コミットで補完後、gh pr edit で反映）
- 独立レビュー・マージは子カードに委譲（本 PR はマージしない）

Closes: 実装カード t_d3a9a61f

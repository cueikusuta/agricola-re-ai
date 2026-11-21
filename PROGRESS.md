# PROGRESS — 対戦AI評価基盤（feat/deepai-eval-harness）

最終更新: 2026-11-20。担当: DeepAI。作業worktree: `C:/Users/agent/work/deepai/wt-eval-harness`。

## 0. 現在地

| 項目 | 状態 |
|---|---|
| 設計書 | PR #400（`research/qwen-ai-eval-design` の `docs/ai-evaluation-design.md`）。**未マージ**。本書はその §1.7・§2・§3・§5 を実装対象にする |
| 基盤ブランチ | `feat/deepai-eval-harness`（origin/main `c4a4ba6` から作成） |
| 章1 実行基盤 | **完了**（`feat/deepai-eval-harness-01-run`） |
| 章2 集計 | **完了**（`feat/deepai-eval-harness-02-report`） |
| 章3 比較・判定 | **完了**（`feat/deepai-eval-harness-03-compare`） |
| 章1〜3 独立レビュー対応 | **完了**（P0なし。P1-1・P2-1〜P2-8・主要P3を修正し、修正差分は再レビュー待ち） |
| PR | 章ごとに1本（#418/#419/#420）。**作成まで。マージしない** |

## 1. 設計書の要点（本作業の前提）

- 最終形は4人戦・`draft_10_7_simultaneous`。物差しは辞書式「首位率 → 期待順位」、探索・学習の報酬は `score_margin = 自分の得点 − 相手全員の平均得点`（設計書 §1.3・§3.1）。
- 一次比較は「候補1席 vs 凍結基準3席＋基準鏡像arm」。独立標本は seed ブロックで、差はブロック・ブートストラップ（percentile, 10,000反復, seed 20261106）（§2.2・§3.2）。
- 判定は事前登録した辞書式二層。得点差・コストは補助記録で採用根拠にしない（§3.3）。
- 条件は fail closed。`condition_id`・config ID・デッキ・予算・provenance が違えば合算しない（§3.4・§5.5）。
- 未決定 U1〜U15 は決めない。特に **U1（10-7同時ドラフトの残る慣行）** と **G1 40+40（公式照合済み4人プール）** が未達のうちは、最終モードの正式評価をしない。フィクスチャは機能検証専用（§1.5・§4.1）。

## 2. 現状の実装を読んだ結果（足りない部品）

既存の再利用対象:

- `src/bin/paired_eval.rs` — 座席ローテーション付きA/B評価。4人戦・再演検証・首位クレジット・期待順位・得点差の行出力（schema v1）は既にある。
- `scripts/paired_eval_stats.py` — 首位クレジット差のブロック・ブートストラップ、飽和・head_to_head・config ID検証・時間指標。schema v1 専用。
- `src/draft.rs` / `src/draft_ai.rs` / `src/draft_game.rs` — 10-7同時ドラフト（2〜4人）と、完成ドラフトから `Game` を開始する橋渡し。
- `src/card_pool.rs` — 4人適格・公式verifiedのプールゲート。現状は **verified 0枚**（`card_pool_report` 実測）。
- `src/dataset_split.rs` — seed分割ManifestとG0検査（本作業では未接続）。

足りない部品（実装順）:

1. **ドラフト→対戦の評価実行**: `paired_eval` はドラフトを回さない。ドラフト方策を席ごとに指定し、完成手札から `start_game_from_draft` で対戦へ接続する経路が無い。
2. **4人ドラフト用プール**: 4人10-7は職業40＋小進歩40を配る。実装済みカードは職業18＋小進歩22で足りず、公式verifiedは0。設計書 §4.1 に従い、機能検証専用の `test_fixture` プール（40+40）を用意し、正式評価とは表示を分ける。
3. **実行マニフェスト**: `condition_id`・事前登録・arms・予算・seed範囲・環境を機械可読に残す出力が無い。
4. **スモークと本測定の分離**: 短い確認実行を明示するフラグと、マニフェスト上の `run_mode` が無い。
5. **ドラフト計測**: 1ピック時間・フォールバック・ドラフト再演ハッシュ・`start_player`・最終手札/破棄（コーディネータ側のみ）を行に残せない。
6. **集計**: 期待順位差・得点差差のブロック区間、得点差分布、人間可読要約、判定区分（§3.3）、ガード一覧（§3.4）、マニフェストとの条件一致検証が無い。
7. **比較可能性**: 2つのRunの `condition_id` 一致を検査して比較可否を出す仕組みが無い。

## 3. 章立て（1章 = 1ブランチ = 1PR）

| 章 | ブランチ | 内容 | 主な成果物 | PR base |
|---|---|---|---|---|
| 1 | `feat/deepai-eval-harness-01-run` | 実行基盤。共通の選択・探索コードを `src/eval_harness.rs` へ抽出し、`eval_harness` バイナリで4人・ドラフト・スモーク/本測定・マニフェスト・schema v2 行を出す | `src/eval_harness.rs`, `src/bin/eval_harness.rs`, `src/bin/paired_eval.rs`（抽出のみ）, tests | `feat/deepai-eval-harness` |
| 2 | `feat/deepai-eval-harness-02-report` | 集計。マニフェスト＋行を検証し、首位率・期待順位・得点差の点推定/差/95%CI・分布・人間可読要約を出す | `scripts/eval_report.py`, `tests/test_eval_report.py` | 章1 |
| 3 | `feat/deepai-eval-harness-03-compare` | 比較・判定。辞書式二層の判定区分、ガード、Run間の `condition_id` 一致検査、ベースライン差し替え手順 | `scripts/eval_compare.py`, 追記, tests | 章2 |

各章で `cargo fmt` / `cargo clippy --all-targets --all-features --locked -- -D warnings` / 追加テストを確認し、全件は Actions に任せる（AGENTS.md 18）。PR本文に設計書の対応節・未実装・スモーク出力を記す。

## 4. 決めないこと（設計書 §6 の未決定を固定しない）

- U1（10-7同時ドラフトの残る慣行）: 既定は `draft_10_7_simultaneous`＋右回り固定＋既存 `DraftRuleset`。**新規の慣行を発明しない**。マニフェストに「未確定」と記録する。
- U2（自損妨害の強制機構）: 実装しない。`interference_proxy` は未実装として記録する。
- U4（標準時間予算値）: 既定値は既存 `paired_eval` と同じ 10,000ms を「明示指定可能な値」として維持し、標準予算の決定とは書かない。
- U6（seed範囲）: 新規範囲を割り当てない。スモークは 700000 台（既存のローカル小規模域）を使い、正式レジストリと衝突させない。
- U10/U11（ドラフト代理指標・参照方策）: L1の代理指標や参照方策の強化は本作業の範囲外。L2（対戦）のみ実装する。
- U12（40+40の現況）: 実行時に `card_pool_report` 相当を実測し、`pool_gate_snapshot` に記録する。公式verified 0枚なら `official_final_mode=false`。

## 5. 再開方法

1. `PROGRESS.md` の「現在地」と直近の `result-deepai-eval-harness.md` を読む。
2. 未完了章のブランチで `git log --oneline -5` と `git status` を確認する。
3. 章の残作業は同ファイル末尾の「章メモ」に追記されている。無ければ PR 本文の「未実装で残した部分」を見る。

---

## 章メモ

### 章1 実行基盤（`feat/deepai-eval-harness-01-run`）

- 追加: `src/eval_harness.rs`（共通の `Ai` / `SeatSelector` / `SearchSettings` / `first_credit` を `bin/paired_eval.rs` から移設。挙動不変）、
  `src/bin/eval_harness.rs`（新CLI）、`tests/eval_harness_smoke.rs`。
- 新CLIの出力: `<out>/run-manifest.json` と `<out>/shard-0/matches.jsonl`（schema_version 2）。
  失敗時は `<out>/errors.jsonl` に1行残して非0終了。既存の出力ディレクトリへの追記は拒否（fail closed）。
- `--draft-mode draft_10_7_simultaneous` は `--cards fixture_4p`（4人・FIX 40+40）とだけ組み合わせられる。
  公式verified 4人プール（G1 40+40）が0枚のため、フィクスチャは機能検証専用で、マニフェストに
  `official_final_mode=false` と理由を記録する。
- `--smoke` は4ブロック上限。マニフェストの `run_mode` は `smoke`/`full`。
- スモーク実測（ローカル・release）:
  - カードなし4人: 8試合 0.22〜0.44s、condition `616db47c…`。
  - フィクスチャ10-7ドラフト4人: 8試合 0.93〜1.04s、condition `6dafd41f…`。
- 章1で**やらない**こと: 集計・判定・ガード・Run間比較（章2/章3）。`interference_proxy` はU2未決定のため未実装、
  マニフェストの `unimplemented` に明記。

### 章2 集計（`feat/deepai-eval-harness-02-report`）

- 追加: `scripts/eval_report.py`、`tests/test_eval_report.py`。
- `run-manifest.json` と schema2 行を突合し、欠落・重複・条件不一致・再演未検証・credit/marginの改竄を fail closed で拒否。
- 指標: 首位率（1/k）・単独1位率・首位到達率・期待順位・得点差（平均/分位/ヒストグラム）・席別首位率。
- 差: seedブロック単位の対応差（首位率・期待順位・得点差）と percentile bootstrap 95%CI（10,000反復・seed 20261106）。
- 出力: `report.json`（機械可読）と `report.md`（人間可読）。スモーク/フィクスチャ/公式プール未達を明示。
- 判定: §3.3 の第一層区分（adopted/improvement/hold/rejected）を**シグナル**として実装。事前登録が無い Run は
  `final=undetermined`。§3.4 のガードを一覧化し、未解決ガードを列挙。
- 実測: カードなし4人スモーク（8試合）と fixture 10-7 スモーク（8試合）で `report.json`/`report.md` を生成。
- テスト: `python -m unittest tests.test_eval_report` 23件成功（レビュー対応で7件追加）。ローカル全Python 309件成功。

### 章3 比較・判定（`feat/deepai-eval-harness-03-compare`）

- 追加: `scripts/eval_compare.py`、`tests/test_eval_compare.py`。harness マニフェストに `preregistration.registered=false` を明示。
- 同一 `condition_id`・同一 seed ブロック・同一予算・head-to-head の2 Runだけを対応差で比較。条件が違えば拒否。
- A−B の首位率・期待順位・得点差のブロック対応差と 95%CI を出し、第一層シグナルを記録。最終は
  `undetermined`（探索的比較であり採用判定ではない）。
- 実測: カードなし4人・2ブロック・同一seedで candidate `eval_veg` と `eval` を比較。A−B 首位率 0.000 pp
  （CI [0.000, 0.000]）、比較可能性 true。
- テスト: `python -m unittest tests.test_eval_compare tests.test_eval_report` 22件成功。

### 章3の後に残るもの

- 正式な採用判定には、事前登録されたマニフェスト（`registered=true`、追加上限、locked test seed）と
  公式最終モード（G1 40+40・U1確定）が必要。本ハーネスはそれを偽装しない。
- 複数候補の多重比較補正、seedレジストリ照合（U6）、`dataset_split`/G0束縛、L1代理指標（U10）は未実装。

- テスト: `python -m unittest tests.test_eval_report` 23件成功（レビュー対応で7件追加）。ローカル全Python 309件成功。

### 独立レビュー対応（2026-11-22）

- P1-1: マニフェストに実測の `comparison_design`（`head_to_head` / `mirror_baseline` / `opponent_is_baseline`）を追加。
  `primary_comparison` と `arms.opponent.note` を `--opponent` の実値で分岐。共通相手Runを直接対戦として表示しない。
- P2-1: ゲーム方策が同一でもドラフト方策だけ異なる寄与armを許可（同一ゲーム・同一ドラフトのみ拒否）。
- P2-2: `condition_parts` に実測 `code_sha` を追加し、`compute_budget` に MCTS sims/depth・PUCT config id・
  max simulations・draft pick 時間を含めた。非スモークは code identity 無しを拒否。
- P2-3: 行の `puct_*` スカラーを v1 と同じ「常に候補側」に統一し、`focus_puct_config_id` で焦点席を表す。
  `puct_tree_depth` 等の欠落フィールドと `baseline_puct_*` 8個、`search_deadline_active` 等を追加。
- P2-4: 行に `draft_actions` / `draft_event_count` / `draft_state_hash` を追加し、第三者がドラフトを再演できるようにした。
- P2-5: 試合前に `run-manifest.pre.json` を書き、終了マニフェストに SHA-256 を記録。非スモークは `run_mode=unregistered`。
- P2-6: ピック期限は未強制であることを `draft_pick_budget_enforced=false` と `pick_illegal_fallback_count` で明示。
- P2-7: `draft_policy_id` を `name@draft-ai-v1` に版付けし、`details_policy_id` を実型名に。
- P2-8: 相手席の停止規則（固定シミュレーション・time-budget対象外）を `arms`/`compute_budget` に明記。
- 主要P3: seedオーバーフロー事前検査、予算0拒否、schema 2、`rustc_version` の取得元明示、`pool_gate_snapshot` の
  キー名・定数再利用・測定SHA、`official_pool_ready` の実測化、`rounds_completed`/`start_player`/`initial_state_hash`、
  `draft_wall_ms`、`interference_proxy: null`、固定seed再現テスト・キー一意性・seed算術・fixture単独拒否テストを追加。

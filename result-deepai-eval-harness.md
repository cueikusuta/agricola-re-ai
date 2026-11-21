# 結果 — 対戦AI評価基盤（feat/deepai-eval-harness）

設計書: PR #400 `docs/ai-evaluation-design.md`（未マージ）。
対象: 4人戦・10-7ドラフトを前提にした評価実行基盤、指標集計、ベースライン比較。

---

## 章0: 着手前の棚卸し（2026-11-20）

- 設計書と既存 `paired_eval` / `paired_eval_stats.py` / `draft*` / `card_pool` を読了。
- 足りない部品7点と実装順を `PROGRESS.md` §2 に記録。
- 章立て（実行基盤 / 集計 / 比較・判定）とブランチ名を確定。
- この時点ではコード変更なし。

## 章1: 実行基盤（`feat/deepai-eval-harness-01-run`）

### できること

- 4人（2・3人も可）の座席ローテーション付きA/B自己対戦。候補armは焦点席だけ候補AI、他3席は固定相手。
  基準armは全席が基準AI（鏡像対照）。1ブロック = `players` 席 × 2arm。
- ゲームAIの差し替え: `--candidate` / `--baseline` / `--opponent` と `--puct-*`（候補側）。
  基準・相手のPUCTは凍結既定（pt6）に固定し、`PuctConfig::default()` の config ID で記録する。
- 10-7ドラフト（フィクスチャ40+40）: `--draft-mode draft_10_7_simultaneous --cards fixture_4p`。
  席ごとのドラフトAIは `--candidate-draft-ai` / `--baseline-draft-ai`。
- シード: 対戦 `--seed-start`、ドラフト `--draft-seed-start`（既定は対戦と同じ）。ブロックは加算。
- 出力: `run-manifest.json`（condition_id・arms・予算・seed・poolゲート・未実装一覧）、
  `shard-0/matches.jsonl`（schema 2、再演ハッシュ・首位クレジット・期待順位・得点差・ドラフト記録）。
- スモーク/本測定: `--smoke`（4ブロック上限、`run_mode=smoke`）。両モードとも
  `strength_claim_allowed=false`（強さの主張は章3の判定手続きを通す）。

### スモーク実行結果

| 条件 | 試合 | 時間 | condition_id | run_mode |
|---|---|---|---|---|
| カードなし4人・eval_veg vs greedy | 8 | 0.22–0.44s | `616db47c…` | smoke |
| fixture 10-7ドラフト4人・eval vs greedy（draft synergy vs card_value） | 8 | 0.93–1.04s | `6dafd41f…` | smoke |

- 全行 `finished=true`・`rounds_completed=14`・`replay_verified=true`。ドラフト行は
  `draft_replay_hash`（64桁）・`start_player`・`final_hands_by_seat`・`pick_*` を持つ。
- `cargo test --release --test eval_harness_smoke` は2件成功（再実行時の出力ディレクトリ拒否も確認）。
- `cargo clippy --all-targets --all-features --locked -- -D warnings` 成功。

### 未実装で残した部分

- `interference_proxy`（U2）、L1ドラフト単体の代理指標（U10）、`dataset_split`/G0束縛（§2.3）、
  seedレジストリ照合（U6）。マニフェストの `unimplemented` に列挙。
- 公式4人プールは `card_pool_report` 実測で verified 0 枚のため、正式な最終モードは実行不可。
  フィクスチャを正式評価の代わりにしない。

## 章2: 集計（`feat/deepai-eval-harness-02-report`）

### できること

- `scripts/eval_report.py --root <run>` が `run-manifest.json` と `shard-*/matches.jsonl`（schema2）を検証し、
  `report.json`（機械可読）と `report.md`（人間可読）を出す。
- 指標: 首位率（1/k）・単独1位率・首位到達率・同率試合率・期待順位・得点差（平均/分位/ヒストグラム）・席別首位率。
- 差: seedブロック対応差（首位率・期待順位・得点差）と percentile bootstrap 95%CI
  （10,000反復・seed 20261106）。必要ブロック数（5pp/80%）も算出。
- 判定材料: §3.3 の第一層区分を「シグナル」として記録。§3.4 のガード（完全性・条件一様性・直接対戦・飽和・
  鏡像・10秒・公式最終モード・スモーク）を一覧化。
- fail closed: 欠落/重複/条件不一致/seed不一致/再演未検証/credit・margin不一致は集計拒否。

### スモーク実行結果

| 条件 | 試合 | 首位率 cand/base | 期待順位 cand/base | 差（credit） |
|---|---|---|---|---|
| カードなし4人・eval_veg vs greedy | 8 | 75.00 / 25.00 pp | 1.250 / 2.500 | +50.00 pp（1ブロックのためCIは点と同じ） |
| fixture 10-7・eval vs greedy | 8 | 100.00 / 25.00 pp | 1.000 / 2.500 | +75.00 pp |

- 両方とも `run_mode=smoke`、`strength_claim_allowed=false`、`final=undetermined`。
- fixture は「TEST FIXTURE CARDS: functional verification only」を明示。
- `python -m unittest tests.test_eval_report` 16件成功、ローカル全Python 309件成功。

### 未実装で残した部分

- 第二層（期待順位）の統計規約は U5 未決定のため記述的参照のみ。
- 事前登録・locked test seed・追加上限は未実装（`registered=false` を明示）。
- 鏡像対称性の合否しきい値は設計書に無いため `descriptive`（判断しない）。

## 章3: 比較・判定（`feat/deepai-eval-harness-03-compare`）

### できること

- `scripts/eval_compare.py --a <runA> --b <runB>` が、同一条件の2 Runを seedブロックで対応させて比較する。
- 比較可能性を fail closed で検査: `condition_id`・人数・カード・ドラフト・探索モード・予算・基準/相手・seed範囲・
  head-to-head。1つでも違えば比較せずエラー。
- 出力: 各Runの candidate−baseline、A−B の首位率/期待順位/得点差（95%CI）、第一層シグナル、`compare.json` と
  `compare.md`。最終は `undetermined`（採用判定ではない）。
- harness マニフェストに `preregistration.registered=false` を追加し、未登録Runが採用判定にならないことを明示。

### スモーク実行結果

- カードなし4人・2ブロック・同一seed（700300）。A=`eval_veg`、B=`eval`。
- 比較可能性 true。A−B 首位率 0.000 pp（CI [0.000, 0.000]）、期待順位 −0.125、得点差 +0.375。
- `python -m unittest tests.test_eval_compare tests.test_eval_report` 22件成功。

### 未実装で残した部分

- 事前登録された採用判定、多重比較補正、seedレジストリ照合、`dataset_split`/G0束縛、公式最終モード（G1/U1）。

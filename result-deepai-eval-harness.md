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

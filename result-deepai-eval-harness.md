# 結果 — 対戦AI評価基盤（feat/deepai-eval-harness）

設計書: PR #400 `docs/ai-evaluation-design.md`（未マージ）。
対象: 4人戦・10-7ドラフトを前提にした評価実行基盤、指標集計、ベースライン比較。

---

## 章0: 着手前の棚卸し（2026-11-20）

- 設計書と既存 `paired_eval` / `paired_eval_stats.py` / `draft*` / `card_pool` を読了。
- 足りない部品7点と実装順を `PROGRESS.md` §2 に記録。
- 章立て（実行基盤 / 集計 / 比較・判定）とブランチ名を確定。
- この時点ではコード変更なし。

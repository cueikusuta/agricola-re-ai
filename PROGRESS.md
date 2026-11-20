# PROGRESS

作業現在地の記録。各タスクの進捗・コミット・次の作業を残す。

## research/qwen-ai-eval-design — 対戦AI評価基盤 設計書（2026-11-20 開始）

- 目的: `docs/ai-evaluation-design.md` を新規作成（成果物は本文書1つ）。既存コード・データは変更しない。
- 前提: 物差しは辞書式「首位率→期待順位」／探索・学習報酬は得点差／自損妨害なし／最終形は4人戦・10-7ドラフト／PUCT prior_temperature=6 標準。
- ブランチ: `research/qwen-ai-eval-design`（base: `origin/main` 9bbce14）。章ごとに commit + push。PRは作成まで（マージ禁止）。
- 正本参照: `AGENTS.md` / `docs/design.md` / `docs/roadmap-m7-onward.md`。

| 章 | 内容 | 状態 | commit |
|---|---|---|---|
| 1 | 目的・範囲・前提 | 完了 | （このコミット） |
| 2 | 自己対戦の回し方 | 未着手 | — |
| 3 | 評価指標の定義と集計・優劣判定 | 未着手 | — |
| 4 | 10-7ドラフトの評価 | 未着手 | — |
| 5 | 計測の記録形式 | 未着手 | — |
| 6 | 未決定の一覧 | 未着手 | — |

- PR: 未作成。
- 次の作業: 第2章（自己対戦の回し方）を書く。
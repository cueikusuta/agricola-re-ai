# PROGRESS.md — 現在地（エージェント別）

このファイルは担当エージェントごとに節を分けて追記する。相手の節・ブランチには触れない。
過去の Freebuff 節は `research/freebuff-retro-review` / `research/freebuff-retro-review-2`
ブランチにあり、main には未マージ。本ファイルは DeepAI の長時間タスク（3部構成）の現在地。

## DeepAI — 遡及レビュー第3弾・基盤対象カード調査・ボーナス点残り4枚

### 第1部: 遡及レビュー第3弾（ブランチ `research/deepai-retro-review-3`）— **完了**

- 基準 `origin/main` = `1a176a0`（PR #335 マージ後）。
- 対象10件の開始時状態:
  #337 / #341 / #334 / #329 / #335 = merged、
  #340 / #333 / #352 / #353 / #332 = open。
- 成果物: `DEEPAI_RETRO_REVIEW_3.md`、`result-deepai.md` の第1部。
- 判定の要約:
  - main で閉じた: #337 P0×2、#341 3件、#334 check 6、#329 F-1、#335 P0×4、#333（#341 経由）。
  - 未クローズ: #340 P1-1a/b/c（conflict）、#352 save/load E2E（CI赤）、
    #353 EndGameBonus（CI赤）、#332 E100（日英矛盾）。
  - 新規 P1: #340 の C116 自己発動回帰、`dispatch_play_after` の `EventKind` 欠落、
    #352 の E2E 未証明、#353 の `use_limit` 設計、#332 の E100 OR/AND 矛盾。
- 変異テスト8件（M3 が未検出 = #334 の 6b-ii）、一時テストは削除、`git status` は文書のみ。
- PR は作成まで。マージしない。

### 第2部: 段階1・段階2の対象カード事前分類（ブランチ `research/deepai-layer1-2-survey`）— 未着手

### 第3部: ボーナス点 残り4枚の解放条件（ブランチ `research/deepai-bonus-remaining-unblock`）— 未着手

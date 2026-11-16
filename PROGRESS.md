# PROGRESS.md — 現在地（エージェント別）

このファイルは担当エージェントごとに節を分けて追記する。相手の節・ブランチには触れない。

## Freebuff — 課題B（DeepAI 実装の遡及レビュー）

### 第1弾（PR #326, ブランチ `research/freebuff-retro-review`）

- 対象: #311〜#315（第1〜5章の12枚）。
- 成果物: `FREEBBUFF_REVIEW.md`、`result-freebuff.md`。
- 主な指摘: F-1（P0, dispatcher が発火サイトを判別しない）、F-5（P1, dispatcher テスト欠落）。

### 第2弾（ブランチ `research/freebuff-retro-review-2`）— **完了**

- 対象: #317（第6章 A099/A101/A106/A117）、#321（第8章 C074/C077/C078/C079/C118）、
  #327（ボーナス点の持ち場・基盤）、#328（E098/E101）。
- レビュー基準: `origin/main` = `5fba360`。
- 成果物: `FREEBBUFF_REVIEW_2.md`、`result-freebuff.md` の第2弾節。**コード変更なし**。
- 結果: P0 1件（G-1, F-1 の継続・A106 の新露出）、P1 3件（G-2 決定化が予約台帳を落とす /
  G-3 #328 が #327 の検証修正を含まない / G-4 A帯の dispatcher・legacy テスト0件）、
  P2 4件、P3 6件。
- 再測: #321 = 661 passed、#327 = 661 passed、#328 = 666 passed。#317 は main 上で緑。
- 変異テスト18件: 17件検出、1件未検出（A099 の legacy 複製）。
- 次の作業（別 Issue / 別 PR）:
  1. G-1 は #329 の修正マージ時に「ワーカー配置で A106 が発火しない」回帰テストを確認。
  2. G-2 `mcts::sample_game` に `delayed_goods` の引き継ぎを追加。
  3. G-3 #328 を #327 の最新 HEAD に rebase し、増分検証の台帳検査を有効化。
  4. G-4 A 帯4枚の dispatcher / legacy テスト追加。
  5. G-14 C074 の文献照合（ページ再取得）。

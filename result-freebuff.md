# result-freebuff.md — 課題B（DeepAI 実装の遡及レビュー）

> 第1弾（#311〜#315）の要約は PR #326 / ブランチ `research/freebuff-retro-review` の
> `result-freebuff.md` と `FREEBBUFF_REVIEW.md` にある（未マージのため `main` には無い）。
> このファイルは本ブランチ（第2弾）の記録である。区切りごとに追記する。

## 第2弾（2026-11-16）— PR #317 / #321 / #327 / #328

担当: Freebuff（課題B）。ブランチ `research/freebuff-retro-review-2`
（`origin/main` = `5fba360` から分岐）。成果物: `FREEBBUFF_REVIEW_2.md`。**コード変更なし**。

### 開始時の確認

- #317 は merged（`abd538e`）、#321 は open（`d8e1722`）、#327 は open（`308ce83`）、
  #328 は open（`f89c1f5`）。
- **#328 の基点は `f2a4bcc`**（#327 の1コミット目）で、レビュー指摘修正 `308ce83` を含まない。
  これがそのまま P1（G-3）になった。
- `main` には第1弾の P0（F-1）修正が未マージ（#329 は open）。#317 の A106 はその新しい露出先。

### #317（第6章 A099 / A101 / A106 / A117）

- 効果文は4枚とも wiki の要約と一致（A101 の窯除外も `is_cooking` と整合）。
- **A106 が `dispatch_after_worker` でも発火**（実測 食料+3）。F-1 の継続（G-1, P0）。
- dispatcher / legacy 経路テストが**0件**。A099 の legacy 複製は変異が全 lib テストで未検出
  （G-4, P1）。
- A106 の「最後の作物」判定が3箇所重複（G-5）。A117 の canonical はゾーン移動前解決のため、
  同じ語彙を小進歩に使うと1つ少なく数える（G-6）。
- 変異テスト: M1/M3/M4/M5/M6 検出、M2（legacy 複製）未検出。

### #321（第8章 C074 / C077 / C078 / C079 / C118）

- 効果文は C077 / C078 / C079 / C118 が一致。**C074 はページ取得失敗で未照合**（G-14）。
- **`sample_game` が `delayed_goods_schedule` を引き継がない**（実測 3件→0件、G-2, P1）。
  PR #265 の well_food_schedule と同じ欠陥の再発。観測ミラーは作られているのに読む経路が無い。
- canonical `cards::play` は予約を実装せず、評価 projection も無効果（G-7, P2）。
- 変異テスト: P1〜P5 すべて検出（ラウンド式・回収・validate・integrity_hash）。661 passed。

### #327（ボーナス点の持ち場・基盤）

- 変異テスト Q1〜Q4 すべて検出（integrity_hash・validate・得点・増分検証）。661 passed。
- 得点・順位の経路は `Game::standings()` に統一され、取りこぼしは無い（grep で確認）。
- validate は範囲 1〜60 と出所カードしか見ないため、プレイ済みカードへの加点改竄は full 検証を
  通る（G-8, P2）。台帳変更で増分検証が常に full に落ちる件は性能面の既知事項（G-11）。

### #328（E098 / E101）

- 効果文は2枚とも一致。ステージ割り当て（4/3/2/2/2/1）も妥当。
- **3経路一致テストがある**（第1〜6章の欠落に対する改善）。実エンジンで E101 はラウンド1に5点、
  `card_bonus` に入り `cards`（印刷点）は0のまま。666 passed。
- **基点が古く、台帳改竄が `validate_incremental` を Pass**（実測、G-3, P1）。rebase 必須。
- `PendingDecision::state_hash_before` は `state_hash()`（台帳非包含）なので、中断→再開の検証では台帳の改竄を検出しない（G-9, P3）。
- 変異テスト R1/R2/R3 すべて検出。

### 横断

- `tests/m13_decision_budget.rs` の1件が3ブランチ同時 `cargo test` で落ち、単独では緑
  （時間依存の偽陽性、G-13）。
- 一時プローブ・変異はすべて削除・復元済み。`git status` は本PRの文書のみ。

### 次の再開位置

- 報告は `FREEBBUFF_REVIEW_2.md` に記載済み。追加対象の指示は無い。
- G-1 は #329 の修正で塞がれる見込み。マージ時に「ワーカー配置で A106 が発火しない」回帰
  テストがあるかを確認する。
- G-2 / G-3 はコード修正が必要なため、別 Issue・別 PR（独立レビュー付き）とする。
- G-14（C074 の文献照合）は再取得でき次第 `FREEBBUFF_REVIEW_2.md` §1 を更新する。

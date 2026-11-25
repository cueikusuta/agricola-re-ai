# PROGRESS

## Freebuff — 実装済み75枚の効果解釈 文献照合（ブランチ `research/freebuff-impl-source-check`）

- 基点: `origin/main` = `7ff3f67`。成果物: `docs/impl-source-check.md`。コード・`data/` は変更しない。
- 対象: #377 の実装済65枚 + A008 + #378 の4枚 + #379 の1枚 + #381 の4枚 = **75枚**（A29/B21/C8/D8/E9）。
  `src/cards.rs` のIDリテラル75件と一致。
- 現在地: 全章完了。PR #387 を作成。**マージしない。** 独立レビュー待ち。
- 第A章: 一致25、食い違い2（A088, A101）、仕様欠落2（A005, A119）。
- 第B章: 一致14、食い違い2（B013, B126）、仕様の表現不足5。
- 第C章: 一致3、食い違い1（C005）、仕様欠落3、文献未確定1（C116）。
- 第D章: 一致1、仕様欠落5（D004/D005の前提、D100/D109のJSON、D105の窪地）、表現不足2（D034, D061）。
- 第E章: 一致4、食い違い2（E113, E118）、仕様欠落3。
- 第6〜8章: 食い違い7、仕様欠落・表現不足20、文献未確定1、質問8件。
- 検証: 帯表75行・一意75、判定合計75。src/data/tests は無変更。
- 再開はこの節の「現在地」から。章ごとに commit と push を行う。
- 共通条件: PRは作成まででマージしない。効果の扱いが決まらないカードは決めず、文献要約を添えて
  人間への質問チェックリストに残す。

## 終了時得点カード（EndGameBonus）実装 — ブランチ `feat/deepai-endgame-impl-2`

- 基点: origin/main `1576429`。PR #503（docs のみ）とは独立。
- 除外: main 実装済み（A032, A038, A098, A099, A101, A133, B098, B099, B153, D030, D032, D034, D100, E032, E034, E035）、他オープンPR担当（A031, A134, A135, A136, C030, C031, C033, D035, D038, E032 関連）。
- 実装済み（2枚）
  - E124 市長候補者: 職業・前提なし。登場時に木材2・石材2。採点時、供給の木材と石材1つごとに−1点。
  - B031 製陶所の敷地: 小進歩・費用なし・印刷1点。前提は大進歩「製陶所」所有（大製陶所 D060 は main 未実装のため対象外）。採点時、直交隣接する未使用農場スペース2つ以上で+2点。
- 共通基盤（追加のみ、enum 末尾追加でシリアライズ互換を維持）
  - `CardRequirement::OwnsMajorImprovement(Improvement)`（`cards.rs` と `card_events.rs` の前提判定）
  - `ScoreBonusRule::WoodStoneSupplyPenalty`, `ScoreBonusRule::UnusedFarmAdjacentPair`
- テスト: `tests/endgame_e124_b031.rs`（7件）。`cargo fmt --check`、`cargo clippy --all-targets --all-features --locked -- -D warnings` 通過、`cargo test --test m6_verified_pool`、`mcts::tests::sample_game_distribution_digest_is_stable`、`--lib end_game` / `--lib prereq` 通過。
- 未実装（理由は PR 本文と `result-deepai.md` の該当節を参照）: B035, B133, B100, C035, C059, E038, C135, E134, E037, B038, B039, C133, C134, C136, E132, E135, E136, E154, E159, B136。
- 再開はこの節の「未実装」から。人間判断が必要な項目はPR本文の質問チェックリストに残す。

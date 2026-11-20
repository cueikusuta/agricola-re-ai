# 実装済みカードの効果解釈 文献照合（第2次） 結果ログ

担当: Freebuff。ブランチ `research/freebuff-impl-source-check-2`（基点 `origin/main` = `d267be8`）。
成果物: `docs/impl-source-check-2.md`。**コード・`data/`・`tests/` は変更しない。PRは作成まで、マージしない。**

## 第0章 方法と対象（完了）

- 対象の同定: `src/cards.rs` のIDリテラル77件から #387 の75枚を引いた差集合 = **C083, C147**。
- 差分の由来は PR #384（`b876a46`）のみ。#378 / #379 / #381 は #387 の75枚に含まれている。
- `src/cards.rs` 以外のIDリテラル（A001 等11件）はテスト用フィクスチャで、実装カードではない。
- 帯別: A29 / B21 / C10 / D8 / E9 = 77。今回の対象は C帯の2枚のみ。
- 出典の等級: 公式ルールA、リポジトリB、BGA wiki・BGA和文ルールC、非公式コンピューディウム・付録裁定集D。

## 第A章 A帯（対象0枚・完了）

- 追加実装なし。前回 #387 の29枚から変化なし。

## 第B章 B帯（対象0枚・完了）

- 追加実装なし。前回 #387 の21枚から変化なし。

## 第C章 C帯 2枚（完了）

- 一致2（ただし C083 は仕様テストの期待が食い違い、C147 は実装の適格反映に注記あり）。
- C083 若牛: 費用なし・前提なし・-3点・出した時牛2。文献と一致。空き容量0での扱いは仕様テストが
  「プレイ拒否」、文献（公式ルールの収容・放棄、BGAの家畜取得手順、同型カードの裁定）は「プレイ可・超過は放棄」。
- C147 牛の飼育士: 牛市場の使用のたび追加で牛1（共通ストックから）。回数・対象・出所まで一致。
  5人戦の「好きな動物1」スペースでは発動しないという裁定と同じ構造で、実装も `Space::CattleMarket` 限定。

## 第D章 D帯（対象0枚・完了）

- 追加実装なし。前回 #387 の8枚から変化なし。

## 第E章 E帯（対象0枚・完了）

- 追加実装なし。前回 #387 の9枚から変化なし。

## 第2章 食い違い・質問（完了）

- 食い違い3件: C083 の仕様テスト期待（重大度 中）、C083 の自動配置・調理なし（重大度 中、実装側）、
  C147 の人数適格がレジストリに反映されていない（重大度 低、実装側）。
- 仕様の表現不足2件（いずれも一致の範囲内の注記）: C147 の `notes`、C083 の `implementation_status`。
- 文献で未確定1件: C083 にカード名指しの裁定が無い。
- 人間への質問4件: Q-1 C083 の容量不足時の可否、Q-2 C083 の配置・調理の決定主体、
  Q-3 C147 の2人戦適格の強制、Q-4 台帳の実装列の更新担当。

## 検証

- 対象集合の機械照合: 77 − 75 = 2（C083, C147）、重複0・欠落0。
- 帯表の行数 = 対象2枚、判定の内訳合計 = 2。
- 変更は `docs/impl-source-check-2.md`・`PROGRESS.md`・本ログのみ（`src/` `data/` `tests/` は無変更）。

## PR とゲート

- PR: #394（base `main`、head `research/freebuff-impl-source-check-2`）。**マージしない。**
- HEAD `fad9ad7` の Actions（docs のみの変更のためコード系は skipped）:
  - Detect changed files: pass / CI required: pass / Pipeline gates (spec/catalog/blockers): pass
  - cargo test / clippy / rustfmt / M1・M2・M7 seeded performance: skipped
  - https://github.com/cueikusuta/agricola-re-ai/actions/runs/44672162343
  - https://github.com/cueikusuta/agricola-re-ai/actions/runs/44672162348
- 独立レビューは未実施。カード効果の解釈に関わるため AGENTS.md §14 の対象で、別セッションでの初回レビューが必要。

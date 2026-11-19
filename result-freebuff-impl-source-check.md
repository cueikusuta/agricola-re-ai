# 実装済みカードの効果解釈 文献照合 結果ログ

担当: Freebuff。ブランチ `research/freebuff-impl-source-check`（基点 `origin/main` = `7ff3f67`）。
成果物: `docs/impl-source-check.md`。**コード・`data/` は変更しない。PRは作成まで、マージしない。**

## 第0章 方法と対象（完了）

- 対象75枚 = #377 の実装済65枚 + A008 + #378 の4枚 + #379 の1枚 + #381 の4枚。
  帯別 A29 / B21 / C8 / D8 / E9。
- `src/cards.rs` のIDリテラル75件と完全一致（重複0・欠落0）。
- 照合順: 仕様JSON → 日本語個別ページ（`?p=`）→ 英語一覧 → 実装コード（読むだけ）。
- 出典: 公式ルールA、リポジトリB、BGA wiki C、ページコメントD。カード固有の公式FAQは未確認。
- 判定: 一致 / 仕様の表現不足・欠落 / 食い違い / 文献で未確定。
- 注意: 台帳の `implementation_status` は大半が `not_started` のままで実装と不一致（後の章で記録）。

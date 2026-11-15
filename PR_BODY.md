# Issue #225: A/C/D/E 001-069 帯 needs_resolution 解消 第1弾

## 概要
Issue #225 の A/C/D/E 001-069 帯から、`review.status=needs_resolution` または `inventory_only` のカードについて、2026-11-15 照合方針 (#308/#316) に基づき仕様確定を行う第1弾。

## 完了カード (16枚)

### A帯 (4枚) — 再生成済み（ビルダー正）
- **A004 幅木**: コスト不明（代替コスト 2食料/1小麦のみ記載）→ `needs_resolution` 維持
- **A009 子供の動物市場**: コスト不明（代替コスト 1羊のみ記載）→ `needs_resolution` 維持
- **A020 畝立て鋤**: ラウンド依存コスト（ラウンド3まで小麦1、ラウンド4-5で小麦1+食料1）語彙未対応 → `needs_resolution` 維持
- **A064 大麦製粉機**: コスト不明（選択コスト 1木+4レンガ/2石 のみ記載）→ `needs_resolution` 維持

### C帯 (12枚)
- **C003 馬車旅行**: 前提条件「未配置の人物1」語彙外 → `needs_resolution` 維持
- **C009 自動水受け皿**: 購入時間窓「すぐに」原典に断定なし → `needs_resolution` 維持
- **C018 回転鋤**: トリガー「いつでも（anytime）」語彙未定義 → `needs_resolution` 維持（fail-closed）
- **C020 もぐら鋤**: 前提条件「ラウンド9以降」語彙外 → `needs_resolution` 維持
- **C023 雇用契約**: 前提条件「職業未プレイ」確認済み → `bga_checked`
- **C029 ビアテーブル**: 前提条件「ストック小麦なし」確認済み → `bga_checked`
- **C051 漁網**: マルチプレイヤー税効果・帰宅フェイズ補充確認済み → `bga_checked`
- **C052 猟師の帽子**: 前提条件「鍋の進歩」、効果: 猪ごと確認済み → `bga_checked`
- **C053 三つ足やかん**: 効果「調理改良で2品物同時変換時」確認済み → `bga_checked`
- **C054 露店**: コスト厩返却・トリガー畑フェイズ後・柵支払い いずれも語彙外 → `needs_resolution` 維持
- **C056 給餌柵**: 最後の厩判定・代替粘土コスト いずれも語彙外 → `needs_resolution` 維持
- **C057 生野菜**: トリガー「いつでも（anytime）」語彙未定義 → `needs_resolution` 維持（fail-closed）

## 変更内容
- 16枚のカードスペックを `scripts/build_bga_card_specs.py`（正本）で再生成し、ドリフトを解消
- A帯4枚（A004/A009/A020/A064）は JSON 直編集をやめ、ビルダーから再生成
- C018/C057 を `bga_checked` から `needs_resolution` に戻し、テスト契約と整合（fail-closed 準拠）
- A020 のコストを単一値から未解決項目へ修正（ラウンド依存コストは語彙で表現不可）
- C003/C009/C020/C054/C056 の未強制前提・語彙外表現を `unresolved_fields` に保持し `needs_resolution` のままとした
- C009 の「any time」断定を削除
- `check_status.py` を削除
- `docs/bga-card-effect-vocabulary.md` に新語彙 7 件を追記（`after_stable_built`, `field_vegetable_stacked`, `cultivated_fields_at_least`, `discard_field_goods_plow_one`, `buy_animal`, `place_unplaced_person`, `build_stable_alternate_cost`）
- 検証スクリプト `python scripts/validate_bga_card_specs.py` 全緑確認済み（errors: []）

## 残作業 (第2弾以降)
- **C帯 needs_resolution**: C058, C060, C062, C065, C069 等 (残り 261枚)
- **D帯 needs_resolution**: D001-D069 全37枚
- **E帯 needs_resolution**: E001-E069 全26枚
- **inventory_only カード**: C031-C050 (20枚) の効果定義
- A005 (partial), A060 (needs_resolution) は PR #320 で継続中

## 照合方針準拠
- 単純カード: `wiki_verified` (二次資料照合のみ) で照合済み
- 複雑条件・未確定: `wiki_literature` (文献照合) で照合済み
- 公式資料必須カード: 本PRでは未着手 (別途実施)
- **fail-closed**: 語彙で表現できないものは `needs_resolution` + `unresolved_fields` 保持。散文の note に移して空にするだけの解除は不可（AGENTS.md §3, §11）

## バリデーション
```
$ python scripts/validate_bga_card_specs.py
# エラーなし (errors: [])

$ python scripts/validate_card_catalog.py
# 既存の D159 英名欠落1件のみ（mainと同一。新規エラーなし）

$ python -m unittest discover -s tests -p test_bga_card_specs_builder.py -v
# OK（再現性テスト green）
```
- bga_checked: 413枚 (408 → 413 = +5)
- needs_resolution: 261枚 (265 → 261 = -4)
- inventory_only: 20枚
- partial: 1枚 (A005)
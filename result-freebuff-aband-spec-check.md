# A帯仕様データの文献照合 結果ログ

担当: Freebuff。ブランチ `research/freebuff-ab-spec-check`（基点 `origin/main` = `be4ee45`）。
成果物: `docs/aband-spec-check.md`。**PRは作成まで、マージしない。**

## 第0章 対象の確定（完了）

- 依頼文の指示どおり、まず既照合PRの対象一覧を確認して重複を避けた。
  - #387 `docs/impl-source-check.md`: 実装済み75枚（**A帯29枚**・B帯21枚）。
  - #394 `docs/impl-source-check-2.md`: A帯・B帯は対象0（C083/C147のみ）。
  - #402 `docs/dband-spec-check.md`: D帯46枚（B008をtravelingの校正用例に使うのみ）。
- したがってA帯の対象は 168 − 29 = **139枚**。
- 文献: BGAアグリコラカード情報wiki の個別ページ（基本情報・強化/品物/役割/勝利点・効果・寸評）を主軸に、
  英語一覧（id付き効果文）・日本語一覧（効果文）・`data/bga-card-base-ledger.csv`（費用・印刷点）で補完した。
  336ページを取得し、`cardBody` 内の「基本情報」表の No. で同定（1ページに人気記事ブロックが混在するため）。
- `?p=` の遷移先が別カードになっているものが10枚あり、そのうちA帯5枚（A005, A010, A013, A023, A062）は
  一覧側の効果文で同定した。

## 第1章 A帯139枚の照合（完了）

- 判定: **一致86・食い違い33・表現不足11・未確定9**。
- 費用・印刷点は139枚すべて台帳と一致（不一致0）。前提の欠落は13枚。
- 文献で読みが確定した修正は **19枚**: 前提の補完13、語彙名3（A070/A082/A084）、効果3（A004/A014/A146）。
- テスト契約（`data/bga-card-tests/`）が旧読みを前提にしている **25枚** は、文献の読みと修正案を
  `docs/aband-spec-check.md` 第4章に残して**データを変更していない**。
- 未確定9枚（A020, A022, A036, A060, A061, A064, A071, A092, A103）は、費用のラウンド依存・語彙不足・
  意図的な保留（フリーアクション窓）で、質問Q-1〜Q-9に分離した。

## 第2章 途中で判明した制約と方針変更（重要）

最初は `data/bga-card-specs/*.json` を直接編集したが、GitHub Actions の `bga-spec-contract`
（`tests/test_bga_card_specs_builder.py`）が失敗した。仕様JSONは `scripts/build_bga_card_specs.py` の表から
生成され、コミット済みファイルが再生成結果と一致することを要求する。加えて各カードには
`data/bga-card-tests/` のテスト契約がある。このため方針を変えた。

1. JSONの直接編集をやめ、**ビルダーの表（PREREQ・effects）を修正**して `--force` で再生成した。
2. テスト契約が旧読みを前提にしているカードは、データを変更せず「保留」として文献の読みと修正案を残した
   （推測でテスト契約を書き換えない）。
3. ビルダー管理外の手書きスナップショット3枚（A070/A082/A084）だけはJSONを直接修正し、
   `review.checked_by`/`checked_on` はビルダーの既定値に戻した。

## 第3章 検証（完了）

- `python scripts/build_bga_card_specs.py --force` → `{"written": 234}`。全生成カードで再生成が一致。
- `python -m unittest discover -s tests -p "test_bga_card_specs_builder.py"` → 3 tests OK。
- `python scripts/validate_bga_card_specs.py` → exit 0（`spec_records` 688、errors 空）。
- 変更: `scripts/build_bga_card_specs.py` と A帯19枚の仕様JSON（＋文書）。
- `src/`・テスト・台帳・語彙文書は無変更のため `cargo` は実行していない（Rust差分なし）。

## 現在地

- A帯は完了。PR #409 を作成済み（マージしない）。B帯は親ブランチの上に文書のみで積む。

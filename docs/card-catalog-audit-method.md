# A〜Eカード台帳：公式照合オーバーレイと検証ゲート

更新日：2026-11-06。対象：Issue #115。**これは公式カードの全件照合完了を意味しない。**

## 目的とデータ境界

- `card-catalog-ae-preliminary.csv`：A〜Eの全840件を収めた**自動一次分類**。名前・効果系統・推定難度・非公式参考URLを記録する。これだけではカードを配布してはならない。
- `card-catalog-ae-audit.csv`：一次台帳に `card_id` で1:1対応する**公式照合のための別台帳**。現在は全840行を `unverified` として初期登録してあり、推測した人数条件や出典を作っていない。
- `scripts/validate_card_catalog.py`：両台帳の整合性、必要証拠、人数適格性の機械的ゲート。出力 `verified_eligible_by_players` は**公式資料を照合済みの候補数**であり、実際にコード実装済み/4人対戦で配布可能とは別。配布にはカード効果テスト・ゲームエンジンの実装・デッキフィルタをさらに必要とする（#105）。

## 列の定義

- `card_id`：A001〜E168の不変の主キー。
- `card_type_normalized`：`occupation` / `minor_improvement` / `multi_type`。一次台帳の型を構造化しただけで、兼用種別の最終確定を意味しない。
- `eligible_at_2` / `eligible_at_3` / `eligible_at_4`：`yes`, `no`, `unverified`。公式確認までは必ず `unverified`。
- `review_status`：`unverified` / `reviewed-needs-official` / `verified` / `rejected`。最終確定ではない記録を`verified`にしない。
- `reference_type`：`secondary_catalog` / `official_pdf` / `official_print` / `official_other`。非公式サイトだけなら `secondary_catalog` に留める。
- `primary_reference_id`：公式の印刷カードID/版・文書・該当箇所を追跡できる安定識別子（例：`printed-card:A005:edition-2016`）。正式照合した事実を持たずに埋めない。
- `primary_reference_url`：公式デジタル資料の検証可能なURL。印刷カード照合の場合は空欄でもよいが、出典IDは必須。
- `source_language`、`ruleset_version`（常に `2016-revised`）、`reviewed_by`、`reviewed_on`（ISO日付）、`review_note`：照合の来歴。

`verified` の必須条件：全人数適格が yes/no に決定し、公式の出典種別、個別参照ID、言語、監査者、照合日があり、デジタル出典ならURLがあること。一次台帳の英語名が空欄の場合も英名を公式照合してから確認すること。CIはこれらの**記入漏れ**を検出するが、**URL内容やカードルールの真偽を自動証明するものではない**。

## 最初に確定している監査上の注意

- A〜E各168行、計840件のIDは構造上存在する。D159の英語名は現時点で欠落。公式表記を確認せず推測補完しない。
- A005は一次分類と既存Rust実装の効果の対応が疑わしい。公式原典で費用・発動条件・効果を確認するまでは `review_status=unverified`。
- A060、D025、D059、D060は複合型のため通常の職業/小進歩の40枚ゲートへ機械的に加算しない。
- 一次台帳の参考URLは多くが非公式一覧であり、カード単位の公式典拠ではない。

## CI・手元での確認

```sh
python3 scripts/validate_card_catalog.py --report catalog-audit-report.json
python3 -m unittest discover -s tests -p test_card_catalog_validator.py -v
```

対象CSV/スクリプト変更時に `.github/workflows/card-catalog-audit.yml` が実行され、結果をArtifactsへ保存する。構造不整合や不正なverified指定は失敗扱いにする。未検証カード0枚の状態自体は失敗ではない（正確な未完状態）。

## 残作業・受け入れ

1. 公式カード・Lookoutの一次資料で、カードごとに**人数適格、費用、効果、発動時点、型、名称**を照合し記録する。特にD159・A005・複合4件を優先する。
2. 照合済み件数、2/3/4人ごとの候補数をレポートに出す。未照合を配布可能としない。
3. #105側で公式照合済み候補を実装し、個別効果/相互作用テスト、4人10-7用40+40の実効配布可能枚数を確認する。
4. 40+40の最低ゲートと80+80の実戦目標を別々に監査。公式カード原文・画像は公開リポジトリに収録しない。

**進行状態**：台帳の監査枠組みと自動構造ゲートは実装済み。公式カード全件の個別照合は未完了であり、#115を閉じる条件にはしない。

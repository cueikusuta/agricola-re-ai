# A〜Eカード台帳：公式照合オーバーレイと検証ゲート（2026-11-15 更新）

更新日：2026-11-15。対象：Issue #115 / #308。**これは公式カードの全件照合完了を意味しない。**
2026-11-15 所有者決定に基づき、照合分類・チェックリスト様式・台帳スキーム拡張設計を追加した。

## 目的とデータ境界

- `card-catalog-ae-preliminary.csv`：A〜Eの全840件を収めた**自動一次分類**。名前・効果系統・推定難度・非公式参考URLを記録する。これだけではカードを配布してはならない。
- `card-catalog-ae-audit.csv`：一次台帳に `card_id` で1:1対応する**公式照合のための別台帳**。現在は全840行を `unverified` として初期登録してあり、推測した人数条件や出典を作っていない。
- `scripts/validate_card_catalog.py`：両台帳の整合性、必要証拠、人数適格性の機械的ゲート。出力 `verified_eligible_by_players` は**公式資料を照合済みの候補数**であり、実際にコード実装済み/4人対戦で配布可能とは別。配布にはカード効果テスト・ゲームエンジンの実装・デッキフィルタをさらに必要とする（#105）。

## 列の定義（現行・2026-11-15時点）

- `card_id`：A001〜E168の不変の主キー。
- `card_type_normalized`：`occupation` / `minor_improvement` / `multi_type`。一次台帳の型を構造化しただけで、兼用種別の最終確定を意味しない。
- `eligible_at_2` / `eligible_at_3` / `eligible_at_4`：`yes`, `no`, `unverified`。公式確認までは必ず `unverified`。
- `review_status`：`unverified` / `reviewed-needs-official` / `verified` / `rejected`。最終確定ではない記録を`verified`にしない。`reviewed-needs-official` は内部確認済みだが公式未確認であり、**`eligible_at_* = yes` を付けられない**（検証スクリプトが失敗させる）。
- `reference_type`：`secondary_catalog` / `official_pdf` / `official_print` / `official_other` / `wiki_verified` / `wiki_literature`。非公式サイトだけなら `secondary_catalog` に留める。
- `primary_reference_id`：公式の印刷カードID/版・文書・該当箇所を追跡できる安定識別子（例：`printed-card:A005:edition-2016`、`wiki:bga:A005:cost-effect`）。正式照合した事実を持たずに埋めない。
- `primary_reference_url`：公式デジタル資料・認められた二次資料の検証可能なURL。印刷カード照合の場合は空欄でもよいが、出典IDは必須。
- `source_language`、`ruleset_version`（常に `2016-revised`）、`reviewed_by`、`reviewed_on`（ISO日付）、`review_note`：照合の来歴。
- `source_summary_ja`：**二次資料（`wiki_verified` / `wiki_literature`）の場合必須**。根拠記述の日本語要約。

`verified` の必須条件：全人数適格が yes/no に決定し、公式出典種別または認められた二次資料種別、個別参照ID、言語、監査者、照合日があり、デジタル出典または二次資料ならURL＋日本語要約があること。一次台帳の英語名が空欄の場合も英名を公式照合してから確認すること。CIはこれらの**記入漏れ**を検出するが、**URL内容やカードルールの真偽を自動証明するものではない**。

公式資料の区分と1枚あたりの照合項目は [公式資料の入手可能性と照合手順](card-catalog-official-sources.md) を正本とする。

## 照合分類の正文化（2026-11-15 所有者決定 §1-4 に対応）

次の4区分の判定条件と台帳での表現を定義する。命名は実装判断だが、意味は所有者決定1〜4と1対1に対応させる。

### (a) 単純カード（`verification_category = simple`）
- **定義**：効果が即時・常時・条件付きのいずれかで、他プレイヤーへの影響・費用の置換/免除・他効果との重なり・発動時期による結果変動・未確定仕様が実行結果に触れることのないカード。
- **照合方法**：Wiki等の二次資料（BGA Wiki 等）で出典記録（`primary_reference_url` + `source_summary_ja`）を付けて `reference_type = wiki_verified` とすることで照合済み扱いとする。
- **台帳表現**：`review_status = verified`、`reference_type = wiki_verified`、`source_summary_ja` 必須。

### (b) 他プレイヤー関与効果（`verification_category = multiplayer_effect`）
- **定義**：手札の移動、相手の選択、左隣への影響など、他プレイヤーが関わる効果を持つカード。AIが自己照合で進行し、人間ルート外。
- **照合方法**：AIが実装・検証を行い、公式資料または文献照合で動作確定後に `reference_type = official_print` / `official_pdf` / `official_other` / `wiki_literature` として `verified` にする。
- **台帳表現**：`review_status = verified`、該当する `reference_type`、`source_summary_ja`（二次資料の場合必須）。

### (c) 複雑条件・未確定（`verification_category = complex`）
- **定義**：費用の置換/免除、他効果との重なり、発動時期で結果が変わるもの、仕様の未確定点が実行結果に触れるもの。
- **照合方法**：まず文献調査（BGA Wiki・公式ルールWeb版・公式errata等）。動作が判明すれば `reference_type = wiki_literature` または公式資料として `verified` にする。
- **台帳表現**：`review_status = verified`、`reference_type = wiki_literature` または公式資料、`source_summary_ja`（二次資料の場合必須）。

### (d) 文献未解決（`verification_category = unresolved`）
- **定義**：文献を調べても決まらないもの。人間チェックリスト行き。配布は fail-closed のまま。
- **照合方法**：人間への質問チェックリストとして提出。決着まで `unverified` のまま。
- **台帳表現**：`review_status = unverified`、`verification_category = unresolved`、`review_note` にチェックリストIssueへの参照を記録。

> **注意**：`verification_category` 列は**台帳スキーム拡張設計**（後述）で追加予定。現行台帳には未実装。行値の書き換えは後続カードが担当。

## 人間チェックリスト行形式の文書化（所有者決定 §1-4 の6項目形式）

文献未解決（d）のカードについて、以下の6項目形式で GitHub Issue または対象Issueへのコメントとして提出する。後続カードがそのまま使える手順とする。

| 項目 | 内容 | 記入例 |
|---|---|---|
| **1. カード名とID** | 例：「粘土積み場 (A005)」 | `A005 粘土積み場` |
| **2. 決まらない論点** | 具体的に何が不明か | `費用の置換ルール（食料1で粘土1を得られるか）` |
| **3. 候補の解釈と動作変化** | 各候補を選んだ場合のゲーム動作の差異 | `候補A: 食料1→粘土1（従来通り）／候補B: 食料2→粘土1（置換不可）` |
| **4. 調べた文献** | 出典名、URL、根拠記述の日本語要約 | `BGA Wiki: https://... 「食料1で粘土を得る」と記載あり` |
| **5. それでも決まらない理由** | 文献の曖昧さ、矛盾、未記載等 | `公式errataとBGA Wikiで記述が食い違う／置換の可否が明記されていない` |
| **6. 回答欄** | 候補から選択または自由記述 | `☐ 候補A / ☐ 候補B / ☐ その他: ______` |

**提出先**：対象の親Issue（例：#115）へのコメント、または専用Issueを作成して親Issueにリンク。テンプレートは `docs/checklist-template.md` 参照（後述で作成）。

## 台帳スキーム拡張設計（行値の書き換えはしない）

`docs/card-catalog-ae-audit.csv` に以下の列を**追加設計**する。既存840行の既存値とその意味を壊さない後方互換とする。

| 列名 | 型・値域 | 必須条件 | 説明 |
|---|---|---|---|
| `verification_category` | `simple` / `multiplayer_effect` / `complex` / `unresolved` | `verified` または `reviewed-needs-official` のとき必須 | 上記 (a)〜(d) の分類 |
| `source_summary_ja` | 文字列（任意長） | `reference_type` が `wiki_verified` / `wiki_literature` のとき必須 | 根拠記述の日本語要約 |
| `literature_resolution` | `resolved` / `unresolved` / `not_applicable` | `verification_category` が `complex` / `unresolved` のとき必須 | 文献照合で解決したか |
| `checklist_issue_url` | URL（文字列） | `verification_category = unresolved` のとき必須 | 人間チェックリストのIssue/コメントURL |

既存列（`review_status`、`reference_type`、`primary_reference_id`、`primary_reference_url`、`source_language`、`reviewed_by`、`reviewed_on`、`review_note`、`eligible_at_*`）は**変更しない**。新列は空欄（デフォルト）から始め、後続の仕分けカードが新体系に沿って書き込む。

## 構造検証と公式照合進捗を分ける

`scripts/validate_card_catalog.py` は次の2つを別項目で報告する。

- `structure_validation`：840行の形式・ID・列挙値・種別整合が正しいか。
- `official_verification_progress`：公式照合が何枚済みか（`verified` として記録された件数）。

構造検証が成功しても、公式照合済みとは扱わない。`high_risk_audit_queue` と `stratified_audit_sample` はレビュー**作業キュー**であり、選ばれた事実は照合の証拠ではない。

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

1. 公式カード・Lookoutの一次資料または認められた二次資料で、カードごとに**人数適格、費用、効果、発動時点、型、名称**を照合し記録する。特にD159・A005・複合4件を優先する。手順と一次資料の区分は [card-catalog-official-sources.md](card-catalog-official-sources.md) に従う。16枚の証拠整理（実装との差異・確認項目つき、`verified` 0枚）は [m10-card-verification-pilot.md](m10-card-verification-pilot.md)。
2. 照合済み件数、2/3/4人ごとの候補数をレポートに出す。未照合を配布可能としない。
3. #105側で公式照合済み候補を実装し、個別効果/相互作用テスト、4人10-7用40+40の実効配布可能枚数を確認する。
4. 40+40の最低ゲートと80+80の実戦目標を別々に監査。公式カード原文・画像は公開リポジトリに収録しない。
5. **本PRの範囲**：台帳スキーム拡張設計、検証スクリプト更新、チェックリスト様式文書化まで。行値の大量書き換えや効果実装は後続カード担当。

**進行状態**：台帳の監査枠組みと自動構造ゲートは実装済み。公式カード全件の個別照合は未完了であり、#115を閉じる条件にはしない。新区分 `wiki_verified` / `wiki_literature` 追加により、単純カードと文献照合で判明したカードはこの環境でも `verified` へ昇格可能になった。

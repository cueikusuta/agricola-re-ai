# A〜Eカード：公式資料の入手可能性と照合手順（Issue #115）

更新日：2026-11-06。**本ファイルは「何を公式根拠として認めるか」の手順書であり、照合結果ではない。照合済みカードは現時点で0枚。**

公式原文・カード画像・カード全文は公開リポジトリへ収録しない（`AGENTS.md` 6）。本ファイルにも転載しない。

## 1. 認める一次資料の区分

台帳の `reference_type` は次のいずれか。`secondary_catalog`（非公式の総合一覧・ファンサイト・Wiki）は**参考情報**であり、`verified` の根拠にできない。

| `reference_type` | 例 | `verified` に使えるか |
|---|---|---|
| `official_print` | Lookout Games 発行の2016改訂版 印刷カード本体（カードIDを目視確認） | 使える |
| `official_pdf` | Lookout Games が配布する公式ルールブック・公式カード一覧（該当ページ/節を特定できる場合） | 使える（`primary_reference_url` 必須） |
| `official_other` | 発行元の公式訂正（errata）・公式サポート回答など、検証可能な公式文書 | 使える（`primary_reference_url` 必須） |
| `secondary_catalog` | 非公式一覧、個人Wiki、まとめサイト | **使えない**（`unverified` / `reviewed-needs-official` のみ） |

`reference_type` が公式でも、**個別カードの該当箇所**を示す `primary_reference_id` が無ければ `verified` にしない。例：`printed-card:A005:edition-2016`、`rulebook:2016-revised:p12`。

## 2. この自動環境で確認できたこと（2026-11-06）

- リポジトリ内に公式カード原文・画像・公式PDFは存在しない（意図的な非収録）。
- 自動エージェントは公式印刷カードを目視できず、ライセンスされたデジタルカードデータにもアクセスできない。公式PDFの内容を照合の根拠として引用・転載することは方針上も禁止している。
- そのため、**この環境だけで `verified` へ昇格できるカードは0枚**である。推測・記憶・非公式一覧からの補完で `verified` を作らない。
- 非公式総合一覧（一次台帳の `source_url`）は838行で共通であり、**カード単位の公式典拠ではない**。`secondary_catalog` として扱う。

結論：残作業は「公式資料にアクセスできる人間または信頼できる一次資料の確認者」による個別照合であり、コードとCIはその記録枠と検証ゲートだけを提供する。

## 3. カード1枚あたりの照合項目

`verified` にする前に、次を公式資料で確認する。すべて `docs/card-catalog-ae-audit.csv` の該当行へ記録する。

1. **カードID**：A001〜E168 の安定キーと印刷カードの対応（デッキ、番号）。
2. **名称**：日本語表記・英語表記。一次台帳の英語名が空欄（例：D159）の場合は、公式表記を確認してから埋める。推測補完は禁止。
3. **種別**：職業 / 小進歩 / 複合（`multi_type`）。兼用カードは通常の40+40へ機械的に加算しない。
4. **人数適格**：`eligible_at_2` / `eligible_at_3` / `eligible_at_4` を `yes` / `no` に確定。
5. **費用・前提**：コスト、プレイ条件、人数条件。
6. **効果と発動時点**：即時 / 常時 / 条件付き / 収穫時 / 終了時得点、任意か強制か、対象範囲。
7. **来歴**：`reference_type`、`primary_reference_id`、`primary_reference_url`（公式デジタル資料の場合）、`source_language`、`reviewed_by`、`reviewed_on`。

## 4. 進行手順

1. 高リスク記録から照合する。優先キューは検証スクリプトの `high_risk_audit_queue` に出る。
   - A005：一次分類と現行Rust実装（`OnPlayGainPerStored`）の相違が疑われる（`#115`・`docs/four-player-card-rollout-m10.md`）。
   - D159：一次台帳の英語名が空欄。
   - A060 / D025 / D059 / D060：複合種別。
2. `stratified_audit_sample`（既定12件、`--sample-size` で変更可）の作業キューを使い、デッキ×種別の層化サンプルで偽陽性・偽陰性を測る。
3. 照合できた行だけ `review_status` を `verified` にし、人数適格を `yes`/`no` に確定する。確認途中の行は `reviewed-needs-official` に留め、`eligible_at_*` は `yes` にしない（検証スクリプトが拒否する）。
4. 全840枚の照合完了を `#115` の完了条件にしない。照合済み範囲と未照合範囲を割合で別表示する。

## 5. 機械的な検証との関係

`scripts/validate_card_catalog.py` は構造と来歴の記入漏れだけを検証する。**URLの内容やカードルールの真偽は自動証明しない。**

- 構造検証：ID重複・欠番、名称、列挙値、種別、840行の一致。
- 来歴検証：`verified` に公式出典種別・個別参照ID・言語・監査者・照合日（デジタルならURL）が揃うこと。
- 適格検証：`verified` 以外の行で `eligible_at_*` が `yes` にならないこと。
- 集計：`verified_eligible_by_players` / `verified_4p_card_count_gate` / `g2_target_4p_80_each`。

コード側の照合ゲート（`src/card_pool.rs`、Issue #139）は、この台帳の状態を取り込むまで **4人配布可能枚数を0と報告する**。両者が一致していることを `tests/m10_card_pool.rs` が ID と正規化種別で突合する。

## 6. 現時点の状態

| 項目 | 値 |
|---|---:|
| 台帳の構造検証 | 840 / 840 行 OK |
| `verified` | **0** |
| `reviewed-needs-official` | 0 |
| `unverified` | 840 |
| 4人戦で公式配布可能な職業 / 小進歩 | 0 / 0 |
| G1（40+40） | 未達 |
| G2（80+80） | 未達 |

原因は「公式資料の照合が未実施」であり、データ形式やCIの問題ではない。

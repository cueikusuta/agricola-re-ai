# 4人戦カード拡充：既知ブロッカーと 40+40 ゲート（Issue #105 / #115 / #91）

更新: 2026-11-08。**本ファイルは未確認事項の記録であり、照合結果でも実装計画の確定でもない。** 公式照合済みカードは現時点で0枚、4人戦へ適法に配布できるカードも0枚である。

## 1. 現状の数字（台帳から再計算）

`python3 scripts/card_inventory_report.py` と `python3 scripts/validate_card_catalog.py` の出力、および `docs/card-catalog-ae-audit.csv` から。

| 項目 | 値 |
|---|---:|
| A〜E 台帳 | 840枚（各168枚） |
| `review_status=verified` | **0枚** |
| 効果実装済み（台帳の `implementation_reference`） | 34枚 |
| 4人戦で適法に配布可能（`verified` かつ `eligible_at_4=yes`） | **0枚** |
| M10 初期ゲート（職業40＋小改善40）までの不足 | 職業40・小改善40 |

`validate_card_catalog.py` は「`verified` は公式 `reference_type` と `primary_reference_id`・`source_language`・`reviewed_by`・`reviewed_on` を必須とし、人数適格が `unverified` のままでは `verified` にできない」ことを既に強制している。台帳の全行が `unverified` である限り、**4人戦プールのゲートは閉じている**。

## 2. この環境で照合できない理由

`docs/card-catalog-official-sources.md` 2節のとおり、自動エージェントは公式印刷カードを目視できず、公式PDF・カード原文を公開リポジトリへ取り込むことも禁止されている（`AGENTS.md` 6）。したがって**この環境で `verified` へ昇格できるカードは0枚**であり、推測・記憶・非公式一覧での補完は行わない。

## 3. 既知の高リスクカード台帳

`docs/card-verification-blockers.json` が正本。Issue #105 で名指しされた6枚について、台帳が既に持っている情報・未確定事項・必要な一次資料・実装ブロッカー・次の作業を記録している。

| カード | 種別 | 既知のリスク | 必要な一次資料 |
|---|---|---|---|
| A005 | 小進歩 | 他プレイヤー・手札操作を含むと推定。difficulty 5 は自動一次分類で事実ではない | 印刷カード（デッキA・A005） |
| D159 | 職業 | **英語名が空欄**。台帳の安定キーと印刷カードの対応が未確認 | 印刷カード（デッキD・D159） |
| A060 | 複合 | 複合カードの数え方（職業/小進歩のどちらに数えるか）が未確定 | 印刷カード（デッキA・A060） |
| D025 | 複合 | 同上 | 印刷カード（デッキD・D025） |
| D059 | 複合 | 同上 | 印刷カード（デッキD・D059） |
| D060 | 複合 | 同上 | 印刷カード（デッキD・D060） |

複合カード（`multi_type`）は 40+40 の枚数へ機械的に加算しない。数え方をルール典拠つきで確定するまで配布プールに入れない。

`docs/card-verification-blockers.json` は**照合待ちキューではない**。キューは `scripts/card_inventory_report.py --cards-csv`（PR #172）が生成する。本ファイルは「すでにリスクが分かっているカード」だけを列挙し、CI が次を検査する。

```sh
python3 scripts/card_verification_blockers.py
```

- 登録カードが台帳に存在すること。
- 登録カードが `verified` になっていないこと。
- 登録カードの人数適格が `yes` になっていないこと。
- 各エントリに「未確定事項」と「必要な一次資料」があること。
- `four_player_pool_gate_open` は単に適格カードが1枚存在するだけでは開かない。A〜E公式照合台帳と一次分類の実装参照をカードIDで突き合わせ、**4人適格・公式verified・実装済み**の職業40枚以上と小改善40枚以上が両方揃ったときだけ真。複合カード・未実装・未照合は数えず、不足数も別フィールドへ出す。

ブロッカーを解消したカードは、**先に登録から外し、その根拠を台帳へ記録してから** `verified` にする。順序を逆にすると CI が落ちる（意図した fail closed）。

## 4. 照合が可能になったときの手順

1. `docs/card-catalog-official-sources.md` 3節の7項目を公式資料で確認する。
2. `docs/card-catalog-ae-audit.csv` の該当行だけを更新する（`review_status`・`eligible_at_*`・`reference_type`・`primary_reference_id`・`source_language`・`reviewed_by`・`reviewed_on`）。
3. `python3 scripts/validate_card_catalog.py` と `python3 scripts/card_verification_blockers.py` を通す。
4. 40+40 に到達したかを `card_inventory_report.py` / `card_pool_report` で確認する。
5. 実装は**照合済みカードだけ**を対象に、効果フック単位の小さなPRで行う。各カードに「発動・非発動・費用・人数条件」のテストを付ける。

## 5. 未解決（ユーザー判断・外部資料待ち）

- **#91**: A055 の支払い資源と改善後効果の適用範囲。推測で確定しない。公式カードまたは公式回答の確認待ち。
- 複合カードの数え方（A060 / D025 / D059 / D060）: ルール典拠の確認待ち。
- D159 の公式英語名: 印刷カードの確認待ち。
- 40+40 の達成には、公式資料を確認できる人間または信頼できる一次資料の確認者が必要。

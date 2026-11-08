# BGA向けカード実装仕様データ（Issue #202）

## 目的と既存資料
カード実装の仕様正本となる*構造化データ*を段階的に構築する。ID・日英名称の既存840行は `docs/card-catalog-ae-preliminary.csv` に既に存在するので再入力しない。`docs/card-catalog-ae-audit.csv` は **公式照合専用**であり、本仕様では更新・自動昇格しない。

参考元は第三者のBGA向けWiki（BGAやLookoutの公式一次資料ではない）:
- 日本語: https://agricola.cloudfree.jp/cardlist/
- 英語（IDつき）: https://agricola.cloudfree.jp/cardlist_en/

外部のカード説明全文・画像は収録しない。自作の構造化処理と短い説明・出典ポインタに限定する（`AGENTS.md` §6）。サイト側の利用規約と取得方法に従うこと。

## 保存形式

| 内容 | 場所 | 更新単位 |
|---|---|---|
| 840件の既存基本台帳 | `docs/card-catalog-ae-preliminary.csv` | 既存ID保持 |
| 840件の公式照合台帳（別管理） | `docs/card-catalog-ae-audit.csv` | #115 |
| BGA参照の効果仕様 | `data/bga-card-specs/<ID>.json` | 1カード/ファイル |
| 効果テストの期待値 | `data/bga-card-tests/<ID>.json` | 1カード/ファイル |
| 効果仕様スキーマ | `schemas/bga-card-spec.schema.json` | v1 |
| テスト契約スキーマ | `schemas/bga-card-test.schema.json` | v1 |

**存在するJSONファイル数＝完成カード数ではない。** 全840IDの基本情報は既存CSVに保持し、個々のJSONファイルは実際に仕様を整理したカードについてのみ作成する。未作成カードは当然 `inventory_only` とみなす。

## 仕様フィールドの要点

- `identity`: 種別・日英名称（既存CSVと同一のIDで突合）。
- `bga`: Wiki掲載・BGA使用可能・禁止区分・基本セット・人数別適格（`unknown` 可）・言語別参照URL・確認日。
- `play`: 使用アクション、コスト・前提・印刷得点。`null` は **未確認**、`[]` は **無料/前提なしが確認済み**、`0` は **0点確認済み**。これらを混同しない。
- `effects`: 各発動イベントの条件、原子操作の順序、選択者、発動頻度、持続期間、状態参照時点。`null` は未調査、`[]` は効果なし確認済み。
- `post_play`: 使用後のカード移動先。手札の左隣移動など、原子効果から独立して記録。
- `review.status`: `inventory_only` / `partial` / `bga_checked` / `needs_resolution` / `blocked`。
- `review.implementation_status`: `not_started` / `in_progress` / `implemented` / `tested`。BGA情報照合とは別軸。
- `review.test_case_ids`: 期待テストID。正常発動に加え非発動・境界・保存復元・秘匿・相互作用を可能な範囲で段階追加。

`bga_checked` は「BGA向けWikiの仕様データとして手作業で内容確認済み」を意味し、**Lookout公式照合済み `verified` とは別**。この状態だけで既存の `verified_eligible_pool` を通してはならない。BGA実験プールを追加する場合は、明示的なオプトインの専用モードを #105 側で設けること。

### 効果式の表現

`effects[].steps[].params` は操作ごとのパラメータ。数量は定数または式木で表す。最低限 `resource_count`、`floor_div`、`add`、`multiply`、`min`、`max` を表現可能とする。データだけでRust効果を実行できると仮定せず、**Rustエンジンの命令への写像**を明記する。未対応命令は #105 に別Issueを作成して保留し、無効果のダミーを登録しない。ターン・収穫・手札・他プレイヤー効果は状態参照時点と決定主体を必ず意識する。

## 具体例（A005）

`data/bga-card-specs/A005.json` と `data/bga-card-tests/A005.json` は**未完成のサンプル**。Wiki日本語版にある左隣へのカード移動が英語版の説明から抜けているため、日英を併読し、プレイ後移動を独立に表現した。所持レンガ6→獲得3→計9という効果単位の期待値を記録したが、使用コスト・前提・得点・人数条件は未照合。通常の合法プレイテストでも標準デッキ採用の証拠でもない。

## 作業順と出荷のゲート

1. 840行の名称/ID/種別を既存CSVで固定し、BGA掲載・BGA未実装・禁止フラグを参照ソースごとに取り込む。
2. A001/A005/A012/A060/D159のような特殊性を持つカードでスキーマ表現力を確認する。
3. 8〜12枚ごとに効果と使用条件を構造化し、テスト契約をセットで追加する。日英差・コスト不明・カード位置不明は `needs_resolution` として分ける。
4. 各バッチから実装可能なカードを #105 側の独立PRで順次実装し、Rustテストに接続する。全840枚のデータ整備完了まで実装を待つ必要はない。
5. `bga_checked` は、コスト/前提/得点/効果/移動先/人数と参照情報・テストケースを確認できたもののみ許容する。単なるJSON存在やカタログ一致は合格ではない。
6. CIでID結合・スキーマ・参照・テストケースの整合性を検証し、`840基本台帳`、`仕様レコード数`、`bga_checked枚数`、`実装枚数`、`公式照合枚数`を**別々に**表示する。

公式ルール（ゲーム全体の基礎仕様）の正本は従来の `AGENTS.md` 方針を維持するが、カード固有のBGA相当挙動は本Issueの独立照合ステータスを使う。BGAと公式で相違が見つかった場合、データ上で両者の区分と影響を記録し、黙って片方を上書きしない。

# PROGRESS

## Freebuff — ラウンド開始時獲得カード（ブランチ `feat/freebuff-roundstart-gain-impl`）

- 基点: `origin/main`。成果物: `result-freebuff-roundstart.md`、PR（作成まで・マージしない）。
- 対象の洗い出し: `data/bga-card-specs/` の `trigger: "round_start"` 36枚のうち、資材・食料の獲得を含むもの。
- 着手時の確認: main 実装済み14枚、他担当 PR（#502 on_play 獲得 / #503 終了時得点 / #321 ラウンドスペース予約台帳）を対象外にした。
- 現在地: **A081 一時保管庫を実装済み**（band A 章）。他帯に新規実装対象なし。テスト5件成功。
- 新規実装: A081（`AfterWorkerStackOnCard` + `StartRoundReleaseCardGoods`、既存 `card_accumulations` を使用）。
- 対象外の一覧と理由（20枚）は `result-freebuff-roundstart.md` の「対象から外したカード」に記載。
  - 予約配置が要る7枚は PR #321 の `delayed_goods` と重複するため触らない。
  - C046 は round start の任意選択チャネルが無いため保留。
  - `needs_resolution` 6枚は実装停止ゲート。
- 再開はこの節から。マージはしない。


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

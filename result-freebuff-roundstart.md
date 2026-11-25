# Freebuff ラウンド開始時獲得カード実装 — 結果

担当ブランチ `feat/freebuff-roundstart-gain-impl`（基点 `origin/main`）。**PR は作成まで。マージしない。**

## 範囲の決め方

- 対象の洗い出しは `data/bga-card-specs/` の `effects` が `trigger: "round_start"` の36枚から行い、
  そのうち資材・食料の獲得（`gain_resource` または同等の `other`）を含むものを対象とした。
- 着手時に `origin/main`（`1576429` 時点の取得）と open PR 全件を確認し、
  main に実装済みのカードと他担当が PR 中のカードは対象外にした。
- `on_play` の獲得部分（PR #502）と終了時得点系（PR #503）は他担当の範囲なので触っていない。

## 実装したカード（1枚）

| ID | カード名 | 種別 | コスト | 印刷点 | 実装内容 |
|---|---|---|---|---|---|
| A081 | 一時保管庫 (Interim Storage) | 小進歩 | food 2 | 0 | 粘土坑→wood 1、葦原→clay 1、東西の石切場→reed 1 をカード上に保管。ラウンド7/11/14の開始時にカード上の品物を全てストックへ移す。 |

- 保管には既存の `PublicState::card_accumulations`（player → card → good）をそのまま使い、新しい永続フィールドを増やしていない。
- 新規 `CardEffect` は `AfterWorkerStackOnCard { space, good, amount }` と
  `StartRoundReleaseCardGoods { rounds }` の2つ。`verified_band_a_cards()` から既定レジストリへ登録。
- 台帳は after-worker と round-start の dispatch にだけ `DispatchContext::card_accumulations` として通す。
  既存の `dispatch_after_worker` / `dispatch_start_round` は互換ラッパーとして残し、台帳なしでは no-op。
- `state_hash` の v5 タプルは不変（`card_accumulations` は従来どおり `integrity_hash` 側）。

## 対象から外したカード（20枚）

### 将来のラウンドスペースへの予約配置が要る（7枚・他担当 PR #321 の `delayed_goods` 台帳と重複）

A074 厩の木 / A111 壁装飾人 / A120 レンガ大工 / B093 腹心の友 / B096 養樹園の指物師 /
B119 杣人 / B125 借地農夫

これらは `round_start` の獲得条件が `round_space_has_placed_good` / `wood_on_this_round_space`
（過去のトリガーで将来ラウンドのスペースに置いた品物）で、置く側の機構が main に存在しない。
PR #321（`feat/deepai-cards-c070-120-08`、未マージ）が `Game.delayed_goods` として実装中であり、
二重実装を避けるため対象外。E047 樹液採取器も同じ機構が必要（かつ `needs_resolution`）。

### 資材・食料の獲得ではない（6枚）

- A019 手押し鋤: ラウンドスペースから畑を取る（獲得ではない）。
- A022 電報: 追加の人物を置く。
- B018 草地鍬 / B020 鎖状農具: 畑を耕す。
- E028 しおり: 職業を出すアクションの付与。
- E088 柵作りの達人: 木材を払って柵を建てる任意効果。

### ラウンド開始の任意選択チャネルが必要（1枚）

- C046 マンドリン: 野菜1を任意で払い1点＋次の2ラウンドの食料。round start で PendingDecision を
  生成しても解決窓がない（`Game::start_round` は dispatch の Transition を破棄する）ため保留。
  A090/A151/A152/B097/B135/C064 が dormant のままなのと同じ制約。

### `needs_resolution` ゲート（6枚）

A089 厩設計士 / D091 鋤手 / E056 ローマ土器 / E126 収税官 / E162 起業家 / E168 見習い調教師

仕様JSON の `review.status = needs_resolution` は実装停止ゲートのため、推測実装しない。
（E047 は上の予約配置グループにも重複して該当。）

## main で実装済みだったラウンド開始時獲得カード（14枚・対象外）

A049 営巣地 / A090 鋤職人 / A151 吟遊楽人 / A152 夜学生 / A157 ボヘミアン /
B057 食器洗い場 / B097 学者 / B106 清廉潔白な人 / B107 召使 / B114 子なし /
B118 小作人 / B135 栄養士 / C064 穀物蒸留酒製造所 / C067 ミネラルフィーダー

## テスト結果

- `cargo test --test roundstart_gain_ch01_a` → 5 passed
  - 定義（種別・コスト・保管マッピング・解放ラウンド・レジストリ一致）
  - 4スペースの保管先が仕様どおり（Forest と他人の使用では保管しない）
  - ラウンド7/11/14のみ解放、解放後に台帳が空になり二重付与しない
  - 台帳を渡さない旧経路が no-op でパニックしない
- `cargo test --test ch06_round_start_gains` → 14 passed（既存のラウンド開始章に回帰なし）
- 詳細は PR 本文の Actions チェック欄に記す。

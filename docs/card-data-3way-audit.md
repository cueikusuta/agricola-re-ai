# カードデータ3種の横断監査（2016年改訂版・全840枚）

対象: 台帳 `data/bga-card-base-ledger.csv`（840行）／仕様 `data/bga-card-specs/<ID>.json`（695件）／
テスト契約 `data/bga-card-tests/<ID>.json`（695件）。
監査ブランチ: `research/deepai-ledger-spec-audit`。ルールの正本は2016年改訂版Lookout公式ルール、
カード面の文言は日本語版カード画像と [Agricola カードDB](https://agricola.cloudfree.jp/)（以下「文献」）で確認した。
本作業はデータと文書のみで、Rustコードには触れていない。

## 1. 目的

同じカードについて3つのデータ源が別々に人手で作られているため、

1. 同一性（ID・名前・帯・種別）
2. 費用
3. 出せる条件（前提）
4. 印刷勝利点
5. 効果の構造（トリガ・対象・数量）
6. 片方にしか無いカード／重複

がずれていないかを、840枚すべてについて突き合わせ、**どちらが正しいかをカード面の文言と文献で判定**して
誤っている側を直し、判定できないものは変更せずに日本語のチェックリストへ分離する。

## 2. 方法

- `scripts/audit_bga_card_sources.py`（新規・標準ライブラリのみ・読み取り専用）で3ソースを機械比較する。
  台帳の日本語略記（`職業2`、`木の家`、`羊7未満` など）と仕様の述語（`occupation_count`、`house_is`、
  `animal_count_less` など）は共有の正規化関数で同一視し、まず**表記ゆれを除外**してから差だけを残す。
- 仕様とテスト契約は、仕様が得る品目・点数とテストの期待値の対応、`review.test_case_ids` と実ケースの
  参照整合、`missing_spec`／孤立ケースを検査する。
- 残った差は1件ずつ、文献の「条件／コスト／勝利点／効果」欄とカード画像で正否を決めた。
- 文献ページが無いカード（D025, D159）と、文献にあっても解釈が一意でないものは**変更しない**。

実行:

```
python scripts/audit_bga_card_sources.py            # 人が読む要約
python scripts/audit_bga_card_sources.py --json     # 機械可読
python scripts/validate_bga_card_specs.py           # 既存スキーマ検証（errors は空であること）
```

## 3. 判定基準

| 状況 | 扱い |
|---|---|
| 台帳と文献が一致し、仕様が未反映 | 仕様を直す（語彙で書ける場合） |
| 語彙で書けない条件 | `raw_text` に原文を残し `needs_resolution`。値は推測しない |
| 台帳が文献と不一致 | 台帳を直す |
| 仕様とテストが矛盾（片方だけ古い） | 文献に合う側へ揃える |
| 文献が無い／解釈が複数成立 | **変更しない**。チェックリストへ |

## 4. 全体集計

| 区分 | 枚数 |
|---|---|
| 台帳の行 | 840（A/B/C/D/E 各168） |
| 仕様とテストが揃う | 695（1:1、IDの欠落・不一致なし） |
| 台帳のみ（仕様・テスト未作成） | 145（A119、C071–C120、D070–D120、E070–E120 の一部） |
| 3ソース突き合わせの対象 | 695 |

3ソースが揃う695枚の最終状態:

| 状態 | 枚数 |
|---|---|
| 3ソース一致（修正不要） | 625 |
| 監査で修正した | 66 |
| 値が食い違い、判定できず未変更（§7） | 4（B062, C060, C162, D159） |

修正66枚の内訳は仕様65枚＋台帳のみの B154（E144 は仕様と台帳の両方）。A103 は「3ソースの値は揃ったが
解釈が未決」なので修正側に入れ、未決は §7 Q-03 に残した。
台帳のみの145枚は仕様・テストが未作成（Hermes の S-1 範囲）で、本監査の対象外だが、
台帳の条件・費用・点数は文献と一致することを確認した（§6）。

台帳 840行 vs 文献: **838行一致、2行（D025, D159）は文献ページが404で照合不能**。
この2行については台帳も仕様も「不明」のままで、値を入れていない。

## 5. 章別の食い違いと判定

以下、各行が1件の食い違い。`判定` 欄が「どちらが正しいか」と根拠（文献URL）。
`修正` 欄が実際に加えた変更。修正はすべて該当IDの仕様・テスト・台帳のいずれかで、Rustには触れていない。
台帳だけを直した行（B154）は B帯の表に入れた。

### A帯（001–168）

| ID | 食い違い | 判定（根拠） | 修正 |
|---|---|---|---|
| A005 | 仕様 `play.cost/prerequisites/printed_points` が null のまま | 台帳=食料1/前提なし/0点、文献=1飯/ー/ー で一致。仕様が未反映。文献: https://agricola.cloudfree.jp/?p=3420 | 費用を食料1、前提を空、印刷点を0へ |
| A010 | 仕様の前提が空 | 台帳=文献=「まだ木の家」。文献: https://agricola.cloudfree.jp/?p=3415 | `house_is wood` を追加 |
| A018 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=3407 | `occupation_count 2` を追加 |
| A020 | 仕様の前提が空 | 台帳=文献=「ラウンド3(5)かその前に出す」だが版差の括弧を含み語彙で表現不能。文献: https://agricola.cloudfree.jp/?p=3405 | `raw_text` で記録し `needs_resolution` を維持 |
| A022 | 仕様の前提が空 | 台帳=文献=「ストックに柵が1本以上ある」。柵の在庫を表す述語が無い。文献: https://agricola.cloudfree.jp/?p=3403 | `raw_text` で記録し `needs_resolution` へ |
| A034 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=3391 | `occupation_count 2` を追加 |
| A035 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=3390 | `occupation_count 2` を追加 |
| A036 | 仕様の前提が空 | 台帳=文献=「ストックの木材≧現在のラウンド数」で述語なし。文献: https://agricola.cloudfree.jp/?p=3389 | `raw_text` で記録し `needs_resolution` へ |
| A039 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=3386 | `occupation_count 2` を追加 |
| A040 | 仕様の前提が空 | 台帳=文献=「未使用の農場が7スペース以下」で「以下」の述語なし。文献: https://agricola.cloudfree.jp/?p=3385 | `raw_text` で記録し `needs_resolution` へ |
| A043 | 仕様の前提が空 | 台帳=文献=「家畜1」。種類を問わない家畜数の述語なし。文献: https://agricola.cloudfree.jp/?p=3382 | `raw_text` で記録し `needs_resolution` へ |
| A044 | 仕様の前提が空 | 台帳=文献=「ちょうど職業2」。文献: https://agricola.cloudfree.jp/?p=3381 | `occupation_count 2` を追加 |
| A045 | 仕様の前提が空 | 台帳=文献=「まだ木の家」。文献: https://agricola.cloudfree.jp/?p=3380 | `house_is wood` を追加 |
| A046 | 仕様の前提が空 | 台帳=文献=「ちょうど牧場1」。文献: https://agricola.cloudfree.jp/?p=3379 | `pasture_count 1` を追加 |
| A049 | 仕様の前提が空 | 台帳=文献=「職業1」。文献: https://agricola.cloudfree.jp/?p=3376 | `occupation_count 1` を追加 |
| A052 | 仕様の前提が空 | 台帳=文献=「ラウンド7またはその後に出す」。文献: https://agricola.cloudfree.jp/?p=3373 | `round_in [7..14]` を追加 |
| A061 | 仕様の前提が空 | 台帳=文献=「パン進歩（パンマークの進歩）」。文献: https://agricola.cloudfree.jp/?p=3364 | `has_improvement type=bake_bread` を追加 |
| A062 | 仕様の前提が空 | 台帳=文献=「ストックに小麦2」。文献: https://agricola.cloudfree.jp/?p=3363 | `resource_in_hand grain min=2` を追加 |
| A065 | 仕様の前提が空 | 台帳=文献=「畑3」。文献: https://agricola.cloudfree.jp/?p=3360 | `field_count 3` を追加 |
| A069 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=3356 | `occupation_count 2` を追加 |
| A103 | 仕様は `bga_checked` だがテストに「自動実行不可」ケースが残る | 段の読み（位置対応か累積か）が日英とも曖昧で確定不能。文献: https://agricola.cloudfree.jp/?p=3322 | `needs_resolution` へ戻し未解決項目を記録（テストは fail-closed のまま） |

### B帯（001–168）

| ID | 食い違い | 判定（根拠） | 修正 |
|---|---|---|---|
| B018 | 仕様の前提が空 | 台帳=文献=「職業2とストックに建設資材1」。文献: https://agricola.cloudfree.jp/?p=3239 | `occupation_count 2` + `raw_text`（建設資材1以上） |
| B019 | 仕様の前提が空 | 台帳=文献=「職業1」。文献: https://agricola.cloudfree.jp/?p=3238 | `occupation_count 1` を追加 |
| B021 | 仕様の前提が空 | 台帳=文献=「職業1」。文献: https://agricola.cloudfree.jp/?p=3236 | `occupation_count 1` を追加 |
| B056 | テストがプレイ可否しか見ていない | 文献=「漁の上にある4スペースのいずれかを使うたび食料1」。対象4スペース自体は未確定。文献: https://agricola.cloudfree.jp/?p=3201 | +1食料の形を1ケース追加（スペース集合は未解決のまま） |
| B097 | テスト契約の参照漏れ | テストファイルに2ケースあるが仕様の参照は1件。文献: https://agricola.cloudfree.jp/?p=3160 | `review.test_case_ids` を2件へ |
| B101 | テスト契約の参照漏れ | 同上。文献: https://agricola.cloudfree.jp/?p=3156 | `review.test_case_ids` を2件へ |
| B106 | テスト契約の参照漏れ | 同上。文献: https://agricola.cloudfree.jp/?p=3151 | `review.test_case_ids` を2件へ |
| B109 | テスト契約の参照漏れ | 同上。文献: https://agricola.cloudfree.jp/?p=3148 | `review.test_case_ids` を2件へ |
| B110 | テスト契約の参照漏れ | テスト3件に対し参照1件。文献: https://agricola.cloudfree.jp/?p=3147 | `review.test_case_ids` を3件へ |
| B112 | テスト契約の参照漏れ | 同上。文献: https://agricola.cloudfree.jp/?p=3145 | `review.test_case_ids` を2件へ |
| B120 | テスト契約の参照漏れ | 同上。文献: https://agricola.cloudfree.jp/?p=3137 | `review.test_case_ids` を2件へ |
| B137 | テストがラウンド8/11しか見ていない | 文献=8〜11のラウンドスペースで対応品を得る（対応は仕様が推定）。文献: https://agricola.cloudfree.jp/?p=3120 | ラウンド9（猪）・10（石材）のケースを追加 |
| B154 | 台帳の前提が `none`、仕様は「羊7未満」 | 文献の効果文とカード画像に「羊7頭より少ないときしか出せない」。仕様が正しく台帳が欠落。文献: https://agricola.cloudfree.jp/?p=3103 | 台帳の前提を `羊7未満` へ |

### C帯（001–168）

| ID | 食い違い | 判定（根拠） | 修正 |
|---|---|---|---|
| C003 | 仕様の前提が空 | 台帳=文献=「未配置の人物1」で述語なし。文献: https://agricola.cloudfree.jp/?p=3086 | `raw_text` で記録（`needs_resolution` は維持） |
| C010 | 仕様の前提が空 | 台帳=文献=「大きい進歩2」で大進歩数の述語なし。文献: https://agricola.cloudfree.jp/?p=3079 | `raw_text` で記録し `needs_resolution` へ |
| C011 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=3078 | `occupation_count 2` を追加 |
| C020 | 仕様の前提が空 | 台帳=文献=「ラウンド9またはその後に出す」でラウンド下限の述語なし。文献: https://agricola.cloudfree.jp/?p=3069 | `raw_text` で記録（`needs_resolution` は維持） |
| C038 | テストが旧効果（得点2）のまま | 文献=出したとき他家全員が食料1。印刷点2は別。文献: https://agricola.cloudfree.jp/?p=3051 | 他家食料+1/自点0のケースと、羊2で出せないケースへ差し替え |
| C048 | 仕様の費用が null | 台帳=文献=無料。文献: https://agricola.cloudfree.jp/?p=3041 | 費用を空配列へ |
| C050 | 仕様の費用が null | 台帳=文献=無料。文献: https://agricola.cloudfree.jp/?p=3039 | 費用を空配列へ |
| C153 | 仕様は `bga_checked` だがテストがプレースホルダ | 文献=他家の改築時に木材2→小麦1+食料1+1点。文献: https://agricola.cloudfree.jp/?p=2936 | 交換・辞退・非発動の3ケースへ差し替え |

### D帯（001–168）

| ID | 食い違い | 判定（根拠） | 修正 |
|---|---|---|---|
| D004 | 仕様の前提が空 | 台帳=文献=「職業3」。文献: https://agricola.cloudfree.jp/?p=2917 | `occupation_count 3` を追加 |
| D005 | 仕様の前提が空 | 台帳=文献=「栽培中の畑1」で述語なし。文献: https://agricola.cloudfree.jp/?p=2916 | `raw_text` で記録し `needs_resolution` へ |
| D006 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=2915 | `occupation_count 2` を追加 |
| D007 | 仕様の前提が空 | 台帳=文献=「ラウンド3、6、9または12に出す」。文献: https://agricola.cloudfree.jp/?p=2914 | `round_in [3,6,9,12]` を追加 |
| D008 | 仕様の前提が空 | 台帳=文献=「空の畑1と栽培中の畑2」で述語なし。文献: https://agricola.cloudfree.jp/?p=2913 | `raw_text` で記録（`needs_resolution` は維持） |
| D010 | 仕様の前提が空 | 台帳=文献=「職業5」。文献: https://agricola.cloudfree.jp/?p=2911 | `occupation_count 5` を追加 |
| D019 | 仕様の前提が空 | 台帳=文献=「職業1」。文献: https://agricola.cloudfree.jp/?p=2902 | `occupation_count 1` を追加 |
| D021 | 仕様の前提が空 | 台帳=文献=「家に人物がいない」で述語なし。文献: https://agricola.cloudfree.jp/?p=2900 | `raw_text` で記録し `needs_resolution` へ |
| D028 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=2893 | `occupation_count 2` を追加 |
| D038 | 仕様の前提が空 | 台帳=文献=「職業2」。文献: https://agricola.cloudfree.jp/?p=2883 | `occupation_count 2` を追加 |
| D047 | 仕様の前提が空 | 台帳=文献=「あなたの前のカード*10枚」。文献: https://agricola.cloudfree.jp/?p=2874 | `cards_in_play_at_least 10` を追加 |
| D052 | 仕様の前提が空 | 台帳=文献=「職業1」。文献: https://agricola.cloudfree.jp/?p=2869 | `occupation_count 1` を追加 |
| D078 | 仕様の前提が空 | 台帳=文献=「職業3」。文献: https://agricola.cloudfree.jp/?p=2843 | `occupation_count 3` を追加 |
| D160 | テストが `effect_verified` だけのプレースホルダ | 文献=他家が最初の人物で家族を増やすたび小麦1。文献: https://agricola.cloudfree.jp/?p=2761 | 発動1/非発動1の実ケースへ差し替え |
| D163 | テストが `effect_verified` だけのプレースホルダ | 文献=出したとき石材2、他家の石家改築・石部屋建設で石材1。文献: https://agricola.cloudfree.jp/?p=2758 | 3ケースへ差し替え |

### E帯（001–168）

| ID | 食い違い | 判定（根拠） | 修正 |
|---|---|---|---|
| E038 | 仕様の前提が空 | 台帳=文献=「職業3」。文献: https://agricola.cloudfree.jp/?p=2715 | `occupation_count 3` を追加 |
| E041 | 仕様の前提が空 | 台帳=文献=「あなたの前にカードが5枚ある」。文献: https://agricola.cloudfree.jp/?p=2712 | `cards_in_play_at_least 5` を追加 |
| E042 | 仕様の前提が空 | 台帳=文献=「大きい進歩の井戸」。文献: https://agricola.cloudfree.jp/?p=2711 | `has_improvement kind=well` を追加 |
| E043 | 仕様の前提が空 | 台帳=文献=「厩1」。文献: https://agricola.cloudfree.jp/?p=2710 | `stable_count 1` を追加 |
| E044 | 仕様の前提が空 | 台帳=文献=「畑タイル3」。文献: https://agricola.cloudfree.jp/?p=2709 | `field_count 3` を追加 |
| E046 | 仕様の前提が空 | 台帳=文献=「ちょうど職業2」。文献: https://agricola.cloudfree.jp/?p=2707 | `occupation_count 2` を追加 |
| E126 | 前提が石の家になっている／効果が4つ独立で「毎ラウンド1つ選ぶ」になっていない | 文献=石の家は効果条件。毎ラウンド開始時に木2/レンガ2/葦1/石1から1つ。文献: https://agricola.cloudfree.jp/?p=2627 | 前提から削除し、効果条件へ。`choice` 1効果に再構成。4択+非発動の5ケースを追加 |
| E129 | 前提が日雇い労働者の人物になっている | 文献=それは効果条件（占有スペースを使える権限の前提）。文献: https://agricola.cloudfree.jp/?p=2624 | 前提から削除し効果条件へ移動 |
| E144 | 台帳・仕様の印刷点が未解決 | 文献=「ー」、カード面に点数の丸なし＝0点。文献: https://agricola.cloudfree.jp/?p=2609 | 台帳を0点へ、仕様も0点へ |


## 6. PR #386 との重複確認

PR #386 が仕様を作り直した40枚（A071, A092, A103, A110, A111, A118, A150, A154, A156, A158, A159,
B010, B013, B048, B062, B069, B095, B098, B099, B121, B126, B142, C038, C040, C044, C070, C135,
C153, C162, C167, D079, D160, D163, E002, E030, E052, E053, E144, E148, E150）は、
「#386 の修正が3ソースで整合しているか」という観点で再確認した。結論:

- **38枚は整合**。仕様の費用・前提・点数・効果が台帳と文献に一致し、テストの期待値も仕様と対応する。
  このうち A071, A092, A103, C040, C044, C070, C135, C162, C167, E030, E052, E053, E144, E148, E150 の
  15枚は、効果の実行意味が未確定なため**意図的に fail-closed**（`needs_resolution` と
  `automatic_effect_execution_allowed: false`）で、これは整合の一形態であり欠陥ではない。
- **C153 は不整合**だった。仕様は `bga_checked` なのに、テストが
  「`review_status: needs_resolution`」を期待するプレースホルダのままだった。
  文献の効果（他家の改築時に木材ちょうど2→小麦1＋食料1＋1点）は一意なので、
  仕様はそのままに**テストを交換・辞退・非発動の3ケースへ差し替え**、参照IDも更新した。
- **B062 は要修正のまま未変更**。文献は「『農地』スペースが（誰かのコマで）占有されているとき」だが、
  仕様は `worker_space_used farmland` と `worker_space_used grain_seeds` の連言で、
  「自分が両方を使う」という不可能な条件になっている。正しくは「誰かのコマが農地にある」で、
  それを表す述語が語彙に無いため**推測で書き換えず** §7 Q-01 に送った。
- #386 が意図的に残した既知の残件（小劇場のスペースキー `small_theater` と `traveling_players` の
  不一致など）は本監査の対象外とし、再修正していない。

## 7. 判定できなかった項目（人間の決定待ち）

値は変更していない。必要な根拠・決めるべきことを添える。

| # | カード | 何が決まらないか | 不足している根拠／必要な決定 |
|---|---|---|---|
| Q-01 | B062 フォーク型くわ | 発動条件「農地スペースが占有されている」を、誰のコマでも成立する形で表せない。現仕様は「自分が農地と小麦の種の両方を使う」で不可能 | 「スペース上の人物（所有者不問）」を表す述語の追加。`worker_on_space` を any-player 化するか新設するか |
| Q-02 | C060 陶工の小窯 | 前提「レンガ窯か石窯を返す」は*返却*を伴うコストで、`has_improvement` だけでは返却を表せない | 進歩を手放す費用の表現（`return_improvement` 等）。返却先が共通ストックか捨て札かも公式に確認 |
| Q-03 | A103 港の商人 | 「食料1/2/3以上を取るたび野菜/小麦/葦1」が、取得量に**位置対応**（1→野菜, 2→小麦, 3→葦）か**累積三段**（1以上で野菜, 2以上で小麦も, 3以上で葦も）か。日英とも文面から一意に読めない | 公式FAQ／BGA実装の挙動。決まるまで仕様は位置対応のまま `needs_resolution`、テストは fail-closed |
| Q-04 | C162 森林所有者 | カード自体が全員の使える公共アクションスペースになる効果の符号化。数値（自分4木材／他家3木材＋自分1木材）は文献で確定 | 公共スペースを表す仕様（`grant_public_action_space`）の設計と、既存のスペース集合への追加方法 |
| Q-05 | D159 葦売り | 台帳は条件・費用・点数とも `unknown`、仕様は前提 `[]`・費用 `null`・点数 `null`。両ソースが「不明」で一致しているだけで、正しさは未確認 | 文献ページが404、カード画像も入手できず。公式カード原文／エラッタ |
| Q-06 | D025 | 同上（台帳 `unknown`、仕様 `null`） | 文献ページが404。公式カード原文 |
| Q-07 | A004, A009, A020, C054, E021, E052, E053 | 家畜（羊・猪・牛）や厩を**費用**に取るカードの費用表現がスキーマに無く、台帳は `unknown`・仕様は `null` か `alternative_cost_notes` 止まり | 費用の品目に animal/stable を許すかの決定。C054 は「自分のストックの厩1」で、支払い元の表現も必要 |
| Q-08 | A022, A036, A040, A043, C003, C010, C020, D005, D008, D021 | 前提の語彙不足: 柵の在庫、木材≧ラウンド、未使用農場≦7、家畜の有無、未配置の人物、大進歩の枚数、ラウンド下限、栽培中の畑の枚数、家に人物がいない | 各述語の追加（`fences_in_supply`, `wood_at_least_round`, `unused_farmyard_max`, `any_livestock`, `unplaced_person`, `major_improvement_count`, `round_gte`, `planted_field_count`, `no_person_at_home`）。文言は `raw_text` に保存済み |
| Q-09 | A020 | 前提「ラウンド3(5)かその前」の括弧が版差（3人用/4人用）か例外かを確定できない | 公式ルールの該当ページ。確定まで `raw_text` のまま |
| Q-10 | B056 | 「漁の上にある4スペース」が具体的に哪4スペースか（追加タイルのON/OFFで変わる可能性） | 追加スペース無効時の対象集合の確定。テストは食料+1の形だけを固定 |
| Q-11 | E144 セールスマン | 「建築資材を食料に変換する効果を持つカードを出す／建設する」の監視対象（どのカードが該当するか）が未定義で、仕様の `effects` は `null` | 対象カードの判定規則と、他カードのプレイ・建設を横取りするトリガの設計 |
| Q-12 | B008 / C054 | 日本語名「露店」・英語名「Market Stall」が2枚で重複。B008 は費用小麦1で野菜1、C054 は費用厩1で小麦＋柵→食料5 の別カード | データ誤りではない。ID基準の参照を徹底する方針の確認と、UIでの表示名の区別（例: 英語名併記） |

補足: Q-01〜Q-04, Q-11 は仕様側が `needs_resolution`、Q-05/Q-06 は両ソースが「不明」なので、
いずれも既存の fail-closed 検証（`validate_bga_card_specs.py` の `errors: []`）を壊さない。

## 8. 再現手順

```
# 3ソース比較（読み取り専用）
python scripts/audit_bga_card_sources.py

# 仕様のスキーマ検証
python scripts/validate_bga_card_specs.py     # "errors": [] を確認

# 仕様ビルダの単体テスト
python -m unittest discover -s tests -p "test_bga_card_specs_builder.py"

# 台帳の前提を仕様へ再生成（語彙で書けるカードのみ。raw_text 指定カードは据え置き）
python scripts/build_bga_card_specs.py --force --cards A010,A018,...
```

`build_bga_card_specs.py` の `PREREQ` 表に今回追加した述語（`occupation_count`, `house_is`,
`round_in`, `has_improvement`, `cards_in_play_at_least`, `field_count`, `pasture_count`,
`stable_count`, `resource_in_hand` など）と、`RAW_TEXT_PREREQ`（語彙で書けない12枚）が入っている。
台帳を直した場合は、この表を経由して仕様を再生成すれば同じ結果になる。

## 9. 限界と残課題

- 効果の**意味**の全文再読はしていない。トリガ・対象・数量は、仕様とテスト契約の対応、
  および #386 対象40枚と紛争カードについて文献と突き合わせた。全695枚の効果文を1枚ずつ
  読み直す作業は別Issueとする（B062 のような条件符号化のずれは、この方法でしか見つからない）。
- `missing_spec` 145枚（A119、C071–C120、D070–D120、E070–E120 の一部）は Hermes の S-1 範囲で、
  本監査では仕様・テストを作っていない。台帳の内容が文献と一致することだけ確認した。
- D025, D159 は文献ページが404のため、台帳・仕様とも「不明」のまま。値を埋めていない。
- #386 の既知残件（`small_theater` と `traveling_players` のキー統一など）は再修正していない。

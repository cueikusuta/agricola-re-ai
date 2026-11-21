# E帯仕様JSON 第1章（E099–E109）

## 目的と範囲

E帯（E001–E168）のうち、`main` に仕様JSONが無く、他担当の進行中ブランチにも含まれない
**E099–E109 の11枚**について、BGA向け第三者Wiki（`agricola.cloudfree.jp`）の個別カードページで
効果を確認し、既存バッチと同じ形式の仕様JSONとテスト契約を追加した。

- 基点: `research/freebuff-eband-spec-create`（`origin/main` = `c4a4ba6` + 棚卸し）
- 台帳（`data/bga-card-base-ledger.csv`）の費用・印刷点・種別・掲載/使用可否は全11枚で一致し、変更していない。
- 対象外: E084–E098（他担当のブランチ `m10/bga-spec-E084-098-c01` に存在。PR未作成のため重複回避で除外）、
  E110–E120（第2章）。
- 仕様の `review.status` は `bga_checked` 9枚、`needs_resolution` 2枚（E101 / E109）。
  公式（Lookout）照合済みの `verified` ではない。

## 作成した11枚

| ID | 名称 | 効果（要約） | 状態 |
|---|---|---|---|
| E099 | 無関心な親 | 各収穫の終わりに石の家なら1点 | bga_checked |
| E100 | 博物館の館長 | 各労働フェイズ開始時、木/レンガ/葦/石/小麦/野菜が1つ以上あれば1点 | bga_checked |
| E101 | はみだし者 | プレイ時、未開始ステージ1つにつき1点。以後職業を出せない | needs_resolution（Q-1） |
| E102 | 獲得者 | 各ラウンド開始時、家族数と同じ食料を払えば任意の品物1つを購入（任意） | bga_checked |
| E103 | 狼 | レンガ/木/小麦を積む。一番上の品物と同じ品物を得るたびに、それを手元へ移し猪1（任意） | bga_checked |
| E104 | スパイス商人 | ラウンド4以前にプレイで野菜3をラウンド11スペースへ。ラウンド11開始時に取得 | bga_checked |
| E105 | 開拓者 | プレイ時と、最も新しく公開されたスペースを使うたびに、建築資材1つ＋食料1 | bga_checked |
| E106 | 急ぎの売り手 | プレイ時、家族数まで建築資材を食料に交換（木/レンガ2、葦/石3） | bga_checked |
| E107 | 土地調査士 | 各収穫の畑フェイズ、畑2/4/6/7以上で食料1/2/3/4 | bga_checked（実装済み） |
| E108 | ブラックベリー農家 | 柵建設ごとに、建てた本数まで残りラウンドスペースへ食料1。各ラウンド開始時に取得 | bga_checked |
| E109 | 編みひも作り | 各収穫時に葦1→食料2（任意）。かご製作所を小進歩アクションで葦1石1で建設可 | needs_resolution（Q-2） |

## 新しい語彙

`docs/bga-card-effect-vocabulary.md` の「2026-11-21追補：E帯099–120」に追記した。
この追補はデータ契約であり、Rustエンジンでの実装済みを意味しない。

- trigger: `after_gain_good`、`feeding_phase`
- predicate: `resource_at_least_any`、`latest_revealed_space_used`、`gained_good_is_card_top`、
  `gained_food_from_this_space`、`minor_improvement_count`、`stable_count_in`
- 数量のカウント源: `remaining_stages`、`fences_built_this_event`、`own_placement_index_this_round`
- `operation: other` の kind: `forbid_occupation`、`exchange_building_resources_for_food`、
  `move_top_good_to_supply`、`harvest_grain_without_removing_field_goods`

## 人間への質問（文献で確定しない点）

### Q-1: E101「まだ始まっていないステージの数」

文献（BGA Wiki 日本語版・英語版）は「ステージ」の境界を書いていない。公式ルールの6ステージ
（4/3/2/2/2/1ラウンド）を前提に `remaining_stages` を定義したが、確認をお願いしたい。

- 典拠: https://agricola.cloudfree.jp/?p=2652 （効果文・寸評のみ）
- 選択肢: (a) 公式の6ステージ区切り（4/3/2/2/2/1）で確定、(b) 別の区切りを採用、(c) 実装しない
- 影響: 仕様の点数計算（ラウンド1で5点〜ラウンド14で0点）。実装とテストの期待値。

### Q-2: E109「各収穫時に葦1を食料2に交換できる」の回数

文献は1回あたり葦1・食料2と書くが、1収穫に何回まで交換できるかを書いていない。
仕様は「1収穫につき1回」と仮置きし、`needs_resolution` にした。

- 典拠: https://agricola.cloudfree.jp/?p=2644
- 選択肢: (a) 1収穫1回、(b) 葦がある限り繰り返し可、(c) 実装しない
- 影響: 葦の多い構成での食料量と、テストの期待値。

## 検証

- `python scripts/validate_bga_card_specs.py` → exit 0（errors 空）。
- `python -m unittest discover -s tests -p "test_bga_card_specs_builder.py"` → 3 tests OK。
  仕様JSONはビルダー管理外のスナップショットとして追加するため、再生成一致の対象外（既存のE070–E083と同じ扱い）。
- `src/`・台帳・既存仕様は変更していない。

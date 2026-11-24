# E帯仕様JSON 第2章（E110–E120）

## 目的と範囲

E帯（E001–E168）のうち `main` に仕様JSONが無い **E110–E120 の11枚**について、BGA向け第三者Wiki
（`agricola.cloudfree.jp` の個別カードページ）で効果を確認し、既存バッチと同じ形式の仕様JSONとテスト契約を追加した。
第1章（E099–E109、PR #416）とは別ブランチ・別PR。

- 基点: `research/freebuff-eband-spec-create`（`origin/main` = `c4a4ba6` + 棚卸し）
- 対象外: E084–E098（他担当ブランチで進行中）、E099–E109（第1章 PR #416）。
- 台帳の費用・前提・印刷点・種別・掲載/使用可否は全11枚で一致し、変更していない。
- `review.status` は `bga_checked` 10枚、`needs_resolution` 1枚（E120）。公式（Lookout）照合ではない。

## 作成した11枚

| ID | 名称 | 効果（要約） | 状態 |
|---|---|---|---|
| E110 | 歯医者 | 各収穫開始時に木材1を任意でカードへ。各食料供給フェイズにカード上の木材1につき食料1 | bga_checked |
| E111 | 世捨て人 | 小進歩が前に無い間、各ラウンド開始に食料1・各収穫開始に木材1 | bga_checked（実装済み） |
| E112 | 穀物泥棒 | 畑フェイズで、小麦畑ごとに小麦を残して共通ストックから小麦1（任意） | bga_checked |
| E113 | 教母 | 「家族を増やす」アクションのたびに野菜1 | bga_checked（実装済み） |
| E114 | 物置小屋作り | このカード以降に建てた1・2番目の厩で小麦1、3・4番目で野菜1 | bga_checked |
| E115 | 種の下僕 | 「小麦の種」使用でパンを焼く、「野菜の種」使用で種をまく | bga_checked |
| E116 | もみの木切り | プレイ時食料1。家畜の累積スペース使用時、そのラウンドの手番1/2/3/4/5で木材1/1/2/2/3 | bga_checked |
| E117 | パイプ喫煙家 | 各収穫開始時、小麦の植わった畑が1つ以上あれば木材2 | bga_checked（実装済み） |
| E118 | 薪切り | アクションスペースで食料を得るたびに木材1 | bga_checked（実装済み・対象が狭い） |
| E119 | 土地相続人 | ラウンド4以前のプレイで木材4・レンガ4をラウンド9へ。開始時に取得 | bga_checked |
| E120 | くず回収者 | 以後6ラウンドに木材とレンガを交互（木材から）に置き、各ラウンド開始時に取得 | needs_resolution（Q-4） |

## この章で使う新しい語彙

`docs/bga-card-effect-vocabulary.md` の「2026-11-21追補：E帯 099–120」に定義を追記済み（第1章 PR #416）。
このPRのbaseには第1章が未統合のため、以下に同章で使う語を再掲する。データ契約であり、Rust実装済みを意味しない。

| 種別 | 語 | 意味 |
|---|---|---|
| trigger | `feeding_phase` | 収穫の食料供給フェイズ（E110） |
| trigger | `after_gain_good` | 品物を得た直後（E103。本章では未使用） |
| predicate | `minor_improvement_count` | 自分の前の小さい進歩の数（E111） |
| predicate | `stable_count_in` | 厩の総数がいずれか（E114） |
| predicate | `gained_food_from_this_space` | 今回のスペース使用で食料を得た（E118） |
| カウント源 | `own_placement_index_this_round` | そのラウンドの人物配置順（E116） |
| kind | `harvest_grain_without_removing_field_goods` | 畑の小麦を残して供給から得る（E112） |
| kind | `place_on_round_spaces`（拡張） | `amounts` の交互配置と `next_6_after_current`（E120） |

## 人間への質問（文献で確定しない/実装が文献より狭い点）

### Q-3: E118「アクションスペースから食料を得るたびに」の対象範囲

効果文は対象スペースを限定せず、寸評はカードによるスペース（アーチ道・森の宿屋）でも発動するとする。
一方、現在のRust実装は日雇い・漁・小劇場・資材市場の4スペース固定で、文献より狭い。
仕様JSONは「食料を得るスペース全般」で記述し、`review.implementation_status=implemented` のまま差を注記した。

- 典拠: https://agricola.cloudfree.jp/?p=2635
- 選択肢: (a) 全スペース（カードスペース含む）へ実装を広げる、(b) 4スペース固定を正とする、(c) 現状維持で注記のみ
- 影響: 実装とテストの対象、森林宿屋・アーチ道などのカード連携。
- 注記先: `data/bga-card-specs/E118.json` の `review.unresolved_fields`（`review.status=needs_resolution`、`implementation_status=implemented` は維持）。

### Q-4: E120「これ以降の6ラウンド」で残りラウンドが6未満の場合

文献は6ラウンド分を置くとだけ書き、終盤プレイ時に余る分を捨てるのか、プレイ自体を制限するのかを書いていない。
仕様は「次の6ラウンド」とし、端数は `needs_resolution` にした。

- 典拠: https://agricola.cloudfree.jp/?p=2633
- 選択肢: (a) 存在するラウンド分だけ置く、(b) 6ラウンド残るまでプレイ不可、(c) 実装しない
- 影響: ラウンド10以降のプレイ可否と、獲得量（6資材か、残りラウンド数か）。

## 検証

- `python scripts/validate_bga_card_specs.py` → exit 0（errors 空）。
- `python -m unittest discover -s tests -p "test_bga_card_specs_builder.py"` → 3 tests OK。
  追加分はビルダー管理外のスナップショット（E070–E083 と同じ扱い）で、再生成一致の対象外。
- `src/`・台帳・既存仕様は未変更。

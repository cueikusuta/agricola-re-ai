# BGAカード効果語彙（Issue #202）

`data/bga-card-specs/<ID>.json` の `effects[]` が使う語彙の一覧。`schemas/bga-card-spec.schema.json` は
`trigger` / `predicate` を自由文字列で受けるため、**綴りの揺れを防ぐ正本はこの文書**とする。新しい語を
追加したらこの表に追記する。語彙はBGA向けWikiの第三者の説明文を、実装AIが機械処理できる粒度に
言い換えたものであり、Lookout公式の原文ではない。

## 発動イベント（`effects[].trigger`）

| trigger | 意味 | 発動主体 |
|---|---|---|
| `on_play` | カードを使用したとき（その解決中） | 所有者 |
| `passive` | 常時有効。イベントではなく状態・権限の付与 | 所有者 |
| `round_start` | ラウンド開始時 | 所有者 |
| `round_end` | ラウンド終了時（帰宅フェイズを含む） | 所有者 |
| `harvest_field_phase` | 収穫の畑フェイズ | 所有者 |
| `after_harvest` | 収穫の解決後 | 所有者 |
| `after_bake_bread` | 「パンを焼く」アクション解決後 | 所有者 |
| `after_worker_space` | いずれかのプレイヤーが特定のアクションスペースを使った後 | `conditions` の `any_player` / `other_player_only` で限定 |
| `after_improvement` | 大小進歩を建設した後（このカード自身を含む場合は `include_this_card`） | 所有者 |
| `after_improvement_declined` | 進歩アクションを得たが使わないことを選んだとき | 所有者 |
| `after_renovation` | 改築後 | 所有者 |
| `after_upgrade_fireplace` | かまどを調理場へ改良した後 | 所有者 |
| `after_fence_build` | 柵の建設後 | 所有者 |
| `after_sow` | 「種をまく」アクション後 | 所有者 |
| `after_occupation` | 職業カードを出した後 | 所有者 |
| `after_animal_acquisition` | 家畜を取得した後 | 所有者 |
| `when_buying` | 進歩・部屋・改築の支払い時（割引の適用） | 所有者 |
| `end_scoring` | 最終得点計算時 | 所有者（他プレイヤーに及ぶ場合は `target`） |
| `after_animal_accommodation` | 累積スペースから得た家畜を農場に収容した後 | 所有者 |
| `after_work_phase` | 労働フェイズ終了時 | 所有者 |
| `after_renovation` | 改築後（`house_is` + `negate` で「木の家でなくなった」を表す） | 所有者 |

## 原子操作（`effects[].steps[].operation`）

`schemas/bga-card-spec.schema.json` の `$defs.step.operation` のenumに一致させる。

| operation | params の要点 |
|---|---|
| `gain_resource` | `good`, `amount`（式可）, `target`（他者へ渡す場合は `all_other_players`） |
| `spend_resource` | `good`, `amount`, `otherwise`（払えない場合の代替。例 `begging_marker`） |
| `gain_points` | `points`（式可） |
| `plow_field` | `amount`, `adjacent`（省略可） |
| `build_stable` | `amount`, `wood_cost`（0で無料） |
| `build_room` | `amount`, `house` |
| `renovate` | `free_building_resources` |
| `move_card` | `from`, `to` |
| `modify_capacity` | `scope`, `animal`, `animal_capacity`, `any_type`, `ignore_stables` |
| `discount_cost` | `good`, `amount`, `scope`, `segment_cap` |
| `grant_action` | `action`, `must_pay_cost`, `without_room` など |
| `exchange` | `give`, `receive` |
| `other` | `kind` で下記の特殊操作を識別 |

### `operation: other` の `kind`

| kind | 用途 |
|---|---|
| `choice` | `options` から1つを選ぶ。選択者は `decision_owner` |
| `substitute_cost` | `material_from` / `material_to` / `scope`。支払い材料の置換権限 |
| `ignore_occupation` | `space`, `occupied_by_others`, `except_space`。占有されていても使える権限 |
| `forbid_renovation` | 以後の改築を禁止する継続効果 |
| `build_improvement` | `options`（カード名配列）, `cost`。指定コストで大進歩を建設 |
| `fence_pasture` | `spaces`, `wood_cost`, `must_be_adjacent`。柵で牧場を作る |
| `rebuild_fences` | `add_up_to`, `keep_animals`。全柵を撤去して建て直す |
| `place_on_round_spaces` | `good`, `amount`, `rounds`。以後のラウンドスペースに品物を置く |
| `count_as_field` | `amount`。カード自体を畑として数える |
| `gain_each_with_at_least` | `goods`, `threshold`, `amount`。所持数が閾値以上の品物を各1つ得る |
| `add_good_to_single_good_fields` | `amount`, `same_type`。品物がちょうど1つの畑へ同種を追加 |

## 数量式（`amount` / `points` に入る式木）

| op | args | 意味 |
|---|---|---|
| `count` | `[source]` | 下記カウント源の個数 |
| `per` | `[source, n]` | カウント源1つにつき `n` |
| `resource_count` | `[good]` | ストックの資源数 |
| `floor_div` | `[expr, n]` | 切り捨て除算 |
| `table` | `[key, {map}]` | キーの値で表を引く（`0-4` のような範囲キーも可） |
| `if_else` | `[cond_expr, a, b]` | 条件式で分岐 |
| `played_on_space` | `[space]` | そのカードを出したアクションスペース |
| `player_count_table` | `[{map}]` | 人数別の値 |
| `custom` | `["threshold_table", key, {threshold, amount}...]` | 閾値表。所持数が閾値以上なら対応する値（`A057` など） |

### カウント源（`count` / `per` の `source`）

`grain_field` `vegetable_field` `field` `empty_field` `unplanted_field` `pasture` `pasture_at_max_capacity`
`stable` `room` `clay_room` `person` `occupation` `improvement` `major_improvement_bottom_row`
`person_on_accumulation_space` `hand_occupation_count` `remaining_rounds` `fence`
`vegetable_taken_from_field` `sheep` `wild_boar` `cattle` `completed_harvest`
`grain_in_supply` `unused_farmyard_space`

## 条件（`effects[].conditions[]`）

| predicate | params | 意味 |
|---|---|---|
| `worker_space_used` | `space` または `spaces` | 対象スペースが使われた |
| `any_player` | — | 発動主体を全プレイヤーに広げる |
| `other_player_only` | — | 所有者以外のプレイヤーに限定 |
| `house_is` | `house`（`wood` / `clay` / `stone` または配列） | 家の材質 |
| `room_count` | `n` | 部屋数がちょうど `n` |
| `room_count_in` | `values` | 部屋数がいずれか |
| `rooms_exceed_people` | — | 部屋数 > 人物数 |
| `occupation_count` | `n` / `max` | 出している職業数 |
| `improvement_count` | `n` | 出している進歩数 |
| `grain_field_count` | `n` | 小麦畑の数 |
| `field_count` | `n` | 畑タイル数 |
| `empty_field_count` | `n` | 何も植えていない畑の数 |
| `pasture_count` | `n` | 牧場の数 |
| `stable_count` | `n` | 厩の数 |
| `farmyard_all_used` | — | 農場スペースが全て使用済み |
| `all_animal_types` | — | 全種類の家畜を飼っている |
| `round_has_no_harvest` | — | 収穫のないラウンド |
| `round_number` | `n` / `min` | ラウンド番号 |
| `fence_count_at_least_round` | — | 建設本数がラウンド数以上 |
| `harvests_equal_occupations` | — | 終了した収穫数と出した職業数が同数 |
| `animal_count_at_least` | `animal`, `n` | 家畜数が `n` 以上 |
| `declined_improvement` | `kind`（`minor` / `major`） | 進歩アクションを使わなかった |
| `sow_unconditional` | — | 制約のない「種をまく」アクション |
| `raw_text` | `text` | 上記で表現できない前提条件。原文の要約を保持し、`review.status` を `needs_resolution` にする |
| `has_improvement` | `kind` | 指定の進歩を所有している |
| `all_accumulated_animals_accommodated` | — | 累積スペースから得た家畜を全て収容できた |
| `round_space_has_placed_good` | `good` | 対象ラウンドスペースに置かれた品物が残っている |
| `resource_greater_than` | `good`, `than` | ストックの `good` が `than` より多い |
| `unused_farmyard_spaces_at_least` | `n` | 未使用の農場スペースが `n` 以上 |
| `animal_count_less` | `animal`, `n` | 家畜数が `n` 未満 |

## 発動頻度・持続・状態参照

- `frequency`: `once_per_play` / `every_use` / `once_per_round` / `once_per_game` / `end_of_game` / `always`
- `duration`: `instant` / `permanent` / `per_round` / `until_next_harvest`
- `snapshot`: `before_event` / `before_effect` / `live_state` / `after_event`
  （数量計算がどの時点の状態を見るか。二重付与の防止に必須）
- `decision_owner`: `none` / `owner` / `affected_player` / `random` / `other`

## 運用ルール

1. 1カードの効果が独立した複数の処理に分かれる場合は `effects[]` を複数要素にし、`order` で順序を明示する。
2. 任意発動は `optional: true` とし、`decision_owner` を必ず埋める。
3. 他プレイヤーに影響する `step` は `target` を `all_other_players` などにし、所有者以外へ勝手に加算しない。
4. 数値が式で決まる場合、`snapshot` を `before_effect` にして「効果適用前」を参照することを明示する。
5. 語彙で表現できない効果は `effects: null` のまま `review.status` を `needs_resolution` とし、`unresolved_fields` に理由を書く。無効果のダミーにしない。

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
| `before_bake_bread` | 「パンを焼く」アクションの直前 | 所有者 |
| `after_room_construction` | 部屋が建設された後（他プレイヤーを含む場合は `any_player`） | 所有者 |
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
| `before_worker_space` | 特定のアクションスペースを使う直前（割引・追加建設） | 所有者 |
| `before_sow` | 「種をまく」アクションの直前 | 所有者 |
| `work_phase_start` | 各ラウンドの労働フェイズ開始時 | 所有者 |
| `harvest_start` | 各収穫の開始時（畑フェイズより前） | 所有者 |
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
| `place_on_round_spaces` | `good`, `amount` または `amounts`, `rounds`, `marker_kind`。以後のラウンドスペースに品物・しるしを置く。`rounds` は整数 / `"remaining"` / `"remaining_even"` / `"remaining_odd"` / 閾値表 |
| `stack_goods_on_card` | `goods`（下から順）。カード上に品物を積む |
| `take_top_good_from_card` | カード上の一番上の品物を得る |
| `gain_matching_improvement` | `pairs`。対応する大進歩を所有していれば品物を得る |
| `capacity(scope="card", per_type=True)` | カード上に種別ごとに家畜を収容 |
| `count_as_field` | `amount`。カード自体を畑として数える |
| `gain_each_with_at_least` | `goods`, `threshold`, `amount`。所持数が閾値以上の品物を各1つ得る |
| `add_good_to_single_good_fields` | `amount`, `same_type`。品物がちょうど1つの畑へ同種を追加 |
| `forbid_house_animals` | 家に家畜を収容できなくする継続効果 |
| `allow_major_via_minor` | `improvements`（配列または `"any"`）。「小さい進歩」アクションでも大進歩を建設できる |
| `discard_field_grain` | `wood_per_grain`。1つの畑の小麦を捨てて木材に交換 |
| `take_from_accumulations` | `count`, `distinct_types`。累積スペースから建築資材を取る |
| `free_outer_edge_fences` | 農場外周の柵を無料にする継続効果 |
| `place_food_on_card` | `any_amount`。食料をカード上に置く |
| `run_field_phase` | `only_owner`。畑フェイズを即座に実行 |
| `place_reed_for_point` | `max`。葦を置いて得点（上限あり） |
| `disable_one_stable_pasture` | 厩つき牧場1つを使えなくする |
| `require_empty_pasture` | `count`。空の牧場を維持する制約 |
| `place_up_to_two_wood_on_card` | カード上に木材を置く |
| `use_two_spaces_one_person` | `spaces`, `order`, `requires_both_unoccupied`。隣接する2つのスペースが両方未占有なら、1人の人物で順に両方を使用できる。C023 で使用。|

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
`grain_in_supply` `unused_farmyard_space` `sown_field` `stone_in_supply`
`column_with_room` `wood_on_card`

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
| `improvements_equal_occupations` | — | 前にある進歩数（基礎進歩＋プレイ済み小進歩）と出した職業数が同数。配置イベント後の状態で判定する（B049 で使用） |
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
| `round_in` | `values` | 現在のラウンドが `values` のいずれか |
| `has_empty_room` | — | 家に空き部屋がある |
| `stone_rooms_exceed_people` | — | 石の部屋数 > 人物数 |
| `goods_in_fields_before_final_harvest_at_least` | `n` | 最後の収穫前に畑上にあった品物が `n` 以上 |
| `accumulation_at_least` | `space`, `n` | 対象の累積スペースに品物が `n` 以上ある |
| `cards_in_play_at_least` | `n` | 自分の前のカード（職業＋進歩）が `n` 枚以上 |
| `fences_built_at_least` | `n` | 建設済みの柵が `n` 本以上 |
| `worker_on_space` | `space` | 自分の人物がそのスペースに置かれている |
| `grain_harvested_at_least` | `n` | 畑フェイズで収穫した小麦が `n` 以上 |
| `wood_rooms_built_at_least` | `n` | 木の部屋が `n` 部屋以上建設された |
| `pasture_size` | `n` | 指定サイズの牧場がある（`A001` はちょうど1） |
| `fenced_stable_count` | `n` | 柵内の厩が `n` 以上 |
| `animal_count_at_most` | `animal`, `n` | 家畜数が `n` 以下 |
| `round_space_in` | `rounds` | 対象ラウンド番号の範囲内（例 A151 の 1-4 ラウンド） |
| `round_space_unused` | — | そのラウンドのラウンドスペースが未使用 |
| `lessons_used_by_fewer_players` | — | 「授業」スペースを使ったプレイヤー数が他のスペースより少ない |
| `has_stone_house` | — | 所有者が石の家に住んでいる |
| `goods_on_upcoming_round_spaces` | — | 開始前のラウンドスペースのいずれかに所有者が受け取る品物がある |

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

## 2026-11-09追補：Issue #226 A070〜A090の追加語彙

本表はBGA向け第三者Wikiから起こした**データ契約**であり、Rustの効果実装済みを意味しない。
通常アクションの成果をすべて解決した後で `after_worker_space` を通知する。
ただし「そのスペースを使ったときの累積数」を条件とするカードは、使用前のイベント値を保持し、`snapshot=after_event` の現在の空スペースを参照してはいけない。

### 新しいtrigger
| trigger | params/タイミング |
|---|---|
| `after_stables_built` | 自分の手番で厩を建て終えたイベント単位、`stables_built_on_own_turn` と併用 |
| `returning_home_phase` | 収穫のないラウンドも含め、人物を帰宅させるフェイズ |
| `after_worker_space` | 使用した通常アクションが完了した後。直前のスペースが集積スペースなら**通常取得済み** |

### 新しいpredicate
| predicate | params・期待値 |
|---|---|
| `farmyard_unused_became_used` | `n`：当該イベントにより未使用→使用済みになった農場マス数 |
| `new_pasture_created` | `min_spaces`：その柵建設で新規成立した牧場の最小セル数 |
| `resource_at_least` | `good,n`：支払前の所有資源の下限 |
| `stables_built_on_own_turn` | `min`：自分の手番で新規建設した厩数 |
| `vegetables_planted` | `min`：その種まきイベントで野菜を植えた畑の個数 |
| `round_space_has_marker` | `kind`：指定ラウンド上のマーカー識別（使用時に原子的に消去） |
| `accumulation_at_least` | `space=any_building_resource_space,n`：**通常アクション解決後**も指定資材スペースがn以上保持している |

### 新しい `operation:other` のkind・既存kind拡張
| kind | params・意味 |
|---|---|
| `move_field_good_to_supply` | `good,good_amount,max_source_fields,counts_as_field_phase`。指定畑から一般ストックへ戻す（収穫ではない） |
| `move_crop_to_empty_field` | `crop_amount,source_field_min_crops,counts_as_sow,counts_as_harvest`。**A071はフリーアクションの受付窓・保存再演が未規定のため`needs_resolution`かつ`effects=null`** |
| `add_good_to_single_crop_type_fields` | `same_type,amount_per_field,scope`。同種作物だけの畑へ追加 |
| `stack_goods_on_card` | `mapping,amount_per_use` を追加。アクションスペースと積載資源の対応 |
| `move_all_goods_on_card_to_supply` | カード上の全資源を一度に所有者へ移す |
| `take_good_from_accumulation` | `good_type=building_resource,amount`。通常取得後に残り4以上のスペース**1か所**から任意に1個取得。空の使用済み累積スペースから二重取得しない |
| `place_animals_on_fenced_pasture` | `animal,amount,destination=newly_fenced_pasture`。新規牧場の収容可能性も検証 |
| `breed_animal` | `animal_types`。支払後に指定家畜種を繁殖。収容/調理/廃棄は既存エンジンの合法性で判断 |
| `allow_wood_to_stone_renovation` | `cost_unchanged`。石の家への改築経路を拡張する権限。通常の資源費用を省略しない |

### 参照時点・保留
- `A082` の閾値4以上は通常アクション解決**後**。同じ累積スペースの通常取得品を2回数えない。`A082-normal-action-drains-space` の回帰テストを参照。
- `A071` の「いつでも」は `passive` だけでは人物を置かない合法な操作として確定できない。フリーアクションAPIで手番境界、連続実行、取消、再演ハッシュを定義するまで実装禁止。
- `A089` のラウンド14を超えるマーカーは既存の`needs_resolution`のままとし、未知のラウンド15以降を生成しない。

## 2026-11-09追補：Issue #226 A091〜A114の追加語彙

この節はカード仕様JSONの新規語彙を定義する。**本節の存在はRustエンジンでの実装完了を意味しない**。各カードの `review.status=needs_resolution` は、他の記述に優先する実装停止ゲートである。

### 発動イベント
| trigger | 意味・参照時点 |
|---|---|
| `after_family_growth` | 家族増員後。A092は同ラウンドの別人物行動・食料支払い窓が未確定なので実行ゲート停止 |
| `sow_action_declined` | 制約なし種まき権限を辞退したイベント。A094は辞退できる権限源/タイミング未確定 |
| `stone_accumulation_space_revealed` | ラウンド準備中に石の累積スペースを新規公開 |
| `after_field_gained` | 同一イベントで獲得・耕作した畑が確定した時点 |

### 述語
| predicate | パラメータ・評価 |
|---|---|
| `accumulation_at_most` | `space,n`（`good`だけではスペースを同定できない）。**使用前**の累積個数で評価する。A095 `Fishing` は2以下 |
| `accumulation_exactly` | `n`。**使用前**の指定した資材累積スペースの個数を用いる（A107） |
| `own_placement_index_this_round` | `index`。自分の1/2/3番目の人物かを行動開始時に記録 |
| `persons_returned_from_accumulation_at_least` | 累積スペースで働いた人物の帰宅数閾値 |
| `improvement_played_from_hand` | `via_major_or_minor_action`。自身の手札から該当進歩を使用したイベント |
| `round_space_has_placed_good` | `good`：カード/所有者ごとに分離されたラウンドマーカー集合の有無。単純なboolを取得数量と混同しない |

### 特殊操作kind・数量
| kind/式 | params・安全な意味 |
|---|---|
| `allow_family_member_to_act` | 増員人物ID、ラウンド、合法な配置窓、食料支払・新生児状態を一貫して記録（A092は未解決のため実行不可） |
| `place_unplaced_person` | `amount,destination,allow_occupied`。既存手番キューに対し未配置人物を一度だけ合法配置 |
| `gain_good_by_food_tier` | `tiers,amount,mode`。A103の閾値対応が未確定のため`effects=null`で実行不可 |
| `add_good_to_matching_fields` | `field_match,placement`で対象畑と既存品物の下位置を指定 |
| `take_all_goods_from_round_space` | `good,source=this_card_round_markers,consume_markers=true,amount=all`。同一ラウンドに重なったマーカーを**カード所有者ごと**に数えて全量取得・原子的に除去（A111） |
| `custom(["count",source])` | source: `pastures_at_least_3_spaces`, `cooking_improvements`, `wood_accumulation_spaces_with_exactly_2`, `wood_accumulation_spaces_with_at_least_3`, `fields_gained_this_event`, `fields_harvested_last_grain`, `fields_harvested_last_vegetable` |
| `grant_action` | action: `minor_improvement` / `major_or_minor_improvement`。手札/市場を合法なものに限定 |

### 時点・保存の契約
- `A095` および `A107`: `trigger=after_worker_space` の効果でも `snapshot=before_event` とし、**通常アクションで取り除く前の累積数**を判定。発動時の資源/食料は通常アクション完了後に増減。未公開・未来情報を参照しない。
- `A111`: `after_room_construction` で各イベント1回だけ置く。複数イベントが同じラウンドに重なれば各マーカーを別々に記録し、`round_start` で**自分のカードが置いた全個数**を得る。他カード由来マーカーは保持する。
- `A092` と `A103`: 不明点を推測実装しない。両者とも`review.status=needs_resolution`かつ`effects=null`でフェイルクローズ。定義された語彙は将来の設計候補である。
## 2026-11-16追補：C帯 001-069 第1弾の追加語彙（Issue #225 / PR #323）

### 新しいtrigger
| trigger | 意味・参照時点 |
|---|---|
| `after_stable_built` | 自分の手番で厩を建設した直後。`stables_built_on_own_turn` と併用可。C056 で使用。 |

### 新しいpredicate
| predicate | パラメータ・評価 |
|---|---|
| `field_vegetable_stacked` | `min_stack`：畑に指定個数以上の野菜が重なっている（最上段が対象）。C057 で使用。 |
| `cultivated_fields_at_least` | `n`：栽培中の畑が n 個以上。C018 で使用。 |

### 新しい `operation:other` のkind
| kind | params・意味 |
|---|---|
| `discard_field_goods_plow_one` | `requires_cultivated_fields, fields_to_clear, plow_amount`。指定畑の全品物を捨てて畑1を耕す。C018 で使用。 |
| `buy_animal` | `animal[], food_cost{animal:amount}, requires_capacity`。家畜を食料で購入する権限。収容可能性を必須条件とする。C009 で使用。 |
| `place_unplaced_person` | `amount, destination, immediately`。未配置の人物を即座に合法なスペースへ配置。C003 で使用。 |
| `build_stable_alternate_cost` | `pay_good, pay_amount, once_per_build`。厩建設時に指定資源で代替支払い（1回の建設アクションで1棟まで）。C056 で使用。 |

### 未解決・保留
- `A071` と `C018` と `C057` の「いつでも（anytime）」トリガーは語彙未定義のため `needs_resolution` のまま。フリーアクションAPI（手番境界・連続実行・取消・再演ハッシュ）が定義されるまで実装禁止。
- `C020` のラウンド依存プレイ制限（ラウンド9以降）、`C054` の厩返却コスト、柵支払い、after_field_phase トリガー、`C056` の last-stable 判定と per-build 代替支払い、`C003` の未配置人物前提、`C009` の購入時間窓は語彙外のため `needs_resolution` 保持。推測で確定しない。

## 2026-11-22追補：ラウンド開始時獲得カード7枚（A090/A151/A152/B097/B106/B135/C064）

本節はカード仕様JSONの新規語彙を定義する。**本節の存在はRustエンジンでの効果適用完了を意味しない**。エンジン側の状態は各カードの spec JSON `review.notes` と実装PRの記述を正本とし、以下は要約である。

### 新しいpredicate
| predicate | パラメータ・評価 |
|---|---|
| `round_space_in` | `rounds`：対象がそのラウンド番号の範囲内。A151 で使用。 |
| `round_space_unused` | —：そのラウンドのラウンドスペースが未使用。A151 で使用。 |
| `lessons_used_by_fewer_players` | —：「授業」スペースを使ったプレイヤー数が他のいずれのスペースより少ない。A152 で使用。 |
| `has_stone_house` | —：所有者が石の家に住んでいる。B097 で使用（A090 は既存の `house_is` を使用）。 |
| `goods_on_upcoming_round_spaces` | —：開始前のラウンドスペースのいずれかに所有者が受け取る品物がある。B106 で使用。 |

### エンジン実装状態（Rust）
- 語彙マッピング（`CardEffect::StartRound*`）と `round_start` トリガーの dispatch 配線は7枚分完了。`verified_round_start_cards()` から既定レジストリへ登録済み。
- **B106** は条件充足時に無条件で獲得できる唯一のカードとして実適用。エンジンの `accumulations` はベースアクションスペースも含むため、revealed ラウンドカードのスペースのみを検査する。解釈注記（第三者のWiki記載を基にするが「開始前」の読みに替代案あり）は実装PRに記録し、人間確認を待つ。
- **A090 / A151 / A152 / B097 / B135 / C064** は dormant（発火しない）。round start で生成する PendingDecision には解決窓がなく（`GameState::start_round` は dispatch の Transition を破棄し、次ラウンド境界で pending をエラーにする）、複数所有者が共有 pending を上書きし合う。プレイヤー決定チャネル（耕す畑の位置・手札カードの選択・与える家畜・grain 消費の可否）を導入するまで適用しない。

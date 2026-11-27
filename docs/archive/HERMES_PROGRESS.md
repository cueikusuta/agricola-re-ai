# HERMES_PROGRESS.md - Issue #225: A/C/D/E 001-069 needs_resolution 解消

## 作業開始: 2026-11-16

### 対象カード一覧 (needs_resolution / inventory_only / partial)

#### A帯 (001-069)
- [ ] A004: 幅木 - needs_resolution (cost不明)
- [ ] A005: 粘土積み場 - partial (複数フィールド未解決) - PR #320で解決済みとのこと
- [ ] A009: 子供の動物市場 - needs_resolution (cost不明)
- [ ] A020: 畝立て鋤 - needs_resolution (ラウンド依存コスト)
- [ ] A060: 東洋かまど - needs_resolution (複雑な効果) - PR #320で継続中
- [ ] A064: 大麦製粉機 - needs_resolution (cost不明)

#### C帯 (001-069) - 17 needs_resolution + 20 inventory_only = 37枚
needs_resolution:
- [ ] C003: 馬車旅行 - prerequisites (未配置人物)
- [ ] C009: 自動水受け皿 - purchase_window (購入タイミング)
- [ ] C018: 回転鋤 - timing (いつでも)
- [ ] C020: もぐら鋤 - prerequisites (ラウンド9以降)
- [x] C023: 雇用契約 - prerequisites (職業未プレイ) → **bga_checked**
- [x] C029: ビアテーブル - prerequisites (小麦ストックなし) → **bga_checked**
- [ ] C051: 漁網 - tax/refill (他プレイヤーへの税) → **needs_resolution** (語彙外: tax_space_use・帰宅フェイズ補充)
- [x] C052: 猟師の帽子 - prerequisites/effects granularity → **bga_checked** (※モデルは pig_market に限定)
- [ ] C053: 三つ足やかん - effects granularity (調理改良ごと) → **needs_resolution** (語彙外: after_cooking_conversion・cooking_conversion_goods)
- [ ] C054: 露店 - cost/timing/fence_to_wood
- [ ] C056: 給餌柵 - last_stable_detection/alternate_cost
- [ ] C057: 生野菜 - timing (いつでも)
- [ ] C058: 木彫 - stock_condition (木材5以下)
- [ ] C060: 陶工の小窯 - prerequisites/timing
- [ ] C062: 調理場の拡張 - scope (調理改良ごと)
- [ ] C065: 穀物倉庫 - cost/rounds
- [ ] C069: 区画整理 - timing (いつでも)

inventory_only (20枚):
- [ ] C031: 書斎
- [ ] C032: 張出便所
- [ ] C033: 緑化計画
- [ ] C034: ナピアグラス培養機
- [ ] C035: ランタンの家
- [ ] C036: 粘土地帯
- [ ] C037: 人工丘の集落
- [ ] C038: キリスト教
- [ ] C039: アトリエボート
- [ ] C040: ズタ袋
- [ ] C041: 農産店
- [ ] C042: 激しい空腹
- [ ] C043: 農舎
- [ ] C044: 鶏小屋
- [ ] C045: シチュー
- [ ] C046: マンドリン
- [ ] C047: 園芸用熊手
- [ ] C048: 農園
- [ ] C049: ビール屋台
- [ ] C050: 厩の庭

#### D帯 (001-069) - 37 needs_resolution
- [ ] D001: ジグザグ鍬 - field geometry
- [ ] D008: シダの胞子 - immediate sow / field prereq
- [ ] D013: こて - stone renovation cost
- [ ] D014: ハンマー破砕機 - timing / extra action
- [ ] D018: 蒸気鋤 - after return-home / plow without person
- [ ] D020: 折り返し鋤 - card-as-field / extra plowing
- [ ] D022: 労働許可証 - round+material arithmetic
- [ ] D023: 開拓者の魂 - owner-only action space
- [ ] D024: 兄弟愛 - exact 4 people / simultaneous placement
- [ ] D025: 魔女の踊り場 - multi_type / multi-role
- [ ] D027: 再教育 - facility exchange
- [ ] D029: 畜糞用くまで - unfenced stable conditions
- [ ] D031: 貯蔵部屋 - pair counting / half-point rounding
- [ ] D033: 避暑地の別荘 - stone house / adjacency
- [ ] D036: 品種登録 - sheep tracking / no-sheep-eaten
- [ ] D037: 彫像 - round/space comparison
- [ ] D039: トリュフスライサー - effect incomplete
- [ ] D040: 汚水溜め - round space placement
- [ ] D041: 馬力船 - pre-round spaces
- [ ] D042: 教育手当 - 6th occupation
- [ ] D044: 森の井戸 - round selection
- [ ] D045: 羊の井戸 - future rounds
- [ ] D046: ペレット圧縮機 - reed payment / timing
- [ ] D048: 市民のはりぼて - hand comparison
- [ ] D049: 本棚 - max times / trigger scope
- [ ] D050: 対外援助 - rounds 12-14 restriction
- [ ] D051: アーチ道 - action space effect
- [ ] D053: 茶屋 - skipped person / timing
- [ ] D055: 新しい市場 - round 8-11 spaces
- [ ] D056: 屠畜場 - improvements / conversions
- [ ] D058: 種子散布機 - Sow action timing
- [ ] D059: 掘り土窯 - conversion frequency / bake bread
- [ ] D060: 大製陶所 - scoring thresholds / clay conversion
- [ ] D064: 焼き菓子教室 - harvest-less rounds
- [ ] D067: 刈り取りフック - 3 round selection
- [ ] D068: 小さなかご - 4+ player / reed payment
- [ ] D069: 小温室 - round calculation / vegetable placement

#### E帯 (001-069) - 26 needs_resolution
- [ ] E002: 改築資材 - cost (alternative only)
- [ ] E021: 羊皮の絨毯 - cost (alternative only)
- [ ] E022: ゲストルーム - timing (any time)
- [ ] E027: 貯金箱 - food source (supply vs general)
- [ ] E030: 子供のおもちゃ - choice cost
- [ ] E037: 牛の頭骨 - on_play_food discrepancy
- [ ] E047: 樹液採取器 - cost (alternative only)
- [ ] E048: 町役場 - cost (alternative only)
- [ ] E052: 小部屋 - feeding phase gain / persistent state
- [ ] E053: ボアスピア - animal_is / not_harvest_breeding
- [ ] E054: 密輸品 - extra material payment / improvement_played_from_hand
- [ ] E055: 石の堰 - trigger / custom operation
- [ ] E056: ローマ土器 - turn order / card-local storage
- [ ] E057: チーズフォンデュ - grain_converted_at_least
- [ ] E058: 昼どきのビール - skip phases
- [ ] E059: 刃付き櫛 - on_use_day_laborer / sheep_on_sheep_market
- [ ] E060: 軍手 - pay_occupation_cost / discount_cost
- [ ] E061: 揚げ床 - cost discrepancy (brick vs clay)
- [ ] E062: 発酵生地 - turn structure change / all_other_players
- [ ] E063: 鉄窯 - before_bake_bread / permanent modification
- [ ] E064: 簡易窯 - before_bake_bread / cost discrepancy
- [ ] E065: お布施袋 - round_number / 0 occupations
- [ ] E066: 穀物小屋 - other_player_action / cumulative spaces
- [ ] E067: 穀物袋 - on_use_grain_utilization / bake_bread_count
- [ ] E068: サクランボ果樹園 - virtual field / wood harvest
- [ ] E069: スイカ畑 - virtual field / vegetable harvest / plow field

## 進捗記録

### 2026-11-16 開始
- ワークスペース確認完了
- 対象カード全件のステータス把握完了
- HERMES_PROGRESS.md 作成

### 完了カード (実態反映版: Round 2 修正後)
- [x] C023: 雇用契約 - needs_resolution → bga_checked (前提条件: 職業未プレイ確認済み)
- [x] C029: ビアテーブル - needs_resolution → bga_checked (前提条件: ストック小麦なし確認済み)
- [x] C052: 猟師の帽子 - needs_resolution → bga_checked (前提条件: 鍋の進歩、効果: 猪ごと確認済み)
- [ ] A004/A009/A020/A064: needs_resolution 維持 (コスト不明・語彙外)
- [ ] C003/C009/C018/C020/C054/C056/C057: needs_resolution 維持 (語彙外・未確定)
- [ ] C051/C053: **Round 2 で needs_resolution に戻し** (語彙外項目を unresolved_fields に明記)

### 次のアクション
C帯の needs_resolution カードを順次解消 (C051, C053, C058, C060, C062, C065, C069)
D帯・E帯の needs_resolution カードへ着手
inventory_only カード (C031-C050, 20枚) の効果定義

---
*注: このファイルはプロセス記録用です。リポジトリ直下へのコミットは非推奨 (round 1 P3 と同種のノイズ)。正確な状態は `python scripts/validate_bga_card_specs.py` の出力を参照のこと。*
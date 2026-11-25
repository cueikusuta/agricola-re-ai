# 作業の現在地 — Freebuff on_play 獲得カード実装

## 依頼と範囲

- リポジトリ: `cueikusuta/agricola-re-ai`。ブランチ `feat/freebuff-onplay-gain-impl`、
  基点は `origin/main` = `7b68eb7`（2026-11-25 取得）。
- 対象: `data/bga-card-specs/` の `effects` が `on_play` の獲得ステップ中心のカード。
  効果のパラメータ（good / amount / cost / printed_points / prerequisites）は仕様JSONを正とする。
- 着手前に `git log origin/main` と open PR 全件（`gh pr list --state open`）を確認し、
  main に実装済みのカードと、他担当が open PR で追加済みのカードは対象から外した。
- 終了時得点（`end_scoring` / `end_game_scoring` / `EndGameBonus`）系は他担当の範囲なので触らない。
- `on_play` 自体に獲得以外の操作（アクション付与、無料改築、大進歩建設、先のラウンドスペースへの配置、
  動的な品物選択）を含むカードは対象外とし、下の一覧に残す。
- 獲得以降の別トリガー（harvest / round_end / after_worker_space など）は各トリガーの章の follow-up とし、
  今回は `on_play` の獲得部分だけを実装する（PR #454 と同じ既存慣例）。
  そのため対象カードの仕様 `implementation_status` は `in_progress` とする。
- 動物（sheep / wild_boar / cattle）だけを獲得するカードは「資材や食料」ではないため対象外。
- `review.status = needs_resolution` は `docs/bga-card-effect-vocabulary.md` の実装停止ゲートなので、
  推測実装せず、末尾の「人間への質問チェックリスト」に残す。

## 章割りと現在地

| 章 | 帯 | 対象 | 状態 |
|---|---|---|---|
| 第1章 | A | A054, A165 | 完了・commit/push 済み |
| 第2章 | B | B025, B103, B116, B155 | 未着手 |
| 第3章 | C | C026, C086 | 未着手 |
| 第4章 | D | D062, D117 | 未着手 |
| 第5章 | E | E082 | 未着手 |
| PR | — | 本文に実装一覧・除外理由・検証結果 | 未作成 |

各章で `cargo fmt` → 対象テスト → commit → push を行う。マージはしない。

## 実装カード（on_play の獲得部分）

| ID | カード名 | 種別 | コスト | 印刷点 | on_play の獲得 | 今回実装しない残り |
|---|---|---|---|---|---|---|
| A054 | 信用貸し | 小進歩 | なし（職業3以下） | 0 | food 5 | 収穫のないラウンド終了時に food 1 支払い or 乞食札 |
| A165 | 猪飼い | 職業 | なし（4人戦のみ） | 0 | food 1 | ラウンド12終了時の無料繁殖 |
| B025 | パン焼き棒 | 小進歩 | wood 1 | 0 | food 1 | 職業を出した後の追加パン焼きアクション |
| B103 | 畑の取引人 | 職業 | なし | 0 | wood 1 + reed 1 | 進歩アクションを辞退したときの food/vegetable |
| B116 | 岸辺の林業者 | 職業 | なし | 0 | wood 1 | 空の葦の集積スペースに葦が置かれたとき wood 1 |
| B155 | 美術教師 | 職業 | なし（4人戦のみ） | 0 | wood 1 + reed 1 | 職業コストを小劇場の food で支払う権利 |
| C026 | 脱穀棒 | 小進歩 | wood 1 | 0 | food 2 | Farmland / Cultivation 後の追加パン焼きアクション |
| C086 | 家畜の餌係 | 職業 | なし | 0 | grain 1 | ストックの grain 1につき家畜1頭を収容できるパッシブ |
| D062 | ビールタップ | 小進歩 | wood 1 | 0 | food 2 | 各食料フェイズの grain 2/3/4 → food 3/6/9 交換 |
| D117 | 木の専門家 | 職業 | なし | 0 | wood 2 | 購入時の木材割引（when_buying） |
| E082 | 不当利得 | 小進歩 | なし | 0 | food 1 | 日雇い使用後の建築資材1つの交換 |

## 対象から外したカード（2026-11-25 時点）

### すでに main に実装済み、または open PR が追加済み（35枚）

A005 A007 A008 A016 A057 A077 A112 A117 A121 A125 A134 A135
B001 B005 B006 B007 B016 B037 B054 B058 B059 B073 B102 B117 B151 B160
C004 C005 C083
D004 D005 D007 D109
E007 E008

（open PR の追加分は `gh pr list --state open` の head ブランチと
`git diff origin/main...origin/<branch> -- src/cards.rs` で確認した。）

### `on_play` に獲得以外のステップがある（4枚）

- A013 改築事業所: clay 3 の後に無料改築（`renovate`）。
- A027 窯置き場: wood 2 の後に大進歩（粘土窯/石窯）の建設。
- A096 受注生産職人: wood 1 と同時に小進歩アクションの付与（`grant_action`）。
- B119 杣人: wood 1 と同時に将来のラウンドスペースへ wood を置く。

### 動的な品物・式・先のラウンドスペースが要る（6枚）

- A033 新しい土地: 残りラウンド数に比例する food と得点。
- B074 深い森 / B078 葦の群生帯 / B084 どんぐりいっぱいのカゴ: ラウンドスペースへの予約配置（`schedule`）。
- C050 厩の庭: 未開始ラウンド数に比例する獲得。
- C144 葦屋根の改装業者: 人数表による reed 獲得（2/4人=0、3人=1）。

### 条件付きだが既存の条件機構で表せない（2枚）

- B071 収穫小屋: 「完了した収穫数 = 出した職業数」の判定機構がない。
- B163 牧師: 「2部屋の家を持つ唯一のプレイヤー」という全員比較の条件機構がない。

### 動物のみを獲得する（3枚）

A011 ぬかるんだ畑（wild_boar）、B148 ペット仲買人（sheep）、D084 飼料ペレット（sheep）。

### 終了時得点と不可分（1枚）

A136 監督官: `on_play` の wood 獲得に `end_scoring` が付く EndGameBonus 系のため、他担当に委ねる。

### `needs_resolution`（実装停止ゲート、27枚）

A004 B022 B052 B136 B138 C040 C060 C121 C135 C136 C139 C140
D008 D088 D096 D097 D141 D145
E056 E060 E065 E124 E125 E155 E157 E165 E167

これらは仕様JSONの `unresolved_fields` が残っているため実装しない。文献要約と質問は次節。

## 人間への質問チェックリスト

各カードの仕様JSONにある `unresolved_fields` と、仕様が参照した第三者の文献URLをそのまま示す。
文献で確定できれば実装可能なので、判断をお願いしたい。

| ID | カード名 | 未決点（仕様JSONの unresolved_fields） | 文献 |
|---|---|---|---|
| A004 | 幅木 | play.cost: the wiki shows only alternative costs (2 food / 1 grain); the base resource cost is not stated | https://agricola.cloudfree.jp/?p=3421<br>https://agricola.cloudfree.jp/cardlist_en/ |
| B022 | 旅行靴 | prerequisites[0]: family_size_lte predicate needs implementation<br>effects[0].steps[1].operation: family_growth resource type needs definition | https://agricola.cloudfree.jp/?p=3235<br>https://agricola.cloudfree.jp/cardlist_en/ |
| B052 | 成育農場 | play.prerequisites: the wiki says 'pasture spaces >= completed rounds'; modeled as custom predicate 'pasture_spaces_at_least_completed_rounds' with no established encoding<br>effects.0.amount: 'current_round' count source has no established encoding; modeled as custom count | https://agricola.cloudfree.jp/?p=3205<br>https://agricola.cloudfree.jp/cardlist_en/ |
| B136 | 執事 | effects.1.award: the rule 'each player with the most rooms gets 3 bonus points' is a global scoring bonus to all tied players; no established gain_points target or predicate expresses 'all players tied for most rooms' (target enum only owner/left/right/all_other/selected/card/board/other) | https://agricola.cloudfree.jp/?p=3121<br>https://agricola.cloudfree.jp/cardlist_en/ |
| B138 | 森の守護者 | effects.1.tax: the mandatory pre-use payment by another player (food 1 before taking ≥5 wood) has no established step vocabulary — same gap as C051 (tax_space_use); modeled with spend_resource source other_player → owner, flagged<br>effects.1.threshold: 'takes at least 5 wood' is modeled as pile ≥5 via accumulation_at_least (assuming take-all); exact take amount not established | https://agricola.cloudfree.jp/?p=3119<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C040 | ズタ袋 | play.cost | https://agricola.cloudfree.jp/?p=3049<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C060 | 陶工の小窯 | play.prerequisites: the wiki requires returning the Clay Oven or Stone Oven major improvement; 'return an owned major improvement' has no established prerequisite predicate — encoded as has_improvement and flagged<br>effects.1.timing: the wiki allows building Clay Oven or Stone Oven before each Bake Bread action; a pre-action build window has no established trigger — modeled with after_worker_space on the bake_bread space and flagged | https://agricola.cloudfree.jp/?p=3029<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C121 | 陶土こね | effects: exact trigger condition for grain/vegetable seeds spaces (name matching)<br>effects: whether wood gain stacks when both spaces used in same round | https://agricola.cloudfree.jp/?p=2968<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C135 | 村長 | effects: exact calculation of 'rounds remaining' at time of play<br>effects: whether wood gains are cumulative (1/2/3/4) or separate thresholds<br>effects: whether 'no negative points' includes begging cards or just category negatives | https://agricola.cloudfree.jp/?p=2954<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C136 | 牧場長 | effects: exact calculation of 'rounds remaining' at time of play<br>effects: whether wood gains are 2/3/4 for 3+/6+/9+ rounds remaining<br>effects: exact definition of 'animal capacity' (pastures + stables + other?)<br>effects: tie-breaking for highest capacity | https://agricola.cloudfree.jp/?p=2953<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C139 | 籠造りの妻 | effects.1.timing: the at-any-time reed-to-food conversion exceeds the established trigger vocabulary; execution windows and replay ordering are not specified (same class as C018/C057/A071/E022) | https://agricola.cloudfree.jp/?p=2950<br>https://agricola.cloudfree.jp/cardlist_en/ |
| C140 | 包装の達人 | effects.1.substitution: taking Bake Bread instead of the Minor Improvement action is an action-substitution mechanic with no established formalization (same class as B026 Agrarian Fences) | https://agricola.cloudfree.jp/?p=2949<br>https://agricola.cloudfree.jp/cardlist_en/ |
| D008 | シダの胞子 | effects: "この小麦はすぐにまかなければならない" - the immediate-sow obligation on the gained grain is a follow-up action that needs formalization<br>play.prerequisites: 空の畑1と栽培中の畑2 - field-state prerequisite needs formalization | https://agricola.cloudfree.jp/?p=2913<br>https://agricola.cloudfree.jp/cardlist_en/ |
| D088 | 石臼職人 | effects[1].steps[0].params.scope: 'fence_stable_room_renovation' not in vocabulary - needs formal definition<br>effects[1].steps[0].params.max_substitutions: '2つまで' (up to 2 materials) per build action needs vocabulary<br>effects[1].steps[0].params.material_to: 'grain' as substitute for building resources needs confirmation<br>effects: two separate effects (on_play gain grain + passive substitute) - order field used | https://agricola.cloudfree.jp/?p=2833<br>https://agricola.cloudfree.jp/cardlist_en/ |
| D096 | 家具調達人 | effects[1].steps[0].params: discount_cost で『進歩1つを木材1安く出せる』が小進歩のみか大進歩も含むか（日語: 進歩=大小進歩両方の可能性、英語版確認要）<br>effects[1].steps[0].params.segment_cap: 『1つ』が部屋建設1回につき1回のみか、累積上限があるか<br>classification: 段階3（部屋・建設イベント）だが、割引対象が小進歩のみか大進歩も含むかが文献で要確認 | https://agricola.cloudfree.jp/?p=2825<br>https://agricola.cloudfree.jp/cardlist_en/ |
| D097 | 無心する学生 | effects[1].steps[0].params.kind=other: play_occupation_without_cost の詳細仕様（手札からのみか市場からも可か、コスト『なし』の正確な意味（食料・資源両方免除か）、同一収穫で複数回不可の制御）<br>effects[1].trigger: harvest_start で『各収穫の開始時』のタイミングと、収穫のないラウンドでの扱い<br>classification: 段階4（P1 出したとき＋収穫の開始）だが、職業コスト免除の範囲と手札/市場の区別が文献で確定できない | https://agricola.cloudfree.jp/?p=2824<br>https://agricola.cloudfree.jp/cardlist_en/ |
| D141 | 種屋 | effects[1].conditions: action_space_name predicate needs to be in vocabulary (Sow action space name)<br>effects[1].trigger: whether this is the 'Sow' or 'Sow and/or Bake Bread' action space | https://agricola.cloudfree.jp/?p=2780<br>https://agricola.cloudfree.jp/cardlist_en/ |
| D145 | 屋根点検者 | effects[0].steps: conditional gain based on major improvement count needs formal parameterization - single step with conditional amount or multiple mutually exclusive steps<br>effects[0].steps: whether major improvements counting includes those just played this action | https://agricola.cloudfree.jp/?p=2776<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E056 | ローマ土器 | trigger 'round_start' with condition 'own_placement_index_this_round' - needs formal definition of turn order check<br>resource 'food' on card itself needs engine support for card-local storage<br>cost shows '1 レンガ' in wiki but ledger says clay:1 - using clay:1 from ledger; alternative_cost_notes captures '1 brick' | https://agricola.cloudfree.jp/?p=2697<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E060 | 軍手 | trigger 'pay_occupation_cost' needs formal definition with access to payment context<br>operation 'discount_cost' with substitution logic needs engine support - '食料2までを建築資材1つで代用' means 2 food = 1 building material<br>which building materials are eligible needs clarification (wood, clay, stone, reed?) | https://agricola.cloudfree.jp/?p=2693<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E065 | お布施袋 | operation uses 'round_number' expression - needs engine support for current round query<br>prerequisite '職業なし' = 0 occupations - using occupation_count with n=0<br>effect: '完了したラウンド2つごとに小麦1' - per 2 completed rounds, gain 1 grain | https://agricola.cloudfree.jp/?p=2688<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E124 | 市長候補者 | effects: cannot discard wood/stone until game end - mechanism for this restriction unclear<br>effects: exact scoring penalty calculation timing | https://agricola.cloudfree.jp/?p=2629<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E125 | 遅れてきた旅人 | effects: which building resource can be chosen (wood/brick/reed/stone)<br>effects: exact timing of extra person placement - 'after all persons placed this round' | https://agricola.cloudfree.jp/?p=2628<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E155 | 空想家 | effects.1 kind 'forbid_family_growth': no established encoding prohibits the owner's family growth (nearest: forbid_renovation / forbid_house_animals)<br>effects.1 escape condition: '他のプレイヤー全員が家族を増やさない限り' (unless every other player has already grown their family) needs a formal predicate (nearest: all_other_players_more_family, E014; not identical)<br>effects.1 boundary: 'ラウンド11まで' (until round 11) - whether round 11 itself is included is not stated | https://agricola.cloudfree.jp/?p=2598<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E157 | 使用権利人 | effects.0.amount: the count source for 'all occupations in play by any player, excluding this card' is not established (nearest: own-only 'occupation' count; B099 uses 'own_occupations_after_this_card')<br>effects.0.conditions: 'first occupation' expressed as occupation_count n=0 evaluated before this card enters play; the predicate has precedent in prerequisites but its use as an effect condition with timing 'before this card' is not established | https://agricola.cloudfree.jp/?p=2596<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E165 | 熟練猟師 | effects.1.conditions: using improvement_played_from_hand with kind=major follows E018's pattern (kind=minor) and B103's precedent for 'kind: major' in declined_improvement; however 'builds a major improvement' via the Major Improvement action may not fully map to 'played from hand', so the condition may need a dedicated predicate 'improvement_is_major' | https://agricola.cloudfree.jp/?p=2588<br>https://agricola.cloudfree.jp/cardlist_en/ |
| E167 | 酪農家 | effects.0.steps[0]: 'each player chooses' — the decision belongs to each affected player (decision_owner affected_player has no usage precedent; E009-style choices are made by the owner)<br>effects.0: the resolution order of the players' independent choices and whether they are simultaneous or sequential needs definition | https://agricola.cloudfree.jp/?p=2586<br>https://agricola.cloudfree.jp/cardlist_en/ |

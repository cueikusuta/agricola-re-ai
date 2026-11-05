# M6 A/Bカード効果パターン一次台帳（Revised 2016 基本セット）

- 対象はA/B各48枚（職業48枚、小改善48枚）、合計96枚。外部リファレンスを確認して規則を実装パターンへ一次仕分けしたもの。
- **一次分類は実装済み宣言ではない**。「常時・時限」にも任意選択・複数タイミング・途中の状態参照を含み得る。個別にルールを精査する。
- 原文・カード画像は保存しない。名称は識別用、説明はコードに自分の短い言葉で記述する。
- 未実装カードは配布しない。単純候補の効果が現在のフックで表現できない場合は、推測や簡略化をせず別Issueで扱う。
- 確認参照: https://agricola.veronahe.no/deck_RevisedA / https://agricola.veronahe.no/deck_RevisedB。ルールの優先順位は公式Lookout付録・規則。

## 全カードの一次仕分け

| パック | 種別 | カード識別名 | 効果パターン（暫定） | 状態 |
|---|---|---|---|---|
| A | 小改善 | Basket | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Big Country | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Canoe | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Clay Embankment | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Claypipe | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Clearing Spade | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Corn Scoop | 常時・時限 | 動作確認済・効果実装待ち |
| A | 小改善 | Drinking Trough | 複雑・要精査 | 効果レビュー待ち |
| A | 小改善 | Dutch Windmill | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Handplow | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Junk Room | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Large Greenhouse | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Lumber Mill | 複雑・要精査 | 効果レビュー待ち |
| A | 小改善 | Manger | 終了時得点 | 効果レビュー待ち |
| A | 小改善 | Milk Jug | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Pond Hut | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Rammed Clay | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Shepherd's Crook | 即時 | 効果レビュー待ち |
| A | 小改善 | Shifting Cultivation | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Sleeping Corner | 複雑・任意選択 | 効果レビュー待ち |
| A | 小改善 | Stone Tongs | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Threshing Board | 常時・時限 | 効果レビュー待ち |
| A | 小改善 | Wool Blankets | 終了時得点 | 効果レビュー待ち |
| A | 小改善 | Young Animal Market | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Adoptive Parents | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Animal Dealer | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Animal Tamer | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Braggart | 終了時得点 | 効果レビュー待ち |
| A | 職業 | Clay Hut Builder | 常時・時限 | 効果レビュー待ち |
| A | 職業 | Conjurer | 常時・時限 | 効果レビュー待ち |
| A | 職業 | Conservator | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Firewood Collector | 常時・時限 | 動作確認済・効果実装待ち |
| A | 職業 | Frame Builder | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Grocer | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Harpooner | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Hedge Keeper | 常時・時限 | 効果レビュー待ち |
| A | 職業 | Lutenist | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Mushroom Collector | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Pig Breeder | 即時 | 効果レビュー待ち |
| A | 職業 | Plow Driver | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Priest | 即時 | 効果レビュー待ち |
| A | 職業 | Roughcaster | 常時・時限 | 効果レビュー待ち |
| A | 職業 | Scythe Worker | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Seasonal Worker | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Stable Architect | 終了時得点 | 効果レビュー待ち |
| A | 職業 | Stonecutter | 複雑・要精査 | 効果レビュー待ち |
| A | 職業 | Wall Builder | 複雑・任意選択 | 効果レビュー待ち |
| A | 職業 | Wood Cutter | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Acorns Basket | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Beanfield | 複雑・要精査 | 効果レビュー待ち |
| B | 小改善 | Bottles | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Bread Paddle | 即時 | 効果レビュー待ち |
| B | 小改善 | Brook | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Butter Churn | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Caravan | 複雑・要精査 | 効果レビュー待ち |
| B | 小改善 | Carpenter's Parlor | 複雑・要精査 | 効果レビュー待ち |
| B | 小改善 | Hard Porcelain | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Herring Pot | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Lasso | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Loam Pit | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Loom | 終了時得点 | 効果レビュー待ち |
| B | 小改善 | Mantelpiece | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Market Stall | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Mini Pasture | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Mining Hammer | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Moldboard Plow | 複雑・任意選択 | 効果レビュー待ち |
| B | 小改善 | Pitchfork | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Sack Cart | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Scullery | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Strawberry Patch | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Thick Forest | 常時・時限 | 効果レビュー待ち |
| B | 小改善 | Three-Field Rotation | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Assistant Tiller | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Brushwood Collector | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Carpenter | 複雑・要精査 | 効果レビュー待ち |
| B | 職業 | Cattle Feeder | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Childless | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Consultant | 即時 | 効果レビュー待ち |
| B | 職業 | Cottager | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Geologist | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Greengrocer | 常時・時限 | 動作確認済・効果実装待ち |
| B | 職業 | Groom | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | House Steward | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Manservant | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Master Bricklayer | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Organic Farmer | 終了時得点 | 効果レビュー待ち |
| B | 職業 | Oven Firing Boy | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Paper Maker | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Pastor | 即時 | 効果レビュー待ち |
| B | 職業 | Roof Ballaster | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Scholar | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Sheep Walker | 複雑・任意選択 | 効果レビュー待ち |
| B | 職業 | Sheep Whisperer | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Small-Scale Farmer | 常時・時限 | 効果レビュー待ち |
| B | 職業 | StoreHouse Keeper | 常時・時限 | 効果レビュー待ち |
| B | 職業 | Tutor | 終了時得点 | 効果レビュー待ち |

## 個別実装ゲート

1. 対象効果の全発動条件・タイミング・費用・前提条件を公式ルールと照合。
2. 1カード1テストを作成し、同じフック型の複数カードを1 Issue/1 branch/1 PRに束ねる。
3. 公式文面の保存は禁止。実装済みカードだけのデッキを選べるようにし、サンプルの試合とログ/セーブ再現を確認。
4. 誤解釈が残るカードは推測実装しない。Issueに不確実な解釈と判断ポイントを残す。

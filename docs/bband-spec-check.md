# B帯仕様データの文献照合（B帯147枚）

担当: Freebuff。ブランチ `research/freebuff-ab-bband-spec-check`（親: `research/freebuff-ab-spec-check`）。
PRは作成まで。**マージしない。**

## 第0章 目的・範囲・方法

### 0.1 依頼

`data/bga-card-specs/` のB帯（B001–B168）の仕様データを、A帯と同じ方法で1枚ずつ文献と突き合わせる。
食い違いは文献の根拠付きで修正し、決まらないものは人間への質問として残す。

### 0.2 既照合分の除外

| PR | B帯の対象 | 枚数 |
|---|---|---:|
| #387 `docs/impl-source-check.md` | 実装済みカード | 21 |
| #394 `docs/impl-source-check-2.md` | なし | 0 |
| #402 `docs/dband-spec-check.md` | B008をtravelingの校正用例に使用（対象一覧には無い） | 0 |

B帯168枚から21枚を除いた **147枚** を対象とする。除外は
B006, B007, B010, B013, B050, B057, B061, B062, B064, B077, B095, B098, B099, B102, B107,
B118, B121, B122, B126, B142, B144。B008 は #402 が traveling の校正に使っただけなので対象に含めた
（`post_play` は一致、効果の構造も一致）。

### 0.3 文献と等級・判定区分

A帯（`docs/aband-spec-check.md` 0.3・0.4）と同じ。B帯は個別ページの `?p=` が別カードへ遷移するものが4枚
（B010, B013, B062, B095 ほか）あり、英語一覧・日本語一覧と台帳で同定した。

### 0.4 B帯に多い構造的な問題

1. **語彙外の trigger / 述語 / kind**: `any_time`, `scoring`, `each_field_phase`, `on_use_*`,
   `other(description=...)`（kind 無しのダミー）など。B帯の表現不足69枚の大半がこれ。
2. **得点を資源として獲得**: `gain_resource(good="bonus_points")` が5枚（B029, B031, B034, B035, B038）。
   本来は `gain_points`。
3. **品物名の語彙外**: `brick`（B156, B163）、`boar`（B147, B165）、`card`, `cumulative_resource`,
   `action_space_use`, `family_growth`, `animal_of_owned_type`, `clay_or_stone`。
4. **`post_play: left_neighbor_hand`（17枚）**: 文献の効果文に「左隣へ渡す」文言が無い（第4章 Q-1）。

## 第1章 照合結果の集計

対象147枚。判定は 一致50・表現不足70・食い違い17・未確定10。
本PRで仕様を修正したのは **22枚**。費用・印刷点は全147枚で台帳と一致し、不一致0。
B帯は「文献と読解が違う」より「文献の読みを語彙で表せていない」ものが多く、
修正も語彙に寄せる形（`gain_points`・`wild_boar`・`clay`・対象スペース名）が中心になる。

## 第2章 全147枚の照合表

| ID | カード | 判定 | 仕様と文献の差（要約） | 本PRの扱い | 文献 |
|---|---|---|---|---|---|
| B001 | 裕福な生活 | 一致 | レンガ5と改築（費用は支払う）。 | — | [?p=3256](https://agricola.cloudfree.jp/?p=3256) |
| B002 | 小牧場 | 一致 | 1マス牧場を無料で、既存牧場に隣接。 | — | [?p=3255](https://agricola.cloudfree.jp/?p=3255) |
| B003 | 密造酒 | 一致 | 手札の職業1枚を無作為に選び、飯2で出すか左隣へ。 | — | [?p=3254](https://agricola.cloudfree.jp/?p=3254) |
| B004 | 薪の山 | 未確定 | 累積スペース上の自分の人物数ぶん木材。公開状態に人数が無いため effects=null の保留（Issue #347）。文献と矛盾しない。 | 質問 | [?p=3253](https://agricola.cloudfree.jp/?p=3253) |
| B005 | 経験の蓄積 | 一致 | 手札の職業0-4/5/6/7枚で石/葦/レンガ/木1。 | — | [?p=3252](https://agricola.cloudfree.jp/?p=3252) |
| B008 | 露店 | 一致 | 小麦1（費用）を野菜1に替える。post_play の左隣移動も一致。 | — | [?p=3249](https://agricola.cloudfree.jp/?p=3249) |
| B009 | むち | 一致 | 葦1を得るか、葦1を牛1に交換。 | — | [?p=3248](https://agricola.cloudfree.jp/?p=3248) |
| B011 | 畝 | 表現不足 | 収容数=牧場数、繁殖後に空き枠ぶんの飯。仕様は kind の無い description だけの other ステップで、意味を機械可読に表していない。 | — | [?p=3246](https://agricola.cloudfree.jp/?p=3246) |
| B012 | 家畜収容所 | 表現不足 | 同種3頭までの収容、牧場扱いしない。仕様は description のみ。 | — | [?p=3245](https://agricola.cloudfree.jp/?p=3245) |
| B014 | 鷹の塔 | 表現不足 | ラウンド12に石の部屋を置き、その時点で石の家なら無料建設。仕様は description のみ。前提のラウンド7以前は一致。 | — | [?p=3243](https://agricola.cloudfree.jp/?p=3243) |
| B015 | 工作台 | 食い違い | 仕様に家具職人小屋と同じ discount_cost(wooden_room,2) が混入している。文献は木材累積スペース使用後に、そこで得た木材だけで牧場1つ（柵1本無料）。 | 修正 | [?p=3242](https://agricola.cloudfree.jp/?p=3242) |
| B016 | 解体ハンマー | 一致 | 飯1と、改築ごとに木を払わず厩1。 | — | [?p=3241](https://agricola.cloudfree.jp/?p=3241) |
| B017 | 森林鋤 | 一致 | 木材累積スペースで木2を払って畑1、木はスペースへ。 | — | [?p=3240](https://agricola.cloudfree.jp/?p=3240) |
| B018 | 草地鍬 | 食い違い | 前提「職業2とストックに建設資材1」が仕様に無い。効果も description のみ。 | 修正 | [?p=3239](https://agricola.cloudfree.jp/?p=3239) |
| B019 | 突き鋤 | 食い違い | 前提「職業1」が仕様に無い。効果は一致。 | 修正 | [?p=3238](https://agricola.cloudfree.jp/?p=3238) |
| B020 | 鎖状農具 | 一致 | ラウンド+7/+8/+9の畑。 | — | [?p=3237](https://agricola.cloudfree.jp/?p=3237) |
| B021 | 干し草小屋 | 食い違い | 前提「職業1」が仕様に無い。加えて文献は「小麦1以上を入庫するたび」だが仕様は木材1以上の条件で飯1を得る。 | 修正 | [?p=3236](https://agricola.cloudfree.jp/?p=3236) |
| B022 | 旅行靴 | 表現不足 | 飯2と人物1の即時配置、次の帰宅フェイズに除去。仕様は人物を資源のように獲得し、除去を表していない。前提（人物4人以下）は一致。 | — | [?p=3235](https://agricola.cloudfree.jp/?p=3235) |
| B023 | 最終章 | 表現不足 | ラウンド14のカードを表にして先行使用。仕様は description のみ。前提のラウンド13以前は一致。 | — | [?p=3234](https://agricola.cloudfree.jp/?p=3234) |
| B024 | 投げ縄 | 表現不足 | 家族2人を連続配置し、少なくとも1人は家畜市場へ。仕様は description のみ。 | — | [?p=3233](https://agricola.cloudfree.jp/?p=3233) |
| B025 | パン焼き棒 | 一致 | 飯1と、職業を出すたびパンを焼く。 | — | [?p=3232](https://agricola.cloudfree.jp/?p=3232) |
| B026 | 耕地の垣根 | 表現不足 | 穀物活用の2アクションの一方を柵の建設に置換。仕様は description のみで trigger も語彙外。 | — | [?p=3231](https://agricola.cloudfree.jp/?p=3231) |
| B027 | 工具箱 | 表現不足 | 労働フェイズに部屋/厩/柵を建てたら手番後に家具製作所・製陶所・カゴ製作所の1つ。仕様は description のみ。 | — | [?p=3230](https://agricola.cloudfree.jp/?p=3230) |
| B028 | 林学教育 | 表現不足 | 森林で木2を戻して職業コスト無しの職業1。仕様は description のみ。 | — | [?p=3229](https://agricola.cloudfree.jp/?p=3229) |
| B029 | 料理教室 | 食い違い | 仕様は gain_resource(good=bonus_points) で、語彙に無い品物。文献は同一手番で授業と鍋進歩を使うと1点。 | 修正 | [?p=3228](https://agricola.cloudfree.jp/?p=3228) |
| B030 | 棒杭柵 | 表現不足 | 外周の柵を木2で置けて1点。仕様は description のみで trigger も語彙外（free_outer_edge_fences は無料化のみ）。 | — | [?p=3227](https://agricola.cloudfree.jp/?p=3227) |
| B031 | 製陶所の敷地 | 食い違い | 仕様は bonus_points の獲得で、得点計算の条件も未表現。文献は得点計算時に隣接する未使用農場2つ以上で2点。 | 修正 | [?p=3226](https://agricola.cloudfree.jp/?p=3226) |
| B032 | 煮沸釜 | 表現不足 | 交換の数値は一致。trigger any_time と description だけの交換が語彙外。 | 修正 | [?p=3225](https://agricola.cloudfree.jp/?p=3225) |
| B033 | 暖炉 | 一致 | 残りラウンド×1点、以後の改築禁止。印刷点-3も文献と一致。 | — | [?p=3224](https://agricola.cloudfree.jp/?p=3224) |
| B034 | 特別な餌 | 表現不足 | 累積スペースの家畜を全て収容できた初回に頭数ぶんの点。仕様は1点固定の bonus_points で頭数を表していない。 | 修正 | [?p=3223](https://agricola.cloudfree.jp/?p=3223) |
| B035 | 装蹄刀 | 食い違い | 文献は羊の頭数が人数別の閾値（9/8/7/6/5/5）に達した時点で1度だけ2点。仕様は得点計算時（scoring）の発動で時点が違う。 | 修正 | [?p=3222](https://agricola.cloudfree.jp/?p=3222) |
| B036 | ガラスビン | 表現不足 | 費用は家族1人につきレンガ1+飯1。仕様は費用0で description のみ。 | — | [?p=3221](https://agricola.cloudfree.jp/?p=3221) |
| B037 | 大農場の称号 | 一致 | 飯1、印刷3点、前提（畑6と全種類の家畜）も一致。 | — | [?p=3220](https://agricola.cloudfree.jp/?p=3220) |
| B038 | 建設予定地 | 食い違い | 文献は「家に隣接する未使用スペースは、他の農場スペースが全て使用済みになるまで使えない」という制約。仕様は得点計算で1点を得る効果になっている。 | 修正 | [?p=3219](https://agricola.cloudfree.jp/?p=3219) |
| B039 | 機織り機 | 食い違い | 文献は畑フェイズに羊1/4/7頭で飯1/2/3、得点計算で羊3頭につき1点。仕様は飯1固定・得点1固定で、述語名も語彙外（occupation_count_gte）。 | 修正 | [?p=3218](https://agricola.cloudfree.jp/?p=3218) |
| B040 | 醸造所の池 | 表現不足 | 効果（漁/葦原で小麦1と木材1）は一致。前提の述語名 occupation_count_gte が語彙外。 | — | [?p=3217](https://agricola.cloudfree.jp/?p=3217) |
| B041 | ハウベルグ | 一致 | 4ラウンド先へ木材2と猪1を交互に、順序は所有者が選ぶ。 | — | [?p=3216](https://agricola.cloudfree.jp/?p=3216) |
| B042 | 森の宿屋 | 表現不足 | 文献は他者が使うとき所有者へ食料1。仕様は交換のみで、その支払いが無い。交換量（人数別）は一致。 | — | [?p=3215](https://agricola.cloudfree.jp/?p=3215) |
| B043 | 料理店 | 未確定 | 費用（木2/レンガ2の選択）が仕様で未確定、3/2ラウンドの対応も文献に無い。仕様の notes も同じ保留を記録している。 | 質問 | [?p=3214](https://agricola.cloudfree.jp/?p=3214) |
| B044 | ひな小屋 | 未確定 | 効果（ラウンド+3/+4に飯2）は一致。費用が木1/レンガ1の選択で仕様は0のため未確定。 | 質問 | [?p=3213](https://agricola.cloudfree.jp/?p=3213) |
| B045 | イチゴ花壇 | 表現不足 | 効果は一致。前提の述語名 vegetable_field_count が語彙外（field_count/grain_field_count の姉妹語彙が未整備）。 | — | [?p=3212](https://agricola.cloudfree.jp/?p=3212) |
| B046 | 会館 | 未確定 | 効果（4ラウンドの飯1＋次の石1）は一致。費用が木3/レンガ2の選択で仕様は0。 | 質問 | [?p=3211](https://agricola.cloudfree.jp/?p=3211) |
| B047 | ニシン鍋 | 一致 | 漁の使用ごとに3ラウンド先まで飯1。 | — | [?p=3210](https://agricola.cloudfree.jp/?p=3210) |
| B048 | 森の石 | 表現不足 | 東の採石場が抜けている（西のみ実装）。文献は石材の累積スペース全般。 | — | [?p=3209](https://agricola.cloudfree.jp/?p=3209) |
| B049 | 天秤 | 一致 | 進歩/職業の配置で両者同数なら飯2。 | — | [?p=3208](https://agricola.cloudfree.jp/?p=3208) |
| B051 | 採掘シャベル | 一致 | 粘土坑の使用ごとに猪の頭数ぶんの飯。 | — | [?p=3206](https://agricola.cloudfree.jp/?p=3206) |
| B052 | 成育農場 | 表現不足 | 効果（現在ラウンド数の飯）は一致。前提が語彙外の custom kind。 | — | [?p=3205](https://agricola.cloudfree.jp/?p=3205) |
| B053 | 彫刻学科 | 一致 | 収穫のないラウンド末に木1→飯2か石1→飯4。 | — | [?p=3204](https://agricola.cloudfree.jp/?p=3204) |
| B054 | 肥料運搬車 | 一致 | 飯2と、制約なし種まきごとに厩数ぶんの飯。 | — | [?p=3203](https://agricola.cloudfree.jp/?p=3203) |
| B055 | 整備保証 | 一致 | カード上に飯3、木材累積で1つ取得、改築で3まで補充。 | — | [?p=3202](https://agricola.cloudfree.jp/?p=3202) |
| B056 | 小川 | 一致 | 漁の上方4スペースの使用で飯1。前提（家族が漁にいる）も一致。 | — | [?p=3201](https://agricola.cloudfree.jp/?p=3201) |
| B058 | 雑草掻き | 一致 | 飯1と、畑から取った野菜1につき飯1。 | — | [?p=3199](https://agricola.cloudfree.jp/?p=3199) |
| B059 | 食料箱 | 一致 | 大進歩スペースで出せば飯4、それ以外は飯2。 | — | [?p=3198](https://agricola.cloudfree.jp/?p=3198) |
| B060 | 仕込み水 | 一致 | 漁の使用ごとに小麦1で6ラウンド先まで飯1。 | — | [?p=3197](https://agricola.cloudfree.jp/?p=3197) |
| B063 | 味見 | 一致 | 授業の使用時に小麦1→飯4（職業コスト支払い前）。 | — | [?p=3194](https://agricola.cloudfree.jp/?p=3194) |
| B065 | 小麦貯蔵庫 | 未確定 | 効果（支払った材質で2/3/4ラウンド先に小麦1）は一致。費用が木2/レンガ2/石2の選択で仕様は0。 | 質問 | [?p=3192](https://agricola.cloudfree.jp/?p=3192) |
| B066 | 荷車 | 一致 | ラウンド5/8/11/14のうち未開始のスペースに小麦1。 | — | [?p=3191](https://agricola.cloudfree.jp/?p=3191) |
| B067 | 手押し台車 | 食い違い | 文献は「占有している累積スペース1か所につき小麦1」。仕様は常に1個で、簡略化した旨を notes に書いている。 | 修正 | [?p=3190](https://agricola.cloudfree.jp/?p=3190) |
| B068 | マメ畑 | 未確定 | 文献に左隣へ渡す文言が無いのに post_play が left_neighbor_hand。効果（野菜専用の畑1つになる）も plow_field では近似。 | 質問 | [?p=3189](https://agricola.cloudfree.jp/?p=3189) |
| B069 | 陶工市 | 未確定 | 文献に左隣へ渡す文言が無いのに post_play が left_neighbor_hand。効果も description のみ。 | 質問 | [?p=3188](https://agricola.cloudfree.jp/?p=3188) |
| B070 | 新規購入 | 一致 | 収穫のあるラウンド開始前に飯2→小麦1、飯4→野菜1を各1まで。 | — | [?p=3187](https://agricola.cloudfree.jp/?p=3187) |
| B071 | 収穫小屋 | 一致 | 収穫数と職業数が同数なら飯1・小麦1・野菜1。 | — | [?p=3186](https://agricola.cloudfree.jp/?p=3186) |
| B072 | 農業愛 | 未確定 | 文献に左隣へ渡す文言が無いのに post_play が left_neighbor_hand。効果も description のみ。 | 質問 | [?p=3185](https://agricola.cloudfree.jp/?p=3185) |
| B073 | 贈り物かご | 一致 | 部屋数2/3/4/5で野菜/飯/小麦/野菜。 | — | [?p=3184](https://agricola.cloudfree.jp/?p=3184) |
| B074 | 深い森 | 表現不足 | 効果（偶数ラウンドに木材1）は一致。前提が語彙外（resource_in_hand）、効果も語彙外の schedule 形式。 | — | [?p=3183](https://agricola.cloudfree.jp/?p=3183) |
| B075 | 木工作業場 | 表現不足 | 前提は一致。効果の trigger before_play_improvement が語彙外。 | — | [?p=3182](https://agricola.cloudfree.jp/?p=3182) |
| B076 | 天井 | 未確定 | 文献に左隣へ渡す文言が無いのに post_play が left_neighbor_hand。効果も description のみ。 | 質問 | [?p=3181](https://agricola.cloudfree.jp/?p=3181) |
| B078 | 葦の群生帯 | 未確定 | 文献に左隣へ渡す文言が無いのに post_play が left_neighbor_hand。効果の内容（5/8/10/12の葦）は一致。 | 質問 | [?p=3179](https://agricola.cloudfree.jp/?p=3179) |
| B079 | 荷揚げかご | 表現不足 | 意味（誰かが石材3以上取ると石1）は一致。trigger と条件が語彙外の独自名で、条件を params 内の文字列で持っている。 | — | [?p=3178](https://agricola.cloudfree.jp/?p=3178) |
| B080 | 硬磁器 | 表現不足 | 交換内容は一致。trigger any_time と exchange(rates=) が語彙外。 | — | [?p=3177](https://agricola.cloudfree.jp/?p=3177) |
| B081 | 手引き車 | 表現不足 | しきい値6/5/4/4と1か所1個は一致。獲得を gain_resource(condition=...) で表しており語彙外。 | — | [?p=3176](https://agricola.cloudfree.jp/?p=3176) |
| B082 | 資産価値 | 表現不足 | 交換内容は一致。harvest_end と exchange(offers=) が語彙外。 | — | [?p=3175](https://agricola.cloudfree.jp/?p=3175) |
| B083 | 泥溜り | 表現不足 | カード上の山とレンガ1での取得は一致するが、description だけの other で機械可読でない。 | — | [?p=3174](https://agricola.cloudfree.jp/?p=3174) |
| B084 | どんぐりいっぱいのカゴ | 表現不足 | 2ラウンド先の猪1は一致。gain_resource(schedule=) が語彙外。 | — | [?p=3173](https://agricola.cloudfree.jp/?p=3173) |
| B085 | 作手 | 表現不足 | 2×2の中心に厩（住む場所1・家畜不可）は一致。trigger on_build_stable と条件が語彙外。 | — | [?p=3172](https://agricola.cloudfree.jp/?p=3172) |
| B086 | トリュフ探し | 表現不足 | 収容数=完了した食料供給フェイズ数。description のみ。 | — | [?p=3171](https://agricola.cloudfree.jp/?p=3171) |
| B087 | 日雇い農夫 | 表現不足 | 日雇いで部屋建設か改築（費用を払う）は一致。trigger が語彙外。 | — | [?p=3170](https://agricola.cloudfree.jp/?p=3170) |
| B088 | 成功者 | 一致 | 2部屋なら無償改築、直後に柵の建設。 | — | [?p=3169](https://agricola.cloudfree.jp/?p=3169) |
| B089 | 馬手 | 表現不足 | 木材1と、石の家で毎ラウンド木1の厩1は一致するが description のみ。 | — | [?p=3168](https://agricola.cloudfree.jp/?p=3168) |
| B090 | 組合の鋤手 | 食い違い | 文献は「小麦の種が占有された状態で『農地』を使うたび」。仕様は農地ではなく耕作（cultivation）を対象にしている。 | 修正 | [?p=3167](https://agricola.cloudfree.jp/?p=3167) |
| B091 | 鋤手助手 | 表現不足 | 意味（日雇いで畑1追加）は一致。trigger 名が語彙外。 | — | [?p=3166](https://agricola.cloudfree.jp/?p=3166) |
| B092 | 靴下編み | 表現不足 | ラウンド5以降の羊市場で増員は一致。trigger とスペース名（日本語文字列）が語彙外。 | — | [?p=3165](https://agricola.cloudfree.jp/?p=3165) |
| B093 | 腹心の友 | 表現不足 | 2〜4ラウンド先への飯1と、回収して種まき/柵の選択は一致。description パラメータで機械可読でない。 | — | [?p=3164](https://agricola.cloudfree.jp/?p=3164) |
| B094 | 家畜警護 | 表現不足 | 柵の建設前に木2、直後に別人物1は一致。trigger と description が語彙外。 | — | [?p=3163](https://agricola.cloudfree.jp/?p=3163) |
| B096 | 養樹園の指物師 | 表現不足 | 次の奇数2ラウンドの木材と小進歩は一致。rounds_ahead=next_2_odd など語彙外。 | — | [?p=3161](https://agricola.cloudfree.jp/?p=3161) |
| B097 | 学者 | 表現不足 | 石の家で毎ラウンド開始時に職業1（飯1）か小進歩（通常コスト）は一致。description 依存。 | — | [?p=3160](https://agricola.cloudfree.jp/?p=3160) |
| B100 | 散らかし屋 | 表現不足 | 得点計算で「累積スペース」を含む後出しカード1枚につき1点は一致。trigger と条件が語彙外。 | — | [?p=3157](https://agricola.cloudfree.jp/?p=3157) |
| B101 | 建具大工 | 表現不足 | 収穫ごとに誰かが家具製作所を持てば飯2で1点は一致。trigger harvest と params が語彙外。 | — | [?p=3156](https://agricola.cloudfree.jp/?p=3156) |
| B103 | 畑の取引人 | 一致 | 木1と葦1、小進歩辞退で飯1/大進歩辞退で野菜1。 | — | [?p=3154](https://agricola.cloudfree.jp/?p=3154) |
| B104 | 夢羊病者 | 表現不足 | いつでも羊1を猪/野菜/石1に交換は一致。形式が語彙外。 | — | [?p=3153](https://agricola.cloudfree.jp/?p=3153) |
| B105 | 箱造り | 一致 | 2つ以上ある食料/小麦/野菜/葦/木を各1。 | — | [?p=3152](https://agricola.cloudfree.jp/?p=3152) |
| B106 | 清廉潔白な人 | 一致 | ラウンド開始直前に開始前スペースに品物があれば飯1。 | — | [?p=3151](https://agricola.cloudfree.jp/?p=3151) |
| B108 | かまど焚き | 表現不足 | 木材累積スペースの使用ごとにパンを焼くは一致。trigger/条件が語彙外。 | — | [?p=3149](https://agricola.cloudfree.jp/?p=3149) |
| B109 | 紙すき | 表現不足 | 職業を出す直前に木1で、前の職業1枚につき飯1は一致。trigger が語彙外。 | — | [?p=3148](https://agricola.cloudfree.jp/?p=3148) |
| B110 | 路面舗装工 | 食い違い | 文献はラウンド14では食料の代わりに野菜1。仕様は2つの効果が並列で、ラウンド14に飯1と野菜1の両方を得る。 | 修正 | [?p=3147](https://agricola.cloudfree.jp/?p=3147) |
| B111 | 純朴な人 | 表現不足 | 新しいレンガの部屋1部屋につき飯2と1点は一致。trigger build_room と条件名が語彙外。 | — | [?p=3146](https://agricola.cloudfree.jp/?p=3146) |
| B112 | サイロ番 | 表現不足 | 直近に公開されたアクションスペースカードの使用で小麦1は一致。trigger が語彙外。 | — | [?p=3145](https://agricola.cloudfree.jp/?p=3145) |
| B113 | 苗床の管理人 | 表現不足 | 飯1→小麦1/飯3→野菜1と畑1になる点は一致。description 依存。 | — | [?p=3144](https://agricola.cloudfree.jp/?p=3144) |
| B114 | 子なし | 表現不足 | 部屋3以上で家族ちょうど2なら飯1＋小麦1か野菜1は一致。パラメータ名が語彙外。 | — | [?p=3143](https://agricola.cloudfree.jp/?p=3143) |
| B115 | 板金工の親方 | 表現不足 | 厩なし牧場の収容1と、種まきごとに畑へ+1は一致。形式が語彙外。 | — | [?p=3142](https://agricola.cloudfree.jp/?p=3142) |
| B116 | 岸辺の林業者 | 表現不足 | 出した時と、空の葦原に葦が置かれるたびに木1は一致。trigger 名が語彙外。 | — | [?p=3141](https://agricola.cloudfree.jp/?p=3141) |
| B117 | 情報屋 | 一致 | 木1と、労働フェイズ後に石>レンガなら木1。 | — | [?p=3140](https://agricola.cloudfree.jp/?p=3140) |
| B119 | 杣人 | 表現不足 | 木1と、建設済み柵の本数ぶん先までのラウンドに木1は一致。description と独自パラメータで機械可読でない。 | — | [?p=3138](https://agricola.cloudfree.jp/?p=3138) |
| B120 | 掃除屋 | 表現不足 | 直近に置かれたラウンドカードの上のスペース使用前にレンガ2は一致。trigger が語彙外。 | — | [?p=3137](https://agricola.cloudfree.jp/?p=3137) |
| B123 | 屋根付け | 食い違い | 文献は飯1を払って部屋数ぶんの石材。仕様は spend_resource(飯1) と exchange(飯1→石材) の二重支払い。 | 修正 | [?p=3134](https://agricola.cloudfree.jp/?p=3134) |
| B124 | 整地人 | 表現不足 | 石材2は一致。ただし「各労働フェイズ1回まで」「既存牧場の分割は含めない」が未表現。 | — | [?p=3133](https://agricola.cloudfree.jp/?p=3133) |
| B125 | 借地農夫 | 一致 | 4ラウンド先へ木・レンガ・葦・石を順に。 | — | [?p=3132](https://agricola.cloudfree.jp/?p=3132) |
| B127 | 女たらし | 一致 | ラウンド5以降に出せば石1・小麦1・野菜1・羊1で住む場所を問わない増員。 | — | [?p=3130](https://agricola.cloudfree.jp/?p=3130) |
| B128 | 配管工 | 表現不足 | 意味（レンガ2か石2の軽減＋改築）は一致。品物名 clay_or_stone が語彙外。 | — | [?p=3129](https://agricola.cloudfree.jp/?p=3129) |
| B129 | 隣り合わせた人 | 一致 | ラウンド13のカードのスペースを、左右隣の人物がいても使える。 | — | [?p=3128](https://agricola.cloudfree.jp/?p=3128) |
| B130 | 汎農 | 一致 | 穀物活用/柵の建設の後、飯1で同じ人物を空いている側へ（各手番1回）。 | — | [?p=3127](https://agricola.cloudfree.jp/?p=3127) |
| B131 | 装備品揃え | 一致 | 木材累積スペースの直後に小進歩1枚。 | — | [?p=3126](https://agricola.cloudfree.jp/?p=3126) |
| B132 | 地主 | 一致 | 農場スペースを使い切っていれば、収穫した野菜1につき1点。 | — | [?p=3125](https://agricola.cloudfree.jp/?p=3125) |
| B133 | 村の農婦 | 一致 | 大進歩・小進歩・職業の最小数ぶんの野菜。 | — | [?p=3124](https://agricola.cloudfree.jp/?p=3124) |
| B134 | 名家録の編纂者 | 一致 | 石の家への改築がラウンド11以前/12/13で飯3/2/1と1/2/3点。 | — | [?p=3123](https://agricola.cloudfree.jp/?p=3123) |
| B135 | 栄養士 | 表現不足 | 家畜1＋小麦1＋野菜1→飯5と2点は一致。good=animal_of_owned_type が語彙外。 | — | [?p=3122](https://agricola.cloudfree.jp/?p=3122) |
| B136 | 執事 | 表現不足 | 木材表が完全一致表（残り5などで何も得られない）で、部屋数最多の3点も語彙外 kind。 | — | [?p=3121](https://agricola.cloudfree.jp/?p=3121) |
| B137 | 卸売業者 | 一致 | カード上に4品、ラウンド8〜11のカード使用で対応品を取得。 | — | [?p=3120](https://agricola.cloudfree.jp/?p=3120) |
| B138 | 森の守護者 | 一致 | 木2と、他者が木材5以上を取る前に飯1を所有者へ。 | — | [?p=3119](https://agricola.cloudfree.jp/?p=3119) |
| B139 | 森林科学者 | 一致 | 帰宅フェイズに盤上の木が尽きていれば飯1、ラウンド5以降は飯2。 | — | [?p=3118](https://agricola.cloudfree.jp/?p=3118) |
| B140 | 農場労働者 | 表現不足 | 労働フェイズ末に飯2は一致。条件の述語 farmyard_goods_placed が語彙外。 | — | [?p=3117](https://agricola.cloudfree.jp/?p=3117) |
| B141 | 畑の世話人 | 一致 | レンガ0/1/3→小麦1/2/3と、畑1として扱う。 | — | [?p=3116](https://agricola.cloudfree.jp/?p=3116) |
| B143 | 粘土守 | 一致 | 他者の窪地でレンガ1、3人/4人では追加でレンガ1/飯1。 | — | [?p=3114](https://agricola.cloudfree.jp/?p=3114) |
| B145 | 柴結び | 表現不足 | 葦1/2を木1で代用は一致。kind substitute_reed_with_wood が語彙外（substitute_cost で表せる）。 | — | [?p=3112](https://agricola.cloudfree.jp/?p=3112) |
| B146 | イリュージョニスト | 表現不足 | 手札1枚を捨てて累積品1つ追加は一致。good=card が語彙外。 | — | [?p=3111](https://agricola.cloudfree.jp/?p=3111) |
| B147 | 緑衣の狩人 | 表現不足 | 小麦1→猪1は一致。good=boar が語彙外（wild_boar が正）。 | 修正 | [?p=3110](https://agricola.cloudfree.jp/?p=3110) |
| B148 | ペット仲買人 | 一致 | 羊1と、職業1枚につき羊1の収容。 | — | [?p=3109](https://agricola.cloudfree.jp/?p=3109) |
| B149 | 屋外飼育人 | 食い違い | 文献は厩3軒をストックから取り除き、農場2マスを柵で囲んで牧場1つ（木2）。仕様は牧場ではなく厩を建てている。 | 修正 | [?p=3108](https://agricola.cloudfree.jp/?p=3108) |
| B150 | 大規模農家 | 表現不足 | 農場拡充/大進歩の後、飯1で同じ人物を空いている側へは一致。good=action_space_use が語彙外。 | — | [?p=3107](https://agricola.cloudfree.jp/?p=3107) |
| B151 | 小百姓 | 一致 | 石1と、ちょうど2部屋の木の家の間は集会所以外を未占有扱い。 | — | [?p=3106](https://agricola.cloudfree.jp/?p=3106) |
| B152 | 駆け出し画家 | 表現不足 | 日雇いの後、飯1で空いている小劇場か授業へは一致。good=action_space_use が語彙外。 | — | [?p=3105](https://agricola.cloudfree.jp/?p=3105) |
| B153 | 寮長 | 表現不足 | 大進歩点の合計（最小を2倍）と5/7/9/11の閾値は一致。kind が語彙外。 | — | [?p=3104](https://agricola.cloudfree.jp/?p=3104) |
| B154 | 羊持ち | 一致 | 羊7頭未満の前提と、7頭到達時に1度だけ3点と飯2。 | — | [?p=3103](https://agricola.cloudfree.jp/?p=3103) |
| B155 | Art Teacher(美術教師) | 表現不足 | 木1と葦1、職業コストを小劇場の食料で払える点は一致。trigger pay_occupation_cost が語彙外。 | — | [?p=3102](https://agricola.cloudfree.jp/?p=3102) |
| B156 | 倉庫番 | 食い違い | 品物名が brick（語彙外）。文献はレンガ1か小麦1。 | 修正 | [?p=3101](https://agricola.cloudfree.jp/?p=3101) |
| B157 | 塩作り | 食い違い | 文献は払った家畜の種類で羊3/猪5/牛7ラウンド先に飯1。仕様は rounds=[3,5,7] を並列の3スペースとして扱っている。 | 修正 | [?p=3100](https://agricola.cloudfree.jp/?p=3100) |
| B158 | 森林地区管理 | 表現不足 | 森林と小さな森の両方を使えば労働フェイズ末に飯5は一致。述語名が語彙外。 | — | [?p=3099](https://agricola.cloudfree.jp/?p=3099) |
| B159 | 農家の大将 | 表現不足 | 他者が畑に隣接して畑を置くと飯1、ラウンド14は小麦1は一致。trigger と kind が語彙外。 | — | [?p=3098](https://agricola.cloudfree.jp/?p=3098) |
| B160 | 飲み屋の亭主 | 表現不足 | 森林・粘土坑・葦原が全て占有で小麦1は一致。スペース名 reed_field が語彙外（reed_bank）。 | 修正 | [?p=3097](https://agricola.cloudfree.jp/?p=3097) |
| B161 | 軟弱者 | 表現不足 | 品物5以上の未使用累積スペースがあるのに使わなければ野菜1は一致。trigger が語彙外。 | — | [?p=3096](https://agricola.cloudfree.jp/?p=3096) |
| B162 | 伐採人 | 表現不足 | 木材2/3/4ちょうどで木1と飯1/0/1は一致。kind が語彙外。 | — | [?p=3095](https://agricola.cloudfree.jp/?p=3095) |
| B163 | 牧師 | 食い違い | 品物名が brick（語彙外）。文献はレンガ2。 | 修正 | [?p=3094](https://agricola.cloudfree.jp/?p=3094) |
| B164 | 羊使い | 表現不足 | ラウンド+2/+5/+8/+10に羊1は一致。kind が語彙外。 | — | [?p=3093](https://agricola.cloudfree.jp/?p=3093) |
| B165 | 獲物調達人 | 表現不足 | 小麦1/3/4→猪1/2/3は一致。ただし「別々の畑から捨てる」条件が未表現。 | — | [?p=3092](https://agricola.cloudfree.jp/?p=3092) |
| B166 | 牛飼いの女 | 表現不足 | 小麦の種で飯1→牛1は一致。単一選択肢を choice で包んでいるだけで語彙外の形。 | — | [?p=3091](https://agricola.cloudfree.jp/?p=3091) |
| B167 | 厩舎長 | 一致 | 飯2で羊1猪1牛1、3頭すべて収容できる場合のみ。 | — | [?p=3090](https://agricola.cloudfree.jp/?p=3090) |
| B168 | 牧畜の達人 | 表現不足 | 改築で飯2と、厩のある牧場ごとに1頭は一致。kind が語彙外。 | — | [?p=3089](https://agricola.cloudfree.jp/?p=3089) |

## 第3章 修正した22枚

| ID | カード | 修正内容 |
|---|---|---|
| B015 | 工作台 | 家具製作所の割引（葦2→木1）の混入を削除 |
| B018 | 草地鍬 | 前提に「職業2」と「建設資材1以上」を追加 |
| B019 | 突き鋤 | 前提に「職業1以上」を追加 |
| B021 | 干し草小屋 | 入庫の条件を木材から小麦に |
| B029 | 料理教室 | 得点の付与を `gain_points` に |
| B031 | 製陶所の敷地 | 得点の付与を `gain_points` に、条件（隣接する未使用農場スペース2以上）を追加 |
| B032 | 煮沸釜 | 交換の3段階を構造化 |
| B034 | 特別な餌 | 家畜1頭につき1点に |
| B035 | 装蹄刀 | 発動時点を頭数到達時に（閾値は人数別のため保留） |
| B038 | 建設予定地 | 得点付与を削除し、未使用スペースの制約として表現 |
| B039 | 機織り機 | 羊の閾値表（1/4/7→飯1/2/3）と羊3頭ごとの得点に |
| B040 | 醸造所の池 | （下記グループ参照） |
| B067 | 手押し台車 | 占有スペース数ぶんの小麦に |
| B090 | 組合の鋤手 | 対象スペースを耕作から農地に |
| B110 | 路面舗装工 | ラウンド14は「飯1の代わりに野菜1」に（二重取得を解消） |
| B123 | 屋根付け | 飯1の二重支払いを解消 |
| B147 | 緑衣の狩人 | `boar` を `wild_boar` に |
| B149 | 屋外飼育人 | 厩ではなく牧場（2マス・木2）を建てる |
| B156 | 倉庫番 | 品物名 `brick` を `clay` に |
| B157 | 塩作り | 家畜種別ごとのラウンド数（羊3/猪5/牛7）に |
| B160 | 飲み屋の亭主 | スペース名 `reed_field` を `reed_bank` に |
| B163 | 牧師 | 品物名 `brick` を `clay` に |

### 3.1 語彙に寄せた修正（B015, B021, B029, B031, B034, B035, B038, B039, B147, B156, B160, B163）

- **B029, B031**: `gain_resource(good="bonus_points")` を `gain_points` に。B031 は条件（隣接する未使用農場スペース2以上）も追加。
- **B034**: 家畜の受け入れ時に「その回に受け入れた頭数」ぶんの得点に。
- **B035**: 得点2は一致。発動時点を「羊の頭数が人数別の閾値に達したとき」に直し、閾値は保留（Q-3）。
- **B038**: 文献は「得点なし・隣接する未使用スペースを他者が使うまで使えない」。得点付与を削除し制約として表現。
- **B039**: 羊1/4/7頭で飯1/2/3、羊3頭ごとに1点（端数切り捨て）に。
- **B147, B156, B160, B163**: 品物名・スペース名を語彙どおりに（`wild_boar`, `clay`, `reed_bank`）。

### 3.2 読みの食い違いを直した修正（B015, B018, B019, B021, B067, B090, B110, B123, B149, B157）

- **B110**: ラウンド14は「飯1の代わりに野菜1」。並列2効果で飯1と野菜1を両方得ていたのを置換に。
- **B123**: 飯1を `spend_resource` と `exchange` で二重に払っていたのを1回に。
- **B149**: 厩3軒を取り除いて**牧場**（2マス・木2）を建てる。仕様は厩を建てていた。
- **B090**: 対象スペースが耕作（`cultivation`）になっていたのを農地（`farmland`）に。
- **B157**: 羊3/猪5/牛7ラウンド先、を「3/5/7の3スペース」と読んでいたのを家畜種別の対応に。
- **B067**: 1ではなく「占有している累積スペース数」ぶんの小麦に。
- **B018, B019**: 前提の補完（職業2＋建設資材1以上／職業1以上）。

## 第4章 人間への質問チェックリスト

### Q-1 `post_play: left_neighbor_hand` の根拠（17枚）

- 対象: B022, B024, B036, B068, B069, B070, B072, B074, B075, B076, B078, B079, B080, B081, B082, B083, B084。
- 文献の要約: いずれも効果文に「このカードは自分の前に置かず、左隣のプレイヤーの手札に入れる」旨が無い。
  #402 は traveling が確認できる B008（露店）で「テキストに文言が入る」ことを校正し、同じ方法で
  D帯9枚の `left_neighbor_hand` を否定した。B帯も同じ基準では根拠が無い。
- 一方で、仕様側はこれらを traveling として扱っており、実装（`post_play`）に影響する。
- 決めたいこと: 文献に文言が無い17枚を `played_area` に戻すか、印刷面（カード画像）で確認して確定するか。
  **本PRでは推測を避けて変更していない。**
- 影響: 手札の移動と「誰の前で発動するか」が変わる。効果の量には影響しない。

### Q-2 費用が仕様0で文献に記載がある4枚（B043, B044, B046, B065）

- B043: 木2/レンガ2の選択。B044: 木1/レンガ1。B046: 木3/レンガ2。B065: 木2/レンガ2/石2。
- 仕様はいずれも費用0・`needs_resolution`。台帳の `cost_alternative` にも選択費用の記載がある。
- 決めたいこと: OR費用（材質の選択）を仕様の `cost` でどう表すか。語彙を決めれば4枚とも埋まる。
- B043 は「3/2ラウンドの対応」も文献に無く、別途確認が要る。

### Q-3 B035 の閾値（人数別）

- 文献の要約: 羊の頭数が人数別の閾値（1/2/3/4/5/6人で9/8/7/6/5/5頭）以上で2点。
- 仕様: 閾値を `raw_text` で保留。人数別の表を語彙（`threshold_table` の人数軸）で表せるか決めたい。

### Q-4 B036 ガラスビン（費用）

- 文献の要約: 家族1人につき［レンガ1＋飯1］を払って出す。台帳の費用は `none`。
- 決めたいこと: 費用が家族数に比例するカードの表し方（台帳側の持ち方も含む）。

### Q-5 B004・B065 など「公開状態に無い情報」を使う効果

- B004: 累積スペース上の自分の人物数（Issue #347 の保留）。文献との食い違いではない。
- 決めたいこと: 保留の解除条件（公開状態に人数を持たせるか）。

## 第5章 検証

- `scripts/validate_bga_card_specs.py` → exit 0（errors 空）。
- 費用・印刷点は台帳と全147枚で突合し、不一致0。
- 変更22ファイルはスキーマ通過。`src/`・テスト・台帳・語彙文書は無変更。

## 第6章 参照文献

A帯と同じ（`docs/aband-spec-check.md` 第6章）。各カードの個別ページURLは第2章の「文献」列。

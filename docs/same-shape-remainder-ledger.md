# 同型残件の台帳（2026-11-20 静的レビュー #386/#387 トリアージ）

- 起点: main（本 branch は `docs/same-shape-remainder-ledger`。受付作成時 HEAD=d267be8、本台帳の実測は rebase 後の `7e6057a`＝当時 origin/main で実施）
- 目的: 読み取り専用静的レビュー（2026-11-20）の指摘3件を、`data/bga-card-specs/` の実測で裏付けた一覧として台帳化する。**本 PR は docs 追加のみ。仕様 JSON・src/ は一切修正しない。**
- 前提の出典: Muse 側静的レビューの要点は依頼本文経由のみ（正本ファイルは受付環境に不在）。B069 の解決先は PR #386 の `docs/spec-data-fixes.md`（未マージ、読み取り専用）。
- 本 PR 後の修正は、各カードの通常フロー（境界ケースの再現テスト → 指摘の再現確認 → 修正 → PR）に委ねる。

## 1. Effect details pending（静的レビュー「17枚（例 C037・C043）」）

- **実測: 20枚**（C031〜C050）。静的レビューの「17枚」と不一致。**実測値を正として記録し、両方を残す**（依頼本文の指示どおり、受付前提を正として扱わない）。
- 例示の C037・C043 は両方とも実測該当。差分の3枚は静的レビュー時点の切断差と考えられるが、根拠は不明。
- 経緯: `7fb1ea4 feat(m10): Cデッキ031-050 仕様JSON+テスト契約の次バッチ（20枚）` の1コミットで20枚一括投入。全20枚が `effects[0].notes = "Effect details pending BGA verification"`。
- 共通属性: 全20枚 kind=minor_improvement、review.status=inventory_only、implementation_status=not_started。
- **正しい値: 全20枚 不明**（BGA verification pending。効果文の文献値は未確定。）

| ID | カード | kind | review.status / impl | 正しい値 |
|---|---|---|---|---|
| C031 | 書斎（Writing Chamber） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C032 | 張出便所（Abort Oriel） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C033 | 緑化計画（Greening Plan） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C034 | ナピアグラス培養機（Elephantgrass Plant） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C035 | ランタンの家（Lantern House） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C036 | 粘土地帯（Clay Deposit） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C037 | 人工丘の集落（Dwelling Mound） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C038 | キリスト教（Christianity） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C039 | アトリエボート（Studio Boat） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C040 | ズタ袋（Canvas Sack） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C041 | 農産店（Farm Store） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C042 | 激しい空腹（Ravenous Hunger） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C043 | 農舎（Farm Building） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C044 | 鶏小屋（Chicken Coop） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C045 | シチュー（Stew） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C046 | マンドリン（Mandoline） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C047 | 園芸用熊手（Garden Claw） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C048 | 農園（Farmstead） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C049 | ビール屋台（Beer Stall） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |
| C050 | 厩の庭（Stable Yard） | minor_improvement | inventory_only / not_started | 不明（BGA verification pending） |

- 修正担当の提案: C帯小進歩バッチ（C031-050）の仕様確認担当。未修正。

## 2. B069 同型 destination 誤り

### 2.1 B069 実在確認

- B069（陶工市 / Potters Market）の仕様 JSON に `post_play.destination: "left_neighbor_hand"` が**実在する**（受付は未 grep。worker が本台帳で確認）。
- **正しい値: `played_area`**（根拠: PR #386 `docs/spec-data-fixes.md` 第6章「B069 destination 修正 | left_neighbor_hand → played_area | 修正済み」＝静的レビュー指摘「left_neighbor_hand を残さない」への対応）。ただし **PR #386 は未マージ**のため main には未反映。→ B069 自体は別 PR で修正先行中。本 PR は台帳にとどめ、データは触らない。
- 参考: PR #386 では B069 の destination 修正のほかに `on_play` → `any_time` のトリガー修正も行われている（第2章）。

### 2.2 静的レビュー指名の同型候補（B068/B070/D039/D040）

4枚すべて実測で `left_neighbor_hand`（kind=minor_improvement、`post_play.notes` なし。B069 と同型）。

| ID | カード | 型 | 現状 | 正しい値 | 状態 |
|---|---|---|---|---|---|
| B068 | マメ畑（Beanfield） | 同名候補（静的レビュー指名） | left_neighbor_hand（notes なし） | 不明 | 未修正 |
| B070 | 新規購入（New Purchase） | 同名候補（静的レビュー指名） | left_neighbor_hand（notes なし） | 不明 | 未修正 |
| D039 | トリュフスライサー（Truffle Slicer） | 同名候補（静的レビュー指名） | left_neighbor_hand（notes なし） | 不明 | 未修正 |
| D040 | 汚水溜め（Cesspit） | 同名候補（静的レビュー指名） | left_neighbor_hand（notes なし） | 不明 | 未修正 |

- **正しい値: 4枚とも不明**（repo 文書に判定なし。docs/*.md に `回る進歩`/`left_neighbor` の記述は0件）。

### 2.3 実測全体像（`post_play.destination`）

- `left_neighbor_hand`: **91枚**（全員 kind=minor_improvement）。内訳:
  - notes で通行メカニクスを主張する **15枚**: A005 B022 B024 B036 D001 D008 D009 D022 D070 D071 D072 D073 D076 D077 D080（「回る進歩: use once then pass to left neighbor」「Minor improvement passes to left neighbor」等）
  - notes なしの **76枚**（B069 を含む同型。B068/B070/D039/D040 はこの76に含まれる）
- 対照: minor_improvement 402枚中 `played_area` は311枚。
- **エンジン影響: 現状なし**。`src/` に `left_neighbor_hand` 参照は0件、`tests/` にも0件（grep 実測）。→ 仕様データ上の注記で、エンジン挙動には未実装・無影響。
- **正しい値の有無**: B069 = 確定（played_area、#386 で解決済み・未マージ）。notes なしの76枚（B068/B070/D039/D040 含む）= 不明。notes あり15枚 = notes が出典文言の主張であり、2016年版公式ルールでの正当性は未検証（repo 文書に判定なし）。
- **修正担当の提案**: B帯/D帯の仕様修正担当カード。未修正。修正時にデータ修正か注記削除かを各カードで判断。

notes なしの76枚一覧:

A001 A002 A003 A004 A006 A007 A008 A009 B001 B002 B003 B004<br>B005 B006 B007 B008 B009 B068 B069 B070 B072 B074 B075 B076<br>B078 B079 B080 B081 B082 B083 B084 C001 C002 C003 C004 C005<br>C006 C007 C008 C009 D002 D003 D004 D005 D006 D007 D039 D040<br>D041 D042 D044 D045 D046 D048 D049 D050 D051 D053 D055 D056<br>D058 D064 D067 D068 D069 E001 E002 E003 E004 E005 E006 E007<br>E008 E009 E076 E078

## 3. small_theater 残り（静的レビュー指名: D151・D143）

- **実測: small_theater 言及は4枚**（B155 D143 D151 E166）。静的レビュー指名の D151・D143 に加え B155・E166 も該当。
- 語彙の状況: エンジン（`src/cards.rs`）のスペース語彙は `traveling_players`（A155 テスト等）、`small_theater` は src に0件。main の仕様JSON で `traveling_players` を使うのは A155/A160/A163/C152。PR #386 第6章も「A150/A154/A158/A160/C152 は `traveling_players`。B155/E166 は `small_theater` を使っている。別 Issue」と記録し、同 PR 対象カードを `traveling_players` に揃えた（未マージ）。

| ID | カード | kind | review.status / impl | 出現箇所 | 正しい値 | 状態 |
|---|---|---|---|---|---|---|
| B155 | Art Teacher(美術教師)（Art Teacher） | 職業（occupation） | bga_checked / not_started | effects[1].steps `use_cumulative_space_food` `space: small_theater`（職業費用の累積食料払い） | 候補: traveling_players（未確認） | 未修正 |
| D143 | 木材裁断師（Tree Cutter） | 職業（occupation） | needs_resolution / not_started | effects[0].conditions `accumulation_has_at_least_3_of_non_wood_resource` の `params.resources` に small_theater を含む／review.notes にも言及 | 候補: traveling_players（未確認） | 未修正 |
| D151 | スピンドクター（Spin Doctor） | 職業（occupation） | needs_resolution / not_started | effects[0].conditions `action_space_name` `params: {name: small_theater}`、steps に occupied スペースへの追加配置許可 | 候補: traveling_players（未確認） | 未修正 |
| E166 | 肉焼き名人（Roastmaster） | 職業（occupation） | needs_resolution / not_started | effects[0].conditions `worker_space_used` `spaces: [small_theater, fishing]`、交換効果。review notes に小劇場/Traveling Players の同一性未確認フラグ | 候補: traveling_players（未確認） | 未修正 |

- **正しい値: 4枚とも未確認**。候補は `traveling_players`（エンジン語彙）だが、小劇場（累積スペース）＝ `traveling_players` の同一性確認が必要。E166 の review notes が「JA ページは累積スペース『小劇場』（small_theater per B155）、英語一覧は Traveling Players。同一スペースか確認必要」と明示。D143 はさらに「Effect formalization needs resolution」（効果の形式化自体が未解決）。
- **修正担当の提案**: スペース語彙の単一正本化を扱う担当カード（未修正）。

## 4. 本 PR の範囲と実測再現方法

- 変更は `docs/same-shape-remainder-ledger.md` の追加のみ。`data/bga-card-specs/`・`src/`・他ドキュメントの diff は0（`git diff` で実測）。
- 実測（再現手順）: `data/bga-card-specs/*.json` を `json.load` し、以下で抽出:
  - (a) `"Effect details pending"` の部分文字列一致 → 20件（C031–C050）
  - (b) `post_play.destination` の値ごとに集計 → `left_neighbor_hand` 91件 / `played_area` 648件 / `unknown` 17件（D025, E137–E152）
  - (c) `"small_theater"` の部分文字列一致 → 4件（B155 D143 D151 E166）
- src 側 grep 実測: `left_neighbor_hand` 0件 / `small_theater` 0件 / `traveling_players` 2件（A155 コメントとテスト）。
- 台帳の各件は未修正。修正が必要なカードは本台帳を起点に、再現テストを先に書く通常フローへ。

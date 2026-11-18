# FREEBBUFF_REVIEW.md — DeepAI 第1〜5章（PR #311〜#315）遡及レビュー

- 対象リポジトリ: `cueikusuta/agricola-re-ai`
- 対象 main: `65f2c2b`（PR #318 マージ後）
- レビューブランチ: `research/freebuff-retro-review`
- レビュー日: 2026-11-15
- 方針: **報告のみ。コードの変更は行っていない。** 測定用の一時テストはすべて削除済み（`git status` はクリーン）。
- 正本: 2016年改訂版 Lookout 公式ルール / `docs/design.md` / `docs/roadmap-m7-onward.md`

## 0. 依頼文の前提との相違（AGENTS.md 1条）

依頼文は「第1章〜第7章 = PR #311〜#318」としているが、実際の対応は次のとおりで、1つずれている。

| PR | 実際の内容 | 本レビューの扱い |
|---|---|---|
| #311〜#315 | DeepAI 第1〜5章（カード実装） | **対象** |
| #316 | `feat(#308): 照合方針 2026-11-15 反映`（カード台帳・出典区分の文書とスクリプト） | DeepAI 章ではないため対象外 |
| #317 | DeepAI 第6章（A099/A101/A106/A117） | 既に独立レビュー済みのため対象外（依頼文どおり） |
| #318 | DeepAI 第7章（難易度4〜5の保留分類のみ、コード変更なし） | 対象外（依頼文どおり） |

したがって本レビューの対象は **#311〜#315 で実装された12枚**である。#316 は章番号の対応を1つずらしているため、次回以降の prompt では PR 番号の対応表を明示することを推奨する。

## 1. 対象カード（12枚）

| ID | 章 / PR | 種別 | 実装した効果 | dispatcher 経路テスト | cards.rs（legacy）テスト |
|---|---|---|---|---|---|
| C076 木材荷車 | 第1章 #311 | 小進歩 | `AfterWorkerGain` ×（森林/小森/林→木2）＋前提 職業3 | **なし** | あり（2件） |
| C106 じゃがいも収穫者 | 第1章 #311 | 職業 | `OnPlayGain` 食料3 ＋ `HarvestFieldGain(VegetablesHarvested)` | あり | あり（2件） |
| C111 小動物飼育員 | 第1章 #311 | 職業 | `StartRoundGainIfFoodAtLeastRound` 食料1 | あり | **なし** |
| C116 調度品職人 | 第2章 #312 | 職業 | `OnPlayGain` 木1 ＋ `AfterOccupationGainPerFoodCost` | あり | あり（1件・2番目の効果は対象外） |
| D100 君主 | 第3章 #313 | 職業 | `ScoreBonusRule::CategoriesAtMaxFour` | **なし** | あり（1件） |
| D105 彫刻家 | 第3章 #313 | 職業 | `AfterWorkerGain`（粘土坑→食料1、採石場西/東→小麦1） | **なし** | あり（1件） |
| D109 種まきの達人 | 第3章 #313 | 職業 | `OnPlayGain` 木1 ＋ `AfterWorkerGain`（種まき3スペース→食料2） | **なし** | あり（1件） |
| E117 パイプ喫煙家 | 第4章 #314 | 職業 | `HarvestFieldGain(HasWheatField)` 木2 | **なし** | あり（2件） |
| E107 土地調査士 | 第5章 #315 | 職業 | `HarvestFieldGain(FieldCountThresholds)` 2/4/6/7→1/2/3/4 | **なし** | あり（1件） |
| E111 世捨て人 | 第5章 #315 | 職業 | `StartRoundGainIfNoMinorPlayed` 食料1 ＋ `HarvestFieldGainIfNoMinorPlayed` 木1 | **なし** | あり（1件） |
| E113 教母 | 第5章 #315 | 職業 | `AfterWorkerGain`（増員3スペース→野菜1） | **なし** | あり（1件） |
| E118 薪切り | 第5章 #315 | 職業 | `AfterWorkerGain`（日雇い/漁/小劇場/資材市場→木1） | **なし** | あり（1件） |

「dispatcher 経路テスト」は `src/card_events.rs` のテストから実際に `dispatch_*` を呼んで当該カードを検証しているものだけを数えた。

## 2. 観点1: 経路一致（正本・dispatcher・legacy）

### 2.1 方法

同一の `Player` / `CardSet` 状態から、(a) `cards.rs` のヘルパ（`after_worker` / `start_round_rewards` / `harvest_field_rewards` / `end_game_bonus`）、(b) `card_events.rs` の dispatcher（`dispatch_after_worker` / `dispatch_start_round` / `dispatch_harvest_field` / `dispatch_play`）、(c) `card_events.rs` 内の legacy 複製（`harvest_field_rewards_legacy` ほか）を実行し、資材・食料・得点を比較した。測定は HEAD `65f2c2b`。

### 2.2 結果（一致したもの）

| カード | 実行 | legacy 実測 | dispatcher 実測 | 判定 |
|---|---|---|---|---|
| C076 | Forest 使用後 | 木2 | 木2 | 一致 |
| C106 | 畑フェイズ（野菜畑2） | 食料2 | 食料2 | 一致 |
| D105 | 粘土坑＋採石場西 | 食料1・小麦1 | 食料1・小麦1 | 一致 |
| D109 | 小麦の種＋耕作 | 食料4 | 食料4 | 一致 |
| E113 | 部屋あり増員 | 野菜1 | 野菜1 | 一致 |
| E118 | 日雇い | 木1 | 木1 | 一致 |
| E111 | ラウンド開始（小進歩なし／あり） | 食料1 / 0 | 食料1 / 0 | 一致 |
| E111 | 畑フェイズ（小進歩なし／あり） | 木1 / 0 | 木1 / 0 | 一致 |
| E117 | 畑フェイズ（小麦畑あり） | 木2 | 木2 | 一致 |
| E107 | 畑フェイズ（畑7） | 食料4 | 食料4 | 一致 |
| D100 | 終了時得点（4カテゴリ最大） | 4 | 4（legacy 複製） | 一致 |

### 2.3 発見 F-1（P0）: dispatcher が発火サイトを判別せず、収穫・ラウンド開始効果が全 After サイトで発火する

`resolve_phase_scan` は「`effect_phase(effect) == phase`」と「`effect_condition_holds`」だけで効果を選ぶ。`HarvestFieldGain` / `HarvestFieldGainIfNoMinorPlayed` / `StartRoundGain*` はすべて `TriggerPhase::After` で、条件判定は `true`（または状態のみ）を返すため、**After フェイズを使う4つの dispatcher すべて**（`dispatch_after_worker` / `dispatch_start_round` / `dispatch_after_clay_construction` / `dispatch_after_harvest_bake`）で発火する。

実測（一時テスト、HEAD `65f2c2b`。ワーカー配置トリガのみを直接呼び、コア効果は加算していない）:

| 呼び出し | カード | 期待 | 実測 |
|---|---|---|---|
| `dispatch_after_worker(DayLaborer)` | C106（野菜畑2） | 増加なし | **食料 +2** |
| `dispatch_after_worker(DayLaborer)` | E117（小麦畑1） | 増加なし | **木 +2** |
| `dispatch_after_worker(DayLaborer)` | E107（畑7） | 増加なし | **食料 +4** |
| `dispatch_after_worker(DayLaborer)` | E111（小進歩なし） | 増加なし | **食料 +1・木 +1**（両方の効果が発火） |
| `dispatch_after_worker(DayLaborer)` | C111（ラウンド4・食料4） | 増加なし | **食料 +1** |

エンジン全体でも再現する。C106 を出した状態で日雇いを実行すると、**食料 3 → 7**（日雇いの +2 のみなら 5）になる。

既存カードでも同じ症状が出る。DeepAI 以前から存在する B057（`StartRoundGain`）でも、`dispatch_after_worker(DayLaborer)` で食料 +1 を実測した。つまり **F-1 は DeepAI が持ち込んだ欠陥ではなく、After フェイズをサイト別に分けていない既存機構の欠陥**であり、第1〜5章のカードがこの欠陥の露出を大きく増やした、という位置づけになる。

- 原因箇所: `src/card_events.rs` の `resolve_phase_scan` / `effect_condition_holds`、`effect_phase`。
- 設計上の痕跡: `src/registry.rs` には `EventKind`（`WorkerPlaced` / `CardPlayed` / `RoundStart` / `Harvest` / `ClayConstruction` / `Bake`）と `EventIndex` があり、`StartRoundGainIfFoodAtLeastRound → RoundStart`、`HarvestFieldGainIfNoMinorPlayed → Harvest` の対応も登録されている。**しかし dispatcher はこの索引を参照していない。** サイト判定の設計は既にあり、配線だけが欠けている状態と読める。
- なぜ章のテストで検出できなかったか: 章の dispatcher テストは `dispatch_harvest_field` / `dispatch_start_round` を直接呼ぶため、「別サイトでは発火しないこと」を検証していない。
- 影響: 収穫・ラウンド開始のたびではなく、**ワーカーを置くたび・建築するたび・パン焼きのたび**に資材が増える。AI 評価・首位率・得点差の測定値はすべて無効になる。
- 推奨: `DispatchContext` に `EventKind` を持たせて `resolve_phase_scan` で絞り込む（または After をサイト別のフェイズに分割する）。あわせて「ワーカー配置では Harvest/RoundStart 効果が発火しない」回帰テストを追加する。状態遷移の中核に触れる修正のため、AGENTS.md 15条により**修正差分の独立再レビュー必須**。

### 2.4 発見 F-2（P2）: C111 は legacy 経路に実装がない

`cards::start_round_rewards` は `StartRoundGain` と `StartRoundGainIfNoMinorPlayed` を処理するが、`StartRoundGainIfFoodAtLeastRound`（C111）の分岐がない。実測（ラウンド5・食料5）:

- legacy `cards::start_round_rewards`: 食料 **5**（増加なし）
- dispatcher `dispatch_start_round`: 食料 **6**

その結果、C111 には legacy 側のテストが1件もなく、変異テストでも**dispatcher テストだけ**が検出した（§3）。3経路一致という品質条件を満たしていない。

### 2.5 発見 F-3（P3）: C116 の legacy `cards::play` は2番目の効果を発火しない（PR 本文に明記済み）

実測: `cards::play` で食料コスト2の職業を出しても木は 0 のまま。PR #312 の本文と `cards.rs` の doc コメントに明記されており、意図的な差分である。ただし「正本・dispatcher・legacy が同じ挙動」という条件からは外れるため、legacy ヘルパをテスト専用と位置づけるのか、前出カードの再スキャンを追加するのかを決めて記録する必要がある。

### 2.6 発見 F-4（P3）: D100 の「閾値の重複実装を避けた」は部分的

PR #313 は「カテゴリ値は `score.rs` の正本から取得し閾値の重複実装を避けた」としているが、実際には **カテゴリ名のリストと閾値 `4` が `cards.rs`（950行付近）と `card_events.rs`（2279行付近）に二重実装**されている。`score.rs` から取っているのは各カテゴリの点数だけである。値の食い違いは現状ないが、片方だけ変更すると静かに乖離する。

## 3. 観点2: dispatcher テストの有無と変異テスト

### 3.1 章別の dispatcher テスト

| 章 | PR | 実装カード | dispatcher テストがあるカード | 追加 `#[test]` 数（PR 主張） | 実測 |
|---|---|---|---|---|---|
| 第1章 | #311 | C076 / C106 / C111 | C106, C111 | 8 | 8 |
| 第2章 | #312 | C116 | C116 | 2 | 2 |
| 第3章 | #313 | D100 / D105 / D109 | **0枚** | 4 | 4 |
| 第4章 | #314 | E117 | **0枚** | 2 | 2 |
| 第5章 | #315 | E107 / E111 / E113 / E118 | **0枚** | 4 | 4 |

追加テスト数は PR 本文の増分（624→632→634→638→640→644→649）と一致した。ただし**第3〜5章の9枚には dispatcher 経路のテストが1件もない**。PR #317 の独立レビューで見つかった「dispatcher テストの欠落」と同じ作り方の問題が、第3〜5章にそのまま残っている。

### 3.2 変異テスト（定義値の改変 → `cargo test --lib` → 復元）

各カードの定義値を1つだけ変え、既存テストが検出できるかを確認した。変異はすべて `git checkout` で復元し、終了時に `git diff` が空であることを確認済み。

| 変異 | 検出したテスト | 検出 |
|---|---|---|
| C076 木2→3 | `cards::verified_band_c_tests::c076_gains_two_wood...`（legacy のみ） | 検出 |
| C106 食料1→2 | dispatcher テスト ＋ legacy テスト（2件） | 検出 |
| C111 食料1→2 | `card_events::tests::c111_round_start...`（dispatcher のみ） | 検出 |
| C116 木1→2 | `card_events::tests::c116_occupation_play...`（dispatcher のみ） | 検出 |
| D100 閾値4→3 | **なし**（`d100_bonus_point_per_category_at_max_four` は通ったまま） | **未検出** |
| D105 食料1→2 | `cards::verified_band_d_tests::d105_food...`（legacy のみ） | 検出 |
| D109 食料2→3 | `cards::verified_band_d_tests::d109_wood...`（legacy のみ） | 検出 |
| E107 段階 [1,2,3,4]→[1,2,4,4] | `cards::verified_band_e_tests::e107_food...`（legacy のみ） | 検出 |
| E111 ラウンド食料1→2 | `cards::verified_band_e_tests::e111_gains...`（legacy のみ） | 検出 |
| E113 野菜1→2 | `cards::verified_band_e_tests::e113_vegetable...`（legacy のみ） | 検出 |
| E117 木2→3 | `cards::verified_band_e_tests::e117_wood...`（legacy のみ） | 検出 |
| E118 木1→2 | `cards::verified_band_e_tests::e118_wood...`（legacy のみ） | 検出 |

- 11/12 は検出されたが、**dispatcher 経路で守っているのは C106・C111・C116 の3枚だけ**。第3〜5章のカードは legacy テストのみで守られている。
- D100 は `--lib` の範囲で変異が検出できなかった。テストデータ（畑5＝4点、牧場4、厩4、小麦8＝4点）に「3点のカテゴリ」が含まれないため、閾値 4→3 の差が現れない。**3点のカテゴリを含むケースの追加**を推奨する。
- F-1（発火サイトの混線）は、どの変異テストでも検出できない種類の欠陥である（定義値ではなく dispatcher の選択ロジックの問題）。

## 4. 観点3: PR 主張の再測

| 主張 | 再測結果 |
|---|---|
| 追加テスト数 8/2/4/2/4/5（#311〜#315） | 各 PR のマージ差分の `#[test]` 追加行数と一致（8/2/4/2/4）。#317 も5で一致 |
| A/B 2人用プリセット 35枚の `content_hash` 不変 | **一致**。分岐前 `a5faf2d` と HEAD `65f2c2b` の双方で `35枚 / e62c95c76bfdb96bd3b82a2affd3bbb86f8a30e4e243d46b65475e287291eccd` |
| v5 リプレイ用フィクスチャ不変 | `tests/fixtures` に差分なし（`git diff a5faf2d 65f2c2b -- tests/fixtures` が空） |
| `verified_basic_card_set` 自体の変更なし | 差分は新規関数と新規テストのみ。プリセット関数本体に変更なし |
| band カードがプリセットに入らない | 各章の `band_*_cards_are_absent_from_the_two_player_preset` テストで確認済み（実行で緑） |

## 5. 指摘一覧

| ID | 重大度 | 内容 | 影響 | 推奨対応 |
|---|---|---|---|---|
| F-1 | **P0** | dispatcher が発火サイトを判別せず、収穫・ラウンド開始効果が全 After サイトで発火（C106/E107/E111/C111/E117 が該当。既存 B057 でも再現） | 資材・食料が過剰に入り、AI 評価・得点・首位率の測定が無効化される | `EventKind` によるゲート導入＋サイト横断の回帰テスト。中核修正のため独立再レビュー必須 |
| F-5 | **P1** | 第3〜5章の9枚に dispatcher 経路テストがない。D100 の閾値変異も未検出 | #317 と同じ種類の経路乖離が今後も検出できない | 各カードの `dispatch_after_worker` / `dispatch_start_round` / `dispatch_harvest_field` テストと、D100 の3点ケースを追加 |
| F-2 | P2 | C111 が legacy `cards::start_round_rewards` 未対応（legacy 5 vs dispatcher 6） | legacy 経路の単体テストが存在せず、3経路一致を満たさない | legacy 分岐＋legacy テストを追加（小規模・自己レビュー可） |
| F-3 | P3 | C116 の legacy `cards::play` は2番目効果を発火しない（PR 明記済み） | テスト専用ヘルパの仕様が暗黙 | legacy をテスト専用と明記するか、再スキャンを追加するかを決定して記録 |
| F-4 | P3 | D100 のカテゴリ名リストと閾値 `4` が2箇所に重複 | 片方だけ変更すると静かに乖離 | `score.rs` に「最大4点到達カテゴリ数」を返す関数を置き、両経路から呼ぶ |

## 6. 範囲外・未実施

- **公式ルール文面との突き合わせは限定的**。各カードの効果解釈（D105 の「累積スペース」の範囲、E118 の対象4スペース、E113 の増員3スペースなど）は PR 本文の wiki 要約を前提とし、原文・画像の再確認はしていない。ルール疑義は本レビューでは判定せず、Issue 分離の対象とする。
- PR #317（第6章）と #321（第8章）、#309 / #310 は依頼どおり触れていない。
- Web UI・Playwright・ブラウザ操作は実施していない。
- 1,000試合規模のシミュレーションは実行していない（F-1 の影響で、現状の数値は参考にならない）。
- `cargo fmt --check` / `cargo clippy --locked -- -D warnings` は PR 主張の範囲で再実行し、結果を §7 に記す。

## 7. 実行した検証

- `cargo test --locked`（HEAD `65f2c2b`、レビュー用一時テスト削除後）: **649 passed / 0 failed**（exit 0）。PR #317 が主張する 649 と一致。
- `cargo fmt --check`: 合格。
- `cargo clippy --locked -- -D warnings`: 警告なしで合格（`--all-targets` は実行していない。PR 本文が言及する既存 lint の再確認は範囲外）。
- 変異テスト12件（§3.2）: すべて復元済み。終了時に `git diff` が空であることを確認。
- 経路比較・サイト混線の実測: 一時テストで実施し、終了後に削除（作業ツリーはクリーン）。
- プリセット `content_hash` の基準値: `a5faf2d` を一時 worktree に展開して計測し、比較後に worktree を削除。

### 再測ログ（要約）

```
HEAD 65f2c2b
cargo test --locked        -> 649 passed / 0 failed (exit 0)
cargo fmt --check          -> ok
cargo clippy -- -D warnings-> ok
preset 2p cards=35 content_hash=e62c95c76bfdb96bd3b82a2affd3bbb86f8a30e4e243d46b65475e287291eccd
baseline a5faf2d           -> cards=35 content_hash=e62c95c76bfdb96bd3b82a2affd3bbb86f8a30e4e243d46b65475e287291eccd (identical)
```

# FREEBBUFF_REVIEW_2.md — DeepAI 実装の遡及レビュー 第2弾（#317 / #321 / #327 / #328）

- 対象リポジトリ: `cueikusuta/agricola-re-ai`
- レビュー基準: `origin/main` = `5fba360`（PR #322 マージ後）
- レビューブランチ: `research/freebuff-retro-review-2`
- レビュー日: 2026-11-16
- 正本: 2016年改訂版 Lookout 公式ルール / `docs/design.md` / `docs/roadmap-m7-onward.md`
- 方針: **報告のみ。プロダクトコードの変更は行っていない。** 測定用の一時テストは削除済み。
- 1 Issue = 1 目的 = 1 ブランチ（本PRは「遡及レビュー第2弾」の1目的）。低リスクの文書のみのため
  AGENTS.md 14条の「単純な文書修正」に該当し、独立レビューは省略する（根拠をここに記録）。

## 0. 対象と状態（開始時点）

| PR | ブランチ | HEAD | base | 状態 | 内容 |
|---|---|---|---|---|---|
| #317 | `feat/deepai-cards-c070-120-06` | `abd538e` | main（マージ済み） | **merged** | 第6章 A099 / A101 / A106 / A117 |
| #321 | `feat/deepai-cards-c070-120-08` | `d8e1722` | `c9d9452` | open | 第8章 C074 / C077 / C078 / C079 / C118 |
| #327 | `feat/deepai-bonus-point-store` | `308ce83` | `65f2c2b` | open | ボーナス点の持ち場（基盤） |
| #328 | `feat/deepai-bonus-cards-01` | `f89c1f5` | `f2a4bcc`（#327 の**1コミット目**） | open | E098 神童 / E101 はみだし者 |

開始時に確認した重要事項:

1. `#327` はレビュー指摘修正 `308ce83`（`validation.rs` の増分検証と `load_json` のレガシー受理）
   まで含む。**`#328` の基点はその1つ前の `f2a4bcc` で、この修正を含まない**（§4 G-3）。
2. `#321` は PR 本文に「独立レビュー＝approve with nits（P0/P1 なし）」とあるが、本レビューで
   **P1 相当（G-2）** を実測で確認した。
3. `main` には第1弾で報告した P0（F-1: dispatcher のサイト混線）の修正がまだ入っていない
   （修正PR #329 は open のまま）。#317 の A106 はこの欠陥の新しい露出先である（G-1）。

## 1. 効果文と典拠の照合（自作要約。原文・画像は転載しない）

典拠は BGA アグリコラカード情報wiki の各カード個別ページ（`https://agricola.cloudfree.jp/?p=<id>`）。
下表は**自分の言葉による要約**であり、カード原文そのものではない。

| ID | 実装した効果（要約） | 文献の要約 | 判定 |
|---|---|---|---|
| A099 放牧人 | 得点時、3マス以上の牧場1つにつき2点 | 同じ（`?p=3326`） | 一致 |
| A101 調理器具商 | 得点時、かまど系進歩1つにつき1点 | 同じ。窯（レンガ窯・石窯）は対象外、掘り土窯・東洋かまどは対象（`?p=3324`） | 一致（未実装の調理系小進歩を数えない制約は PR に記録済み） |
| A106 液肥散布人 | 畑フェイズで「最後の」小麦/野菜を取った畑1か所につき食料2/1 | 同じ（`?p=3319`） | 一致 |
| A117 木材調達人 | プレイ時、手元の進歩1つにつき木1 | 同じ（`?p=3308`） | 一致 |
| C077 粘土補給 | 以後3ラウンドの開始時に粘土1 | 同じ（`?p=3012`） | 一致 |
| C078 ヒキガエル | 現在ラウンド+5/7/9/11/13 の開始時に葦1 | 同じ（`?p=3011`） | 一致 |
| C079 石車 | 未プレイの偶数ラウンドの開始時に石1、コスト木2、前提 職業2 | 同じ（`?p=3010`） | 一致 |
| C118 木材集め | 以後5ラウンドの開始時に木1 | 同じ（`?p=2971`） | 一致 |
| E098 神童 | 最初の職業なら、その時点の進歩1つにつき1点（以後の進歩は対象外） | 同じ（`?p=2655`） | 一致 |
| E101 はみだし者 | 未開始ステージ1つにつき1点、以後職業禁止 | 同じ（`?p=2652`） | 一致 |
| C074 私有林 | 未プレイの偶数ラウンドの開始時に木1、コスト飯2、前提 職業1 | 同じ（`?p=3015`。初回取得失敗後に再取得） | 一致 |

ステージ割り当て（`model::stage_of_round` = 1〜4 / 5〜7 / 8〜9 / 10〜11 / 12〜13 / 14）は
ラウンドカードの配布枚数 4/3/2/2/2/1 と一致し、E101 の「未開始ステージ数」＝`6 - 現ステージ` も
文献の要約と整合する。

## 2. 観点1: 経路一致（正本 `cards.rs` / dispatcher `card_events.rs` / legacy）

### 2.1 方法

同一の `Player` / `CardSet` 状態から (a) `cards.rs` のヘルパ、(b) `card_events.rs` の dispatcher、
(c) `card_events.rs::legacy` の複製を実行し、資材・食料・得点・台帳を比較した。実測は各PRの
HEAD を worktree に展開して行った（`#317` は `main` 上で確認）。

### 2.2 #317（A099 / A101 / A106 / A117）

- A099 / A101 は `cards.rs::end_game_bonus_for_definitions` と
  `card_events::legacy::end_game_bonus_for_definitions_legacy` に**同じ式が二重実装**されている。
  値は一致するが、legacy 側は**テストで守られていない**（§3 M2）。
- A106 の「最後の作物」判定は `cards.rs` / `card_events.rs` / legacy の**3箇所**に複製されている。
- A117 は canonical（`play` → `apply_on_play_effects`）と dispatcher で同じ数を得る。
  ただし **canonical はゾーン移動前に効果を解決する**ため、`played_cards` に自分が入らない。
  A117 は職業なので差は出ないが、同じ語彙を小歩進に使うと canonical だけ1つ少なく数える（G-6）。
- A106 の dispatcher は**発火サイトを判別していない**ため、`dispatch_after_worker` でも発火する（G-1）。

実測（食料の初期値 0、`dispatch_after_worker(Space::DayLaborer)` を直接呼ぶ）:

```
A106 をプレイ済み、小麦畑(remaining=1) と野菜畑(remaining=1) を持つ
-> dispatch_after_worker 後 food = 3（正しくは 0）
```

### 2.3 #321（C074 / C077 / C078 / C079 / C118）

- 予約は `CardEffect::OnPlayScheduleRoundGain` → `Game::delayed_goods` に登録され、
  `Game::start_round` が対象ラウンドの開始時に付与して削除する。実プレイ・保存復元・再演の
  一致は PR のテストどおり緑。
- **canonical `cards::play` / `cards::apply_on_play_effects` は予約を実装していない**
  （`CardEffect::OnPlayScheduleRoundGain => {}`）。PR は「C116 と同じく本番経路のみ」と記録して
  いるが、結果として「正本経路では無効果」という3経路不一致になる（G-7）。
  `evaluation.rs` の projection も同じ経路を通るため、AI の仮想評価では予約が見えない。
- 予約台帳の公開ミラー `Observation::delayed_goods_schedule` は作られているが、
  **それを `Game` に戻す経路が無い**（§4 G-2）。

### 2.4 #327 / #328（ボーナス点）

- #327 は効果語彙を追加しないため経路比較の対象外。得点経路は
  `Game::standings()` に統一され、`score_with_cards` / `standings_with_cards` の直接呼び出しは
  `score.rs` 内部と `tests/m6_endgame_score_replay.rs`（Player 単体）だけになった（確認済み）。
- #328 は **3経路一致テストを持つ**（`equivalence_play_time_bonus_points_across_all_three_paths`）。
  canonical / legacy / dispatcher の台帳・供給・プレイ済み・印刷点を同一論理状態で比較しており、
  第1〜6章の「dispatcher テストが無い」問題に対する改善として評価できる。
- ただし #328 の基点が古く、#327 の増分検証修正を含まない（G-3）。

## 3. 観点2: テストの検出力（変異テスト）

定義値・分岐を1箇所だけ変え、既存テストが落ちるかを確認した。変異はすべて `git checkout` で
復元し、終了時に作業ツリーがクリーンであることを確認している。

| # | PR | 変異 | 検出したテスト | 検出 |
|---|---|---|---|---|
| M1 | #317 | A099 の牧場マス条件 3→2（`cards.rs`） | `cards::verified_band_a_tests::a099_two_points_per_large_pasture` | ○ |
| M2 | #317 | A099 の同じ条件（`card_events.rs` legacy 複製） | **なし**（`--lib` 全341件を実行しても緑） | **×** |
| M3 | #317 | `Improvement::is_cooking` に `ClayOven` を追加 | `a101_one_point_per_cooking_improvement` | ○ |
| M4 | #317 | A106 の小麦1畑あたり食料 2→3 | `a106_food_per_last_crop_harvested` | ○ |
| M5 | #317 | A106 の「最後」判定 `remaining==1`→`==2` | `a106_food_per_last_crop_harvested` | ○ |
| M6 | #317 | A117 の基礎進歩を数えない | `a117_wood_per_improvement_in_front` | ○ |
| P1 | #321 | C077 の `NextRounds(3)`→`(2)`（定義のみ） | `c074_c077..._definitions_match_the_printed_metadata` と `m5_card_actions` | ○ |
| P2 | #321 | `start_round` の回収ラウンドを1つ早める | `delayed_round_goods_are_collected_at_round_start_and_only_once` | ○ |
| P3 | #321 | `validate` が現在ラウンドの予約を許す | `delayed_goods_on_the_current_or_past_round_are_invalid` | ○ |
| P4 | #321 | `integrity_hash` が台帳を含まない | `c077_play_reserves_clay_and_the_ledger_survives_save_and_replay` | ○ |
| P5 | #321 | C077 の付与量 1→2 | 定義テストと `m5_card_actions` | ○ |
| Q1 | #327 | `integrity_hash` が台帳を含まない | `state_hash_ignores_the_store_but_integrity_hash_covers_it` ほか | ○ |
| Q2 | #327 | `validate` の「プレイ済みカード」検査を外す | `validate_rejects_unknown_player_unplayed_card_and_bad_amounts` | ○ |
| Q3 | #327 | 得点が台帳を無視 | `standings_count_stored_bonus_points_in_card_bonus` | ○ |
| Q4 | #327 | `validate_incremental` の台帳検査を外す | `tampered_bonus_points_on_unplayed_card_declines_incremental` | ○ |
| R1 | #328 | E101 の未開始ステージ `6-stage`→`5-stage` | `e101_one_point_per_unstarted_stage` | ○ |
| R2 | #328 | E098 が2枚目の職業でも支払う | `e098_pays_nothing_when_it_is_not_the_first_occupation` | ○ |
| R3 | #328 | E101 の職業禁止を `can_play` から外す | `e101_blocks_every_later_occupation` | ○ |

- **M2 だけが未検出**。A099 の「legacy 複製」は誰も見ていない（G-4）。A101 / A106 の legacy
  複製も同じ作りで、テストは canonical 側だけを叩いている。
- #317 の4枚には `card_events.rs` の**dispatcher 経路テストが1件も無い**
  （`card_events.rs` 内に `A099` / `A101` / `A106` / `A117` を参照するテストは0）。
  第1弾で報告した F-5 と同じ作りが A 帯にも残っている（G-4）。
- #321 / #327 / #328 は主要な不変条件（ラウンド式・回収・整合性・台帳の得点反映・職業禁止）が
  変異で守られている。

## 4. 指摘一覧（重要度は重大度。開発工程番号とは別）

| ID | 重大度 | 対象 | 内容 | 影響 | 推奨対応 |
|---|---|---|---|---|---|
| G-1 | **P0**（第1弾 F-1 の継続・新露出） | #317 | A106 の `HarvestFieldGain` が After フェイズ共通のため、ワーカー配置・建築・パン焼きでも発火する。実測 食料+3 | 資材・食料が過剰に入り、既存の AI 評価・首位率・得点差の測定値が無効 | #329 のサイト判定修正で A106 が塞がれることを回帰テストで確認する（§6） |
| G-2 | **P1** | #321 | `mcts::sample_game` が `Observation::delayed_goods_schedule` を `Game` に戻さない。実測: 観測に3件、サンプル後0件 | MCTS の決定化で予約が消え、C074/C077/C078/C079/C118 の価値とロールアウトの資材が実戦と食い違う（PR #265 の well_food_schedule と同じ欠陥の再発） | `sample_game` に `game.delayed_goods = obs.delayed_goods_schedule.clone();` を追加し、決定化後に台帳が一致するテストを追加 |
| G-3 | **P1** | #328 | 基点が `f2a4bcc`（#327 の1コミット目）で、`308ce83` の増分検証修正を含まない。実測: プレイ済みカードに9点を注入した状態で `validate_incremental` が **Pass** | 改竄された台帳が増分検証を素通りする（本番の検証経路） | #327 の最新 HEAD に rebase し、`validate_incremental`/`validate_free_action` の台帳検査が効くことを確認 |
| G-4 | **P1** | #317 | A帯4枚に dispatcher / legacy 経路テストが無く、A099 legacy 複製の変異が全 lib テストで未検出 | 経路乖離と重複実装の食い違いを検出できない | `dispatch_after_worker` / `dispatch_harvest_field` / legacy 得点の各テストと、カテゴリ3点ケースの追加 |
| G-5 | P2 | #317 | A106 の「最後の作物」判定が3箇所（`cards.rs` / dispatcher / legacy）に重複 | 片方だけ直すと静かに乖離 | 判定を1関数に集約し3経路から呼ぶ |
| G-6 | P2 | #317 | `OnPlayGainPerImprovementPlayed` は dispatcher が「プレイ済みエリア（自分を含む）」前提、canonical はゾーン移動前 | 同語彙を小進歩に使うと canonical だけ1つ少ない | #328 の `store_on_play_bonus_points(..., played_including_this)` と同じ形に canonical を揃える |
| G-7 | P2 | #321 | canonical `cards::play` / `apply_on_play_effects` が予約を実装しない（意図的と記録済み） | 正本経路と評価 projection でカードが無効果になる | 予約先（台帳）を引数で渡す形に canonical を拡張するか、legacy/canonical をテスト専用と明記して評価側の扱いを決める |
| G-8 | P2 | #327 | `Game::validate` は台帳を「実在プレイヤー・プレイ済みカード・1〜60点」しか見ない | プレイ済みカードへの加点（例 2→4）は full 検証を通る。守るのは保存時の `integrity_hash` だけ | カード定義から上限を導く（E098/E101 は最大5/14点）か、許容上限をカード別に持つ |
| G-9 | P3 | #327 / #328 | `PendingDecision::state_hash_before` は `game.state_hash()` を使い、これは台帳を含まない | 中断→再開の整合性検査では台帳の変化を検出しない（保存時の `integrity_hash` だけが守る） | 再開時に `integrity_hash` も照合するか、`state_hash_before` の doc に台帳非包含の理由を追記 |
| G-10 | P3 | #321 | `integrity_hash` の台帳追記にドメイン区切りが無い（#327 は `\0bonus_points\0` を挟む） | 固定長タプルの後置なので実害は無いが、先例と不揃い | #327 と同じ区切りを入れる |
| G-11 | P3 | #327 | 台帳が変わると `validate_incremental` は常に `Unsure`→full 検証 | 加点カード入りの対局で毎手 full 検証になり、大量対戦のコストが上がる | 現状は許容。収穫時加点カードを入れる前に差分検証を検討 |
| G-12 | P3 | #317 | A101 は未実装の調理系小進歩（掘り土窯・東洋かまど等）を数えない | 実装済みカードのみの制約。PR に記録済み | 該当カード実装時に `is_cooking` の語彙へ追加 |
| G-13 | P3（横断） | CI | `tests/m13_decision_budget.rs::detail_generation_is_charged_to_the_same_budget` が、3ブランチの `cargo test` 同時実行時に1回落ちた（単独実行では緑、50ms 予算＋100k シミュレーションの時間依存） | 負荷の高いランナーで偽陽性になり得る | 予算を広げるか、詳細生成の呼び出し回数を時間非依存で検証する |

## 5. PR 主張の再測

| 主張 | 再測 |
|---|---|
| #321「新規12件、`cargo test --locked` 661 passed」 | **661 passed / 0 failed**（一致） |
| #327「8件＋検証テスト、649 起点」 | **661 passed / 0 failed**（649 + 12。`tests/bonus_point_store.rs` 8件＋`pending.rs` 2件＋`validation.rs` 2件と整合） |
| #328「9件追加、666 passed」 | **666 passed / 0 failed**（一時プローブを除いた再実行で確認） |
| #317「649 passed（第6章 +5）」 | 第6章時点の 649 は第1弾で再測済み。本レビューの基準 `main`（`5fba360`、#317 と #322 を含む）は **685 passed / 0 failed** |
| A/B 2人用プリセット35枚の `content_hash` 不変 | 各ブランチのテストに固定値アサートがあり、全緑（`e62c95c7…`） |
| #321「独立レビュー approve with nits（P0/P1 なし）」 | **P1（G-2）を実測で確認**。レビュー時に決定化経路が対象外だったと推測される |

## 6. 範囲外・未実施

- **F-1 の修正PR #329 は本レビューの対象外**（指示された3件ではない）。ただし #317 の A106 は
  同じ欠陥の露出先なので、#329 のサイト判定が `HarvestFieldGain` を収穫サイトに限定すること、
  および「ワーカー配置では A106 が発火しない」回帰テストが入ることをマージ条件として推奨する。
- 1,000試合規模のシミュレーション、Playwright、Web UI は実行していない（G-1 の影響で数値も
  現時点では参考にならない）。
- 11枚の効果文照合はすべて完了した。C074 は初回のページ取得が一時失敗し、再取得で一致を確認した。
- 公式ルールブック原文との突き合わせは行っていない（典拠は BGA カード情報wiki の要約）。
- カード原文・画像・PDF は取得・保存していない（AGENTS.md 6条）。

## 7. 実行した検証（実コマンドと結果）

```
review base: origin/main 5fba360
#317 = merged (abd538e) / #321 = d8e1722 / #327 = 308ce83 / #328 = f89c1f5

cargo test --locked  (worktree: #321)  -> 661 passed / 0 failed
cargo test --locked  (worktree: #327)  -> 661 passed / 0 failed
cargo test --locked  (worktree: #328)  -> 666 passed / 0 failed
                                         （並列実行時に m13_decision_budget が1件失敗。
                                           単独実行では 11 passed。§G-13）
cargo test --locked  (main 5fba360)     -> 685 passed / 0 failed
cargo test --locked --lib (main)       -> 341 passed / 0 failed（M2 の変異を入れた状態）

経路・サイト混線の実測（一時テスト、削除済み）
  A106: dispatch_after_worker(DayLaborer) -> food 3（正しくは 0）
  #321: sample_game 後の delayed_goods = {}（観測は 3 件）
  #328: 台帳を 9 点に改竄 -> validate_incremental = Pass

変異テスト18件（§3）: 17件検出 / 1件未検出（M2）
すべて復元済み。レビュー終了時に `git status` は本PRの文書のみ。
```

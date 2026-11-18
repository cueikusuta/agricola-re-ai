# DEEPAI_RETRO_REVIEW_3.md — 遡及レビュー第3弾（監査修正10件のクローズ確認）

- 対象リポジトリ: `cueikusuta/agricola-re-ai`
- レビュー基準: `origin/main` = **`1a176a0`**（PR #335 マージ後。2026-11-17 時点で進行なしを確認）
- レビューブランチ: `research/deepai-retro-review-3`
- レビュー日: 2026-11-17
- 正本: 2016年改訂版 Lookout 公式ルール / `docs/design.md` / `docs/roadmap-m7-onward.md`
- 方針: **報告のみ。プロダクトコードの変更はしていない。** 変異テストと一時プローブはすべて
  元に戻し、終了時に `git status` が本PRの文書だけであることを確認した。
- 重要度 P0〜P3 は**重大度**であり、開発工程番号（M7 等）とは無関係（AGENTS.md 15条）。

## 0. 対象PRの開始時状態

`gh pr view` で確認した（2026-11-17、レビュー開始時点）。

| PR | 内容 | base | 状態 | CI | 本レビューの扱い |
|---|---|---|---|---|---|
| #337 | GrowWithMinor の家族増加 | main | **merged** | green | main 上で検証 |
| #340 | カードプレイ順序の統一 | `882d77d` | **open / CONFLICTING** | green（当時） | ブランチ `a46b002` で検証。main 未反映 |
| #341 | MCTS sample_game の決定化完全性 | `882d77d` | **merged** | green | main 上で検証 |
| #334 | 増分検証 check 6 の追加スペース | `7b49b22` | **merged** | green | main 上で検証 |
| #333 | 井戸の予約スケジュール復元 | `7b49b22` | **open / CONFLICTING** | green | #341 が同一内容を先にマージ済み。ブランチは古い |
| #329 | dispatcher の発火サイト区別 | `5e67d8e` | **merged** | green | main 上で検証 |
| #335 | OnPlayGainPerCount 章のレビュー対応 | `7b49b22` | **merged** | green | main 上で検証 |
| #352 | 一時停止・再開のセーブ／リプレイ | main `1a176a0` | **open / MERGEABLE** | **clippy fail** | ブランチ `8836351` で検証。main 未反映 |
| #353 | EndGameBonus 走査除外・E111・多重付与 | `dafdbb4` | **open / MERGEABLE** | **clippy fail** | ブランチ `933c2ef` で検証。main 未反映 |
| #332 | ボーナス点 第2章 E099 / E100 | `308ce83`（#327 の先端） | **open / CONFLICTING** | green（当時） | ブランチ `3849a04` で検証。#327/#328 も未マージ |

補足:

1. **#332 の base `feat/deepai-bonus-point-store` は main にマージされていない。** #327（基盤）と
   #328（E098/E101）も open のため、#332 単独では成立しない。PR 本文の「両方がマージされるまで
   マージしないでください」は正しいが、現時点でスタック全体が未着地である。
2. #352 / #353 は `mergeable=MERGEABLE` だが CI が赤い。失敗はどちらも**新規テストコードの
   clippy エラー**で、機能テスト自体はローカルで緑（§4）。
3. ローカルの `cargo clippy` は Windows のツールチェーン差で main でも失敗するため、
   #352 / #353 の CI 判定には GitHub Actions のログを使った（§1）。

## 1. 方法

- **main の修正**: `origin/main` から作った worktree で、対象の既存テストを実行し、
  修正行を1箇所だけ壊す変異を入れてテストが落ちるかを確認した。変異は `git checkout` で復元。
- **open PR の修正**: PR の head を detached worktree に展開し、PR 自身の回帰テストを実行。
  さらに #340 では、レビュー側で一時的な統合テストを追加して C116 の自己発動を実測した
  （実行後に削除。成果物には含めない）。
- **経路一致**: 正本（`cards.rs`）、dispatcher（`card_events.rs`）、legacy ミラー、
  `validate` / `validate_incremental`、`sample_game` のどこを同じ状態で通すかを PR ごとに確認した。
- **文献**: E099 / E100 / C098 / C099 / D099 / A100 について、日本語版 wiki
  （`https://agricola.cloudfree.jp/?p=<id>`）と英語版一覧
  （`https://agricola.cloudfree.jp/cardlist_en/`）を取得して照合した。原文・画像は保存せず、
  本稿には自分の言葉の要約だけを記す。
- **実行環境**: 各 worktree の `target` を使用。`cargo test --locked` の対象テストのみ。
  1,000試合の大量シミュレーションは本レビューの対象外（文書レビューのため）。

## 2. PR別の判定

### 2.1 PR #337 — GrowWithMinor の家族増加（merged）

| 元の指摘 | 判定 | 根拠 |
|---|---|---|
| **T-1 (P0)** `GrowWithMinor` が `Ok` を返すのに family / newborns が増えない | **閉じた** | main `src/engine.rs:1701` で `grow(p, true)?` を実行。`grow_with_minor_regression_tests` 12件が緑。変異（`grow` 行の削除）で **8/12 が赤**（§3 M1） |
| **T-2 (P0)** 部屋不足・family 5・カードセット無効・未知カードで Err、`validate_action` と一致、ワーカー未消費 | **閉じた** | 4つの Err テストと `grow_with_minor_err_validate_action_matches` が緑。`apply_with_plans_impl` は `next` クローン上で実行し、Err 時は `self` を更新しない（`src/engine.rs` の transactional apply） |
| **T-3 (P1)** 「家族増加 → 小進歩」の順序（B010 自身の +1 容量を同じアクションの部屋条件に使わない） | **実装は閉じたが、典拠が弱い（P2）** | テスト (c) は緑。ただし PR 本文は「2016年改訂版ルール」とだけ書き、ルールブックのページ・公式FAQを挙げていない。`docs/rule-decisions.md` にも追記がない。公式典拠を PR に残すべき |

補足（P3）: `GrowWithMinor` は `Detail` 専用で、正本 `cards::play` には対応する経路がない。
「3経路一致」の対象外であること自体は設計どおりだが、その旨を PR に明記するとよい。

**判定: T-1 / T-2 は閉じた。T-3 は実装済みだが典拠不足（P2）。**

### 2.2 PR #340 — カードプレイ順序の統一（open / CONFLICTING）

main には未反映（base `882d77d`、main はその後 #329 / #335 が進行）。したがって
**main の利用者から見た3つの指摘はすべて未クローズ**である。ブランチの修正自体は妥当だが、
マージ前に直すべき点が見つかった。

| 指摘 | main の状態 | ブランチの状態 | 判定 |
|---|---|---|---|
| **P1-1a (P1)** `MaximumOccupations(0)` の自己計数で合法プレイが Err | **未修正**。`execute_work` は `play_from_hand` → `dispatch_play` の順（例: `src/engine.rs:1541` の直後に `dispatch_play`）。Replace の前提判定が移動後の `played_cards` を見る | ブランチの `reproduction_max0_occ_self_count` と `p1_1_three_paths_agree_on_max0_occ` は緑 | **未クローズ（main）** |
| **P1-1b (P1)** 自分自身を数える小進歩（`OnPlayGainPerImprovementPlayed`） | **未修正**。engine は移動後に OnPlay を解決 | `reproduction_self_minor_wood_self_count` は緑 | **未クローズ（main）** |
| **P1-1c (P1)** C116 が dispatcher 専用で、正本・評価 projection に無い | **未修正**。main の C116 は `dispatch_play` の専用ループのみ（`src/card_events.rs:1505-1537`） | `p1_1_evaluation_projection_matches_engine_for_c116` は緑 | **未クローズ（main）** |

**新規 P1（ブランチが持ち込む回帰）: C116 の自己発動。**
#340 は C116 を汎用 After スキャンに移したが、スキャン対象の `played_cards` には
「いま出したカード自身」が含まれる。C116 を Lessons で出すとき、既に職業を1枚出していれば
参加費1食料が `play_food_paid` に入り、**C116 自身の After 効果が自分を対象に +1 木**する。

レビュー側の一時テスト（2人、C116 + ダミー職業/小進歩、既存職業1枚、食料1）で実測:

```
#340 ブランチ: C116 を Lessons でプレイ -> 木 2（OnPlay 1 + 自己発動 1）
main:          同じ操作           -> 木 1（自己発動しない）
```

main は `let owned = played.len().saturating_sub(1); // exclude the new card` で自分を除外して
いる（`src/card_events.rs:1516`）。#340 の新 `apply_after_play_effects` と
`dispatch_play_after` の両方がこの除外を失っている。**C116 の公式文言は「このカードの後に
職業を出したとき」なので、自分自身は対象外**。修正必須。

**新規 P1（マージ順序）: `dispatch_play_after` に `event_kind` がない。**
#340 は #329 より前の base のため、`PhaseTriggerData` に `event_kind` を持たない。
#329 マージ後の main に rebase すると、`dispatch_play_after` の `PhaseTriggerData` は
`event_kind` 未設定（`Default` = `WorkerPlaced`）になる。そのままだとカードプレイ時に
`AfterWorkerGain` が誤発火する。`EventKind::CardPlayed` を設定する必要がある。

**P3:** `apply_after_play_effects` は C116 だけを直接処理し、イベント予算・カード使用上限・
`ChooseTriggerOrder` を通さない。C116 に使用上限は無いので現害はないが、正本と dispatcher の
意味が完全には揃っていない。

**判定: 3指摘とも未クローズ（main 未反映）。ブランチは P1 の自己発動回帰と `event_kind` の
rebase 対応が必要。マージ不可。**

### 2.3 PR #341 — MCTS `sample_game` の決定化完全性（merged）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **P1-5** `hand_counts` が固定7で、標準外の手札を拒否する | **閉じた** | `sample_private_cards` は `saturating_sub(played)` と実枚数で構築し、末尾で `obs.hand_counts` と照合。`sample_game_survives_many_played_cards_and_the_policy_simulates` などが緑 |
| **P2-3** `card_uses` / `card_use_limits` が Observation から消える | **閉じた** | main `src/mcts.rs:284` で `card_uses` を復元、`card_use_limits` は定義から導出。`sample_game_reconstructs_card_use_limits` が緑。変異（両方空に）で3テスト赤（§3 M6） |
| **P1-4** 井戸の `well_food_schedule` ミラーが `start_round` で消えない | **閉じた** | `src/model.rs` の `start_round` が `well_food_schedule.remove(&round)` を無条件に実行。`sample_game_reconstructs_well_reservations` が緑 |
| **R1** `clippy --all-targets` の未使用 import/変数 | **閉じた** | PR の修正コミット後に CI 緑。main の CI も緑 |

補足（P3）: `sample_game` は `pending` と `event_budget` を復元しない。MCTS は保留決定の外側で
呼ばれる契約なので現害はないが、`pending` がある状態で呼ぶと「決定なし」の標本を作る。
呼び出し側の契約をコメントで固定するのが望ましい。

**判定: 指摘はすべて閉じた。**

### 2.4 PR #334 — 増分検証 check 6 の追加スペース（merged）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **C-1 (P0)** `validate_incremental` の check 6 が、未公開ラウンドスペース・無効な追加スペースを Pass する | **閉じた** | `src/validation.rs:137-161` に 6b-i（無効な追加スペース）、6b-ii（相互排他の複数使用）、6b-iii（`PublicState::available_spaces` に無い）を追加。`unrevealed_round_card_space_western_quarry_not_pass` と `additional_space_copse_disabled_not_pass` が緑 |
| **C-2 (P2, 新規)** 6b-ii（複数の追加スペース）に回帰テストがない | **未クローズ（テストギャップ）** | 6b-ii の条件を `false` にしても validation 30件がすべて緑（§3 M3）。PR 本文は「Multiple mutually-exclusive additional spaces」を挙げているが、テストは3件だけで 6b-ii を踏んでいない |

**判定: 本体は閉じた。6b-ii のテスト追加が必要（P2）。**

### 2.5 PR #333 — 井戸の予約スケジュールの復元（open / CONFLICTING）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **W-1 (P1)** `sample_game` が `well_food_schedule` を復元せず、井戸の予約が消える | **別PR（#341）で閉じた** | main の `src/mcts.rs:273` が `obs.well_food_schedule` から `well_food_schedule` と `delayed_well` の両方を復元。`sample_game_reconstructs_well_reservations` が緑 |
| #333 自身のマージ | **不要（古い）** | #333 は `7b49b22` 基点で main と conflict。同一内容は #341 が `search_from_parts` 方式で取り込んでいる。重複マージは避け、close してよい |

**判定: 指摘は #341 経由で閉じた。#333 自体は close 推奨。**

### 2.6 PR #329 — dispatcher の発火サイト区別（merged）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **F-1 / G-1 (P0)** `HarvestFieldGain` / `StartRoundGain` が `dispatch_after_worker` でも発火する | **閉じた** | `PhaseTriggerData.event_kind`（`src/card_events.rs:647`）を追加し、`effect_condition_holds` が Harvest / RoundStart / Clay / Bake / WorkerPlaced を判定（`:663-706`）。11枚の「誤サイトで発火しない」テストが緑 |
| **変異検出力** | **確認** | `HarvestFieldGain` を `true` にすると6件、`StartRoundGain` を `true` にすると3件が赤（§3 M4/M5）。テストは修正を守っている |
| **残る P1（別指摘）** `EndGameBonus` は main で依然 `true`（`src/card_events.rs:716`） | **未クローズ（#353 の担当）** | A099 / A101 のような EndGameBonus が After スキャンで発火し、`ChooseTriggerOrder` の不要な pause を起こす。#353 が `false` にする修正を出しているが open |

補足（P3）: `event_kind` は `#[serde(default)]` で `WorkerPlaced` になる。旧バージョンで作った
「Harvest で pause したセーブ」を新バージョンで開くと、`trigger_data.event_kind` が
`WorkerPlaced` になり、再開時の条件再評価で効果が落ちる可能性がある。跨バージョンの保留セーブを
保証するなら移行措置が要る。

**判定: F-1 は閉じた。EndGameBonus の同種欠陥は #353 待ち（P1）。**

### 2.7 PR #335 — OnPlayGainPerCount 章（merged）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **P0-1** C004/C005/D005/B006/B007 の cost / prerequisite が spec と不一致 | **閉じた** | main の定義（`src/cards.rs:2828` 以降）が spec と一致。`*_cost_food_1_pinned` と `spec_cost_prereq_parity_with_verified_cards` が緑。変異（C004 を food 2）で2件赤（§3 M7） |
| **P0-2** `verified_band_b_cards` の二重定義 | **閉じた** | 定義は1箇所（`src/cards.rs:3195`）。B004 はこの関数に含まれない |
| **P0-3** B004 の推計実装 | **閉じた（保留として）** | B004 は registry / pool に未登録。spec は `needs_resolution` / `effects: null` / `not_started`。Issue #347 に分離 |
| **P0-4** 章のテストが0件 | **閉じた** | `tests/ch03_onplay_per_count.rs` 879行、24テストが緑。canonical と dispatcher の両経路を含む |
| **P1-5** spec の implementation_status / test_case_ids の整合 | **一部閉じた** | 5枚は `tested`。ただし C005 と B007 の spec は `partial` のまま（下記） |
| **P2-7** カウントロジックの二重実装 | **閉じた** | `card_events` が `cards.rs` の `pub(crate)` ヘルパーを呼ぶ形に統一（コード確認） |
| **P2-8** C005 の `improvement` 語義 | **未クローズ（P2）** | `improvement_count_in_front`（プレイ済み小進歩＋大進歩）に固定したが、spec notes は「major improvement owned」。Issue #349 が open のまま。実装が先に意味を決めている |
| **新規 P2** B006 の「person in the owner's supply」 | **未クローズ（P2）** | 実装は `CountKind::FamilyMembers`（家族総数）。「supply にいる人」は配置済みワーカーを含まない可能性が高く、spec の notes とも一致しない。Issue #351 が open。テスト `b006_normal_family_3_stone_delta_3` は家族3→石3を固定しており、曖昧な解釈をテストで固定している |
| **新規 P2** B007 の最下行の構成 | **未クローズ（P2）** | `Joinery / Pottery / Basketry` の3種に固定。Issue #348 が open。公式ボード照合待ち |

**判定: 元の P0 はすべて閉じた。C005 / B006 / B007 の効果語義は未決定のまま実装・テストされて
おり、AGENTS.md 3条の観点で P2。**

### 2.8 PR #352 — 一時停止・再開のセーブ／リプレイ（open / CI赤）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **P0-3a** `ChooseTriggerOrder` の解決で `pending_resolution` が二重記録され、replay が壊れる | **コードは閉じた／E2E は判定不能** | ブランチは `apply_pending_resolution` の独自 push を削除し、`record_pending_resolution` 1回に統一。新テストでイベント数1を確認。**しかし同じテストの最後で `Game::load_json` が `prestate hash mismatch` で失敗することを assert している**（`src/card_events.rs:4130-4134`）。手動で A078 を played に置いたテストハーネスの限界で、「save/load 成功」という PR 本文の主張はテストで裏づけられていない |
| **P0-3b** 再開チェーンで2回目の pause が `Err("unexpected pause during resume")` になる | **コードは閉じた／テストなし（P2）** | 4箇所の pause サイトで新しい `PendingDecision` を作る実装を追加。ただし2回目の pause を実際に起こすテストは無い。4ブロックの重複は保守性も悪い（P3） |
| **P0-3c** `debug_assert` / `.expect()` を Err に | **閉じた** | `start_round` は pending を Err に、`harvest_fields` は `Result` を返し呼び出し元が `?`。`cargo test --lib` は緑 |
| **CI** | **未クローズ（P1, マージ不可）** | clippy が `points as i32`（`src/card_events.rs:4073`）で失敗（i32→i32 の不要キャスト）。CI ログで確認 |
| **新規 P3** `start_round` の pending チェックが round 更新・カード公開の後 | **P3** | Err 時は `next` クローンが捨てられる経路が多いが、直接呼び出しでは部分更新が残る。チェックを先頭に移すのが安全 |

**判定: 二重記録の修正は妥当。ただし save/load/replay の E2E が未証明で、CI 赤のため未クローズ。**

### 2.9 PR #353 — EndGameBonus・E111・多重付与（open / CI赤）

| 指摘 | 判定 | 根拠 |
|---|---|---|
| **P0-1a** EndGameBonus が実行中の After スキャンで発火する | **ブランチでは閉じた／main は未クローズ（P1）** | ブランチ `effect_condition_holds` が `EndGameBonus => false`。main は `true`（`src/card_events.rs:716`）。`endgamebonus_excluded_from_running_scan_no_pause` はブランチで緑 |
| **P0-1b** E111 の使用回数がイベント種別をまたいで共有される | **指摘の前提が誤り（P1）** | **E111 の `use_limit` は `None`**。`src/cards.rs` の全カード定義に `use_limit: Some(...)` は1枚もなく、実ゲームで `card_use_limits` は空。テストは `c.limits.insert("E111", 1)` の合成値で、実カードの挙動ではない。さらに「カード単位・期間単位」と宣言されている `use_limit` を `(card, event_kind)` に分割すると、同じラウンドに複数イベントで使えることになり、宣言された意味と逆になる。将来 `use_limit` 付きカードを入れる前に、効果単位かカード単位かを決定すべき |
| **P0-1c** post-work チェーンでの多重付与 | **テストが無効（P2）** | `post_work_chain_no_multi_grant` は E111 を使い、bake / clay / worker の3 dispatcher を呼ぶ。E111 の効果は RoundStart / Harvest のサイト条件付きなので、#329 のサイト判定で**そもそも一度も発火しない**。したがって修正前の main でもこのテストは緑になり、回帰テストとして機能しない |
| **CI** | **未クローズ（P1）** | 新規テストの `let mut` 2箇所が未使用可変（clippy `-D warnings`）。CI ログで確認 |
| **新規 P2** `integrity_hash` の互換性 | **P2** | `card_uses` のキーが `card_id` → `card_id:EventKind` に変わる。`card_uses` は `integrity_hash` に含まれるため、変更前に保存したカード有り対戦のセーブは `load_json` の整合性検査で拒否される。移行措置がない |
| **新規 P2** 正本経路との不一致 | **P2** | 使用回数の検査は dispatcher のみ。`cards::start_round_rewards` / `harvest_end_rewards` は `card_uses` を見ない。`use_limit` 付きカードを入れると3経路で結果が変わる |

**判定: EndGameBonus の修正は妥当だが、E111 の設計変更は前提が誤っており、main 未反映＋CI赤。
マージ不可。**

### 2.10 PR #332 — ボーナス点 第2章 E099 / E100（open / スタック）

| 主張 | 判定 | 根拠 |
|---|---|---|
| **E099 無関心な親**（各収穫の終わりに石の家なら1点） | **ブランチでは閉じた（P3 の運用注意あり）** | 日本語版・英語版の wiki が一致（「石の家」「各収穫の終わり」）。`dispatch_harvest_end` を繁殖後に呼び、台帳へ記帳。実エンジンの5テストが緑。条件は `House::Stone` のみ |
| **E100 博物館の館長**（各労働フェイズ開始時に…1点） | **判定不能（P1）— 日英で条件が矛盾** | 下記 §5 H-1。実装は「6種のうち1つ以上（OR）」だが、英語一覧は「6種すべてを各1以上（AND）」と読める。テスト `e100_pays_at_the_work_phase_start_when_a_listed_good_is_in_stock` は OR を固定しており、AND が正しい場合はテストごと誤り |
| **3経路一致** | **閉じた（OR 条件の範囲で）** | `equivalence_event_bonus_points_across_all_three_paths` が canonical / legacy / dispatcher を家3種×在庫4種で比較。site 相互漏れテストもある。ただし条件そのものが未確定なので、AND になれば書き直し |
| **spec 台帳** | **未クローズ（P2）** | `data/bga-card-specs/E099.json` と `E100.json` が存在しない。実装だけが先行し、spec の `review.status` / `test_case_ids` / `unresolved_fields` が無い。#335 で確立した「spec と実装の整合」から外れる |
| **スタック** | **未クローズ（P1, マージ不可）** | base #327 と #328 が未マージで、#332 は conflict。単独マージ不可 |
| **P3** `start_round` のエラー握りつぶし | **P3** | `src/model.rs` の `start_round` は `dispatch_start_round` の Result を `let _ = ... .unwrap_or_else(...)` で捨てる。E100 の記帳が予算切れ等で失敗してもラウンドは進む。現行カードでは起きにくい |
| **P3** round-start pause 時の E100 落ち | **P3** | `dispatch_start_round` は全員の phase scan の後に E100 を記帳する。Interrupt pause が起きると早期 return し、E100 が記帳されない。現行カードに該当なし |

**判定: E099 は実装として妥当。E100 は日英矛盾のため未クローズ（人間決定待ち）。#332 は
#327/#328 のスタックが着地するまでマージ不可。**

## 3. 変異テスト（main と #340）

| # | 対象 | 変異 | 結果 |
|---|---|---|---|
| M1 | #337 | `grow(p, true)?` を削除 | 12件中 **8件赤**（残り4件は Err 系で grow 前に落ちる） |
| M2 | #334 | 6b-iii の `available.contains` を無効化 | WesternQuarry テスト **赤**（1件） |
| M3 | #334 | 6b-ii の `additional_count > 1` を無効化 | validation 30件 **すべて緑（未検出）** |
| M4 | #329 | `HarvestFieldGain` のサイト判定を `true` | 6件 **赤**（B050/B061/A106/C106/E107/E117） |
| M5 | #329 | `StartRoundGain` のサイト判定を `true` | 3件 **赤**（B057/B107/B118。C111/E111 は別アームのため対象外） |
| M6 | #341 | `well_food_schedule` と `card_uses` の復元を空に | 3件 **赤**（roundtrip / well / card_use_limits） |
| M7 | #335 | C004 の cost を food 1 → 2 | 2件 **赤**（cost pin / spec parity） |
| M8 | #340 | C116 を Lessons（参加費1）でプレイ | ブランチ **木2で失敗**、main は木1で成功（自己発動は #340 の回帰） |

すべて `git checkout` で復元済み。M8 の一時テストは削除済み。

## 4. CI とローカル環境の注意

- #352 の CI 失敗は `error: casting to the same type is unnecessary (i32 -> i32)` 1件
  （`src/card_events.rs:4073`）。テスト自体は緑。
- #353 の CI 失敗は `error: variable does not need to be mutable` 2件
  （新規テスト内）。テスト自体は緑。
- ローカルの `cargo clippy --all-targets --all-features -- -D warnings` は **main でも失敗**する
  （Windows の clippy が `needless_range_loop` を追加検出するため）。したがってローカルの
  clippy 結果を PR の合否に使わず、CI ログを正とした。

## 5. 人間の決定が必要な項目（チェックリスト）

### H-1 (P1) E100 博物館の館長: 「1つ以上」は OR か AND か

- 日本語版 wiki（`?p=2653`）の要約: 「各労働フェイズのはじめに、**1つ以上の**木、レンガ、葦、石、
  小麦、野菜があなたのストックにあれば、ボーナス1点」。自然な読みは「6種のうち1種類以上」。
- 英語版一覧（`cardlist_en`）の要約: "at least 1 wood, 1 clay, 1 reed, 1 stone, 1 grain,
  **and** 1 vegetable in your supply" → 「6種すべてを各1以上」。
- 影響: OR なら毎ラウンドほぼ確実に1点（最大13点）で、カード評価7.50の割に強すぎる。
  AND なら6種を揃える終盤以外はほぼ発動せず、評価と整合する。
- 実装（#332）とテストは **OR** を固定している。AND が正しい場合は効果語彙
  `StartRoundBonusPointsIfStockHasOneOf` の名前・条件・テストをすべて変更する必要がある。
- 質問: 公式カード（英語版/ドイツ語版）の原文はどちらか。公式FAQに「ストックに6種すべて」と
  いう記述があるか。BGA実装の挙動はどちらか。
- 決定まで **#332 はマージしない**。

### H-2 (P2) B006 採石場見学: 「person in the owner's supply」は配置済みワーカーを含むか

- 現実装は家族総数（`FamilyMembers`）。「supply にいる人」は未配置の家族だけを指す可能性が高い。
- Issue #351 が open。公式カードの "supply" の定義（ルールブックの用語）を確認する必要がある。
- 決定まで B006 を本番プールへ入れない（現状は `verified_band_b_cards` に含まれる点も確認）。

### H-3 (P2) B007 賃金: 最下行の大進歩は Joinery / Pottery / Basketry の3種だけか

- Issue #348 が open。2016年改訂版の供給ボード最下行の構成を公式ボードで確認する。

### H-4 (P2) C005 改修: `improvement` はプレイ済み小進歩を含むか

- Issue #349 が open。spec notes は「major improvement owned」、実装は小進歩＋大進歩。
- 公式カードの "improvement" の定義を確認する。

### H-5 (P2) #337 の家族増加→小進歩の順序

- 実装は「家族増加が先」。公式ルールブックの「Family Growth with Room」の手順ページと、
  B010（Caravan）のような住居提供カードを同じアクションで使えるかに関する公式FAQを確認する。
- 現状は `docs/rule-decisions.md` に追記がないため、PR に典拠を残せていない。

## 6. 総括

- **main で閉じた**: #337 の P0 2件、#341 の3件、#334 の check 6、#329 の F-1、#335 の P0 4件、
  #333 の指摘（#341 経由）。
- **main で未クローズ**: #340 の P1-1a/b/c（PR が conflict）、#353 の EndGameBonus（open）、
  #352 の save/load E2E（open）、#332 の E100（日英矛盾）。
- **新規の重大指摘**:
  - P1: #340 の C116 自己発動（ブランチが main の正しい挙動を回帰させる）
  - P1: #340 の `dispatch_play_after` に `EventKind::CardPlayed` がない（rebase 後に誤発火）
  - P1: #352 の save/load 主張がテストで裏づけられていない
  - P1: #353 の `use_limit` をイベント種別に分割する設計（宣言された意味と逆・実カードは0枚）
  - P1: #332 の E100 日英矛盾
- **P2**: #334 の 6b-ii テスト欠落、#335 の B006/B007/C005 語義、#353 の無効な多重付与テスト、
  #353 の `integrity_hash` 互換性、#332 の spec 欠落、#337 の典拠不足。
- **P3**: #329 の旧セーブ `event_kind` 既定値、#341 の `pending`/`event_budget` 未復元、
  #352 の `start_round` 部分更新、#332 の `start_round` エラー握りつぶし。

マージ推奨:

1. **#352 / #353 は CI を緑にするまでマージしない。** #352 は clippy 1件、#353 は clippy 2件の
   修正が必要。加えて #353 は E111 の設計をやり直すこと。
2. **#340 は rebase と C116 自己発動の修正が必須。** 修正後、#329 の `EventKind` を
   `dispatch_play_after` に設定し、C116 の「自分を除く」テストを追加する。
3. **#332 は E100 の人間決定までマージしない。** E099 だけを分離するか、E100 を保留する。
4. **#333 は close（#341 で解決済み）。**
5. #334 の 6b-ii テスト追加は別の小PRでよい。

## 7. 限界

- open PR は head 時点の検証であり、rebase 後の挙動は保証しない。
- 1,000試合の強さ・首位率の測定はしていない（本レビューは文書と経路の確認が目的）。
- 文献は第三者の BGA 情報 wiki（日本語・英語）であり、Lookout 公式カードの目視照合ではない。
  E100 の日英矛盾はこの限界の典型で、公式カード/FAQ の確認を人間に依頼する。
- 変異テストは対象テストのみ。全スイートの網羅的な変異ではない。

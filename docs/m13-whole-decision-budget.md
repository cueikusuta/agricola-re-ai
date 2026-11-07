# M13 1手全体の時間予算（Issue #167 A）

更新: 2026-11-07。対象: `src/decision_budget.rs`（新規）、`src/policy.rs`、`src/mcts.rs`、`src/mcts_puct.rs`、`src/bin/paired_eval.rs`、`src/bin/decision_budget_probe.rs`（新規）、`scripts/paired_eval_stats.py`、`tests/m13_decision_budget.rs`（新規）、`tests/test_paired_eval_stats.py`。

**本ドキュメントは実装契約と実測の記録。強さの比較結果ではない。** 首位率 +5pp の判定は `docs/roadmap-m7-onward.md` 3節・3.2節が正本。

## 1. 解いた問題

`docs/m13-search-time-control.md` 5.1 節は、#164 が**探索だけ**を締切で止め、**1手全体**は止めていなかったことを記録している（200 ms 予算で決断全体 203 ms）。ロードマップ 3.3 の制限は「ワーカー配置・カード・資源・配置などの詳細決定を含む1回の意思決定」に対する 10 秒であり、探索だけを止めても条件を満たさない。

#167 A は次を実装する。

| 要件 | 実装 |
|---|---|
| 探索と詳細生成に共通の時間予算 | `DecisionBudget` が1つの締切を持ち、探索には「締切 − 詳細予備」を渡す |
| 期限切れでも合法な詳細行動へフォールバック | 予算切れ／詳細方針が拒否したら `SimpleDetails`（全スペースに合法手を返す）で確定 |
| 全決断時間・超過量・フォールバック回数を計測 | `DecisionOutcome` / `DecisionTimings` と `paired_eval` の行フィールド |
| 50 ms / 200 ms / 1 s / 10 s を2人・4人で検証 | `decision_budget_probe`（1手）と `paired_eval`（1試合）で実測（4節・5節） |
| `simulation-capped` の固定 seed 再演を維持 | 予算 `None` は時計を一切見ない。再演一致を実測とテストで確認 |

## 2. 設計

```text
決断開始             探索の締切                         決断の締切
   |---- search ----|------- detail reserve -----------|
   |<------------------ budget ------------------------>|
```

```rust
pub struct DecisionBudgetSpec { limit: Duration, reserve: Option<Duration> }
pub struct DecisionBudget     { limit, reserve, started: Instant }
pub struct BudgetedDecisions  { spec: DecisionBudgetSpec, scope: BudgetScope }
pub enum   BudgetScope        { NoSeats, AllSeats, FocusSeat(usize) }

pub fn resolve_decision(obs, legal, space_policy, detail_policy, rng, budget)
    -> Result<(Action, DecisionOutcome), String>
```

- **予算が適用される席（レビューP1で修正）**：`play_to_end_with_budget` は `BudgetedDecisions::applies_to(seat)` が真の席にだけ予算を渡す。評価は必ず `focus_only(spec, focus_seat)` を使い、**相手席は時計を見ない従来経路**（フォールバック置換なし・固定seedで再現）を維持する。`all_seats` は「試合全体を計時する」用途に限り、ペア比較では使わない。

- **予備（reserve）**：既定は予算の 1/8、上限 100 ms、下限 1 ms、かつ予算の半分を超えない。`--detail-reserve-ms` で明示できる。実測した詳細生成は最大 0.2 ms（4節・5節）なので、1/8 は十分な余裕であり、同時に探索へ予算の 87.5 % を残す。
- **探索の締切**：`DecisionBudget::search_deadline()` = 決断の締切 − 予備。`SpacePolicy::select_with_deadline` が MCTS/PUCT へ `SearchLimit::TimeBudget` として渡す。`simulation-capped` は締切を受け取らない。
- **詳細の締切**：`DetailPolicy::produce_with_deadline` に決断の締切を渡す。既定実装は従来どおり。
- **フォールバック連鎖**（予算ありのみ）：

  | 事象 | 挙動 | 記録 |
  |---|---|---|
  | 探索が `None` を返す | 合法手リストの先頭で確定 | `selection_fallback` |
  | 探索が予算を使い切った | 詳細方針を呼ばず `SimpleDetails` | `budget_exhausted_before_details` + `detail_fallback` |
  | 詳細方針が `None` を返す | `SimpleDetails` | `detail_fallback` + `detail_declined` |

  予算なし（`None`）、および**予算対象外の席**では従来の契約を維持し、方針が拒否したら**エラー**にする。評価の意味を黙って変えないため。
- **非公開情報**：入力は `Observation` と合法手リストのみ。新しい経路は内部状態・seed・相手手札を参照しない。

### 2.1 保証しないもの

- **OS 上の絶対的な時間保証はない。** 実行中の関数は中断できないため、締切を過ぎても停止しない。超過は `overshoot_ns` / `late_decision_count` として**記録**し、隠さない。
- 予備は実測に基づく経験値であり、証明ではない。非常に遅い詳細方針を挿せば超過する（テスト `detail_generation_is_charged_to_the_same_budget` が 60 ms の詳細方針で超過を記録することを固定している）。
- 報告する `observed_all_decisions_within_*` は**観測された決断の測定結果**であり、ハードウェア・負荷・OS を問う保証ではない。

## 3. 自己レビューで見つけて修正した問題

0. **（独立レビューP1）時間予算が相手席にも適用されていた。** 旧 `play_to_end_with_budget` は全席に同じ予算を渡していたため、相手席の詳細生成が壁時計で置換され得た。カードあり条件では相手の `CardChoiceDetails` が予算切れで `SimpleDetails` に落ち、**相手の手がマシン負荷で変わる**余地があった。`BudgetScope` を導入し、評価では focus 席のみに適用するよう修正（`BudgetedDecisions::focus_only`）。回帰テストは `only_the_focus_seat_is_budgeted`、`a_scope_with_no_seats_reproduces_the_unbudgeted_game_exactly`、`a_puct_opponent_is_never_affected_by_the_focus_budget`。修正後、カードあり2人戦 200 ms でも focus 最大 175.3 ms・遅延0・方針拒否0、`simulation-capped` の二重実行は意味フィールド完全一致を再確認。

1. **`SeatSelector` が探索の締切を無視していた（重要）。** `paired_eval` の focus 席は `SpacePolicy::select` を実装しており、`SearchSettings` のテンプレート（決断予算そのもの）から締切を作っていた。そのため予備が機能せず、**修正前は全 66 決断が予算超過・全 66 回で詳細方針がスキップ**されていた（2人戦 50 ms、seed 911）。`select_with_deadline` を実装して `search_deadline` を優先させた結果、同条件で超過 3/66・詳細スキップ 3/66 になった（4節）。回帰は `paired_eval` の `time_budgeted_honours_the_search_window_reserve` が固定する。
2. **「遅い」の定義が厳密すぎて常に偽になっていた。** 協調的締切はシミュレーション間でしか見られないため、0.1〜0.6 ms の超過が毎回出る。厳密カウントの隣に許容幅（予算の1 %、下限1 ms）付きカウントを追加し、両方を記録する。
3. **「フォールバック率 1.0」が異常に見えた。** 予算切れによる自動詳細と、方針が拒否した失敗を分離し、`detail_declined_count` を追加（実測は全条件 0）。
4. 予算フィールドを集約器が検証しないと、`simulation-capped` の行が「1手全体を予算化した」と主張できてしまう。`whole_decision_budget_enforced` / `detail_reserve_ms` / 詳細時間 / 超過・フォールバック件数を fail closed で検証し、`observed_*` は**生カウンタから再計算**する（`tests/test_paired_eval_stats.py` の 8 テスト）。

## 4. 実測A：1手（50 / 200 / 1,000 / 10,000 ms、2人・4人）

`./target/release/decision_budget_probe --players 2,4 --budgets 50,200,1000,10000`。固定初期局面、release ビルド、ローカル Windows。**同一コード経路（`resolve_decision`）で選択と詳細生成の両方を含む。**

| 人数 | 方針 | 予算 | 決断全体 | うち詳細 | 予算使用率 | 超過 | 遅延 | 合法 |
|---:|---|---:|---:|---:|---:|---:|---|---|
| 2 | mcts | 50 ms | 43.80 ms | 0.017 ms | 0.876 | 0 | なし | yes |
| 2 | mcts | 200 ms | 175.02 ms | 0.001 ms | 0.875 | 0 | なし | yes |
| 2 | mcts | 1,000 ms | 900.01 ms | 0.000 ms | 0.900 | 0 | なし | yes |
| 2 | mcts | 10,000 ms | 9,900.02 ms | 0.000 ms | 0.990 | 0 | なし | yes |
| 2 | puct | 50 ms | 43.76 ms | 0.001 ms | 0.875 | 0 | なし | yes |
| 2 | puct | 200 ms | 74.47 ms | 0.000 ms | 0.372 | 0 | なし | yes |
| 2 | puct | 1,000 ms | 68.69 ms | 0.001 ms | 0.069 | 0 | なし | yes |
| 2 | puct | 10,000 ms | 73.07 ms | 0.001 ms | 0.007 | 0 | なし | yes |
| 4 | mcts | 50 ms | 43.78 ms | 0.001 ms | 0.876 | 0 | なし | yes |
| 4 | mcts | 200 ms | 175.05 ms | 0.000 ms | 0.875 | 0 | なし | yes |
| 4 | mcts | 1,000 ms | 900.08 ms | 0.001 ms | 0.900 | 0 | なし | yes |
| 4 | mcts | 10,000 ms | 9,900.08 ms | 0.001 ms | 0.990 | 0 | なし | yes |
| 4 | puct | 50 ms | 43.96 ms | 0.001 ms | 0.879 | 0 | なし | yes |
| 4 | puct | 200 ms | 175.05 ms | 0.001 ms | 0.875 | 0 | なし | yes |
| 4 | puct | 1,000 ms | 208.39 ms | 0.001 ms | 0.208 | 0 | なし | yes |
| 4 | puct | 10,000 ms | 228.15 ms | 0.001 ms | 0.023 | 0 | なし | yes |

読み方：

- MCTS は予算の 87.5 %（= 1 − 予備 1/8）を使い切り、詳細生成は予備の中で終わる。1 s / 10 s では予備が上限 100 ms に張り付くため使用率が 0.900 / 0.990 になる。
- **PUCT の低い使用率は異常ではない。** `PuctConfig::default().simulations = 32` が安全上限として効いており、木が小さい局面では 32 回で止まる。予算は「上限」であって「必ず使い切る量」ではない。
- 1手の計測では超過は 0 だが、これは**この局面で**の観測であり保証ではない。次の節の対戦では実際に超過が出る。

## 5. 実測B：1試合（2人・4人、50 / 200 / 1,000 ms）

条件：カードなし、`candidate=mcts`（安全上限 100,000）、`baseline=eval`、他席 `greedy`、seed 911 の1ブロック（2人=4試合、4人=8試合）、`--search-mode time-budgeted`、release、ローカル Windows。**1ブロックのみなので首位率の差は強さの主張に使えない。**

| 人数 | 予算 | 予備 | focus決断 | 最大 | p95 | 詳細最大 | 厳密遅延 | 許容幅超え | 最大超過 | 詳細FB | 方針拒否 | 探索回/決断 |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2 | 50 ms | 6 ms | 66 | 52.49 ms | 50.22 ms | 0.002 ms | 3 | 2 | 2.49 ms | 3 | 0 | 45.4 |
| 2 | 200 ms | 25 ms | 66 | 185.22 ms | 183.11 ms | 0.199 ms | 0 | 0 | 0 ms | 0 | 0 | 200.8 |
| 2 | 1,000 ms | 100 ms | 72 | 900.53 ms | 900.38 ms | 0.001 ms | 0 | 0 | 0 ms | 0 | 0 | 1,418.5 |
| 4 | 50 ms | 6 ms | 114 | 68.49 ms | 50.02 ms | 0.003 ms | 2 | 1 | 18.49 ms | 2 | 0 | 11.8 |
| 4 | 200 ms | 25 ms | 132 | 184.58 ms | 178.19 ms | 0.002 ms | 0 | 0 | 0 ms | 0 | 0 | 105.6 |

- 詳細生成は全条件で 0.2 ms 以下。予備 6 ms は 2,000 倍以上の余裕がある。
- 50 ms では**締切の粒度が粗い**。4人戦の 1 シミュレーションが約 3 ms かかるため、締切直前に始まったシミュレーションが 18.5 ms 超過した例がある（114 決断中 1 件）。**これが「保証しない」の実測根拠**であり、隠さず記録する。
- 200 ms 以上では全決断が予算内（厳密比較で 0 件超過）。
- 10 秒の**1試合**はローカルで実行していない（2人戦でも約 40 決断 × 10 s = 約 400 s/席・1試合、座席×arm で数十分）。1手の 10 s は 4節で 2人・4人ともに計測済み。1,000 試合規模は `docs/roadmap-m7-onward.md` 6.8 に従い GitHub Actions の専用ワークフローで実行する：

  ```sh
  gh workflow run paired-eval.yml \
    -f players=2 -f blocks=4 -f seed_start=900 \
    -f candidate=mcts -f baseline=eval -f opponent=greedy \
    -f search_mode=time-budgeted -f decision_budget_ms=10000
  ```

- **首位率は読まない。** 1ブロック（2人=4試合、4人=8試合）では区間が退化する。`scripts/paired_eval_stats.py` も区間と必要数を併記するだけである。

## 6. 記録されるフィールド（`paired_eval` の1行）

| フィールド | 意味 |
|---|---|
| `decision_budget_ms` | 決断全体の予算（0 = 締切なし） |
| `whole_decision_budget_enforced` | 予算が選択**と**詳細生成の両方を拘束したか（= time-budgeted） |
| `detail_reserve_ms` | 探索から詳細生成へ渡した予備（実測値ではなく実際に解決された値） |
| `focus_max_decision_ms` / `focus_p95_decision_ms` / `focus_p99_decision_ms` | 決断全体（選択＋詳細） |
| `focus_max_detail_ms` / `focus_p95_detail_ms` | うち詳細生成 |
| `focus_late_decision_count` | 厳密に予算超過した決断数 |
| `focus_late_beyond_tolerance_count` | 許容幅（予算の1 %、下限1 ms）を超えて超過した決断数 |
| `focus_budget_tolerance_ms` | 上記の許容幅 |
| `focus_max_decision_overshoot_ms` | 最大超過 |
| `focus_decision_fallback_count` / `focus_selection_fallback_count` / `focus_detail_fallback_count` | フォールバック件数 |
| `focus_detail_declined_count` | 詳細方針自体が拒否した件数（予算切れと区別） |
| `focus_budget_exhausted_before_details_count` | 探索が予算を使い切った件数 |
| `focus_observed_all_decisions_within_budget` | 厳密な観測結果（time-budgeted 以外は false） |
| `focus_observed_all_decisions_within_budget_tolerance` | 許容幅込みの観測結果 |
| `focus_observed_all_decisions_within_10s` | 10 秒以内の観測結果 |

集約器 `paired_eval_stats.py` は上記を fail closed で検証し、モードと矛盾する行（simulation-capped なのに予算を主張する行など）を拒否する。`candidate_observed_all_decisions_within_*` は**行のフラグを写さず生カウンタから再計算**する。

## 7. CLI

| オプション | 既定 | 意味 |
|---|---|---|
| `--search-mode` | `simulation-capped` | 変更なし |
| `--decision-budget-ms` | 10000 | 決断全体の予算。time-budgeted では `> 0` 必須 |
| `--detail-reserve-ms` | 未指定（予算の1/8、上限100 ms） | 探索から詳細生成へ渡す予備。`0..=decision_budget_ms/2` |

`--search-mode simulation-capped` は時計を一切見ないため、`--detail-reserve-ms` を指定すると**エラー**にする（適用されない値をログに残さない）。

`paired_eval` は `--search-mode time-budgeted` のとき、**focus 席にだけ**予算を適用する。相手席は同じ実行の中で従来どおり回数上限で動く。

### 7.1 集約器のフラグ（P1で矛盾を解消）

| フィールド | 意味 |
|---|---|
| `candidate_focus_search_deadline_control` | **探索**が締切下で走ったか（探索ポリシー かつ time-budgeted）。旧 `candidate_focus_deadline_control` と同じ意味で、互換のため残す |
| `candidate_whole_decision_budget_enforced` | **決断全体**（選択＋詳細）が1つの時計で拘束されたか。focus 席ならポリシーを問わない |
| `candidate_10s_observed_pass` | 上記の whole-decision 予算が有効・予算が 0 < ms ≤ 10000・実測最大が予算＋許容幅以内、の3条件。**MCTS/PUCT に限定しない** |
| `candidate_10s_observed_pass_basis` | 上記の判定根拠を文章で明示 |

旧版はこのフラグを「探索ポリシーのときだけ」真にし、説明文は「MCTS のときだけ」と書いていたため、**予算化された eval/greedy 席は永久に false** で、文面とも食い違っていた。`caveat` の記述も same 修正した。回帰テストは `PairedStatsTenSecondFlagTests`（3件）。

## 8. 未解決・範囲外

1. **GitHub Actions の入力追加は未実施。** `paired-eval.yml` に `detail_reserve_ms` 入力を足す変更は、この作業環境でワークフロー編集が拒否されたため適用していない。CLI と集約器は既に対応済みで、未指定なら既定の予備が自動で決まるため、既存入力（`search_mode` / `decision_budget_ms`）だけで 10 秒検証は実行できる。入力追加は別PR/別作業で行う。
2. **10 秒の1試合・1,000 試合はローカル未実行**（6節）。1手の 10 秒は計測済み。
3. **ドラフトの1ピックは本予算の対象外。** ロードマップ 3.3 はドラフトの1ピックも同じ上限で評価するとするが、`draft_ai` の決定経路はこのPRで変更していない。別項目として残す。
4. **Web UI のCPU手番は本予算を使っていない。** `src/web.rs::advance_cpu` は従来どおりシミュレーション回数（`self.budget`）で動く。人間対戦の10秒を保証したとは主張しない。`DecisionBudget` は再利用可能な形にしてある。
5. **強さの主張はない。** 本PRは時間制御とその計測のみ。

# M13 探索の時間制御（Issue #164）

更新: 2026-11-07。対象: `src/mcts.rs`、`src/bin/paired_eval.rs`、`src/bin/mcts_profile.rs`、`scripts/paired_eval_stats.py`、`.github/workflows/paired-eval.yml`。

**本ドキュメントは実測値と実装契約の記録。首位率の改善は主張しない。**

## 1. 解いた問題

`MctsPolicy::select_with_budget` は `for _ in 0..self.simulations` を主ループにしていた。締切（deadline）は各シミュレーションの合間・木の降下中・ロールアウト中でしか確認しないため、`--decision-budget-ms 10000 --mcts-simulations 64` では **64回で必ず打ち切られ、10秒予算のほぼすべてが未使用**だった。

修正前の同時刻計測（`mcts_profile`、2人戦の固定局面）:

| 固定シミュレーション数 | 1手の所要時間 |
|---:|---:|
| 4 | 14.3 ms |
| 16 | 41.2 ms |
| 64 | 177.1 ms |
| 256 | 577.6 ms |

つまり既定の64回は **10秒予算の約1.8%** しか使っていなかった。この状態の結果を「同一10秒予算での比較」と記述することはできない。

## 2. 設計：2つの停止規則を明示的に分離する

```rust
pub enum SearchMode { SimulationCapped, TimeBudgeted }

pub enum SearchLimit {
    Simulations(usize),
    TimeBudget { deadline: Instant, max_simulations: usize },
}
```

| モード | 打ち切り条件 | 決定性 | 使う場面 |
|---|---|---|---|
| `simulation-capped` | シミュレーション**試行回数**の上限に達したら（完了数はそれ以下になり得る） | **決定的**（固定seedで再現） | 回帰テスト、再演ハッシュ、過去結果との比較 |
| `time-budgeted` | 締切に達したら（`max_simulations` は暴走防止の安全上限） | **非決定的**（マシン依存） | 人間対戦の1手10秒、実時間予算での比較 |

- `simulation-capped` は**締切を一切適用しない**。モードの契約が「探索量は時計でなく回数で決まる」だからである。このため `deterministic_policy_selection` は実態どおり `true` になる。
- `time-budgeted` は毎シミュレーション後に締切を確認し、**時間が残る限り探索を続ける**。割り込まれたシミュレーションは**破棄**し、回数にも木の更新にも数えない。
- 締切が既に過ぎている場合は木に触らず、`DetailAwareEvaluationPolicy` の合法手へフォールバックする（`used_fallback = true`）。
- 戻り値の `Space` は常に `legal` の要素である（`fallback_space` も `legal.contains` で検査する）。
- `MctsOutcome` に `elapsed_ms` / `overshoot_ms` / `mode` を追加。`overshoot_ms` は締切超過量の実測で、**隠さず記録する**。
- 非公開情報の境界は不変: 入力は `Observation` と合法手リストのみ。内部seed・相手手札は参照しない（既存の `m8_observation_noninterference` 系テストが担保）。

### 後方互換

`select_with_budget(obs, legal, rng, deadline)` は残す。`Some(deadline)` は `TimeBudget { deadline, max_simulations: self.simulations }`、`None` は `Simulations(self.simulations)` に対応する。既存の呼び出し・テストの意味は変わらない。

## 3. 実験A：予算による探索回数の変化（実測）

`./target/release/mcts_profile --search-budget-sweep`（2人戦の固定局面、1手、`tree_depth=2`、`simulations=1` は安全上限のみ）。

| 予算 | 完了シミュレーション | 実測時間 | 予算使用率 | 締切超過 | フォールバック | 合法手 |
|---:|---:|---:|---:|---:|---|---|
| 1,000 ms | 441 | 1000.0 ms | 1.000 | 0 ms | なし | yes |
| 3,000 ms | 1,241 | 3000.0 ms | 1.000 | 0 ms | なし | yes |
| 5,000 ms | 1,855 | 5000.0 ms | 1.000 | 0 ms | なし | yes |
| 10,000 ms | 3,783 | 10000.0 ms | 1.000 | 0 ms | なし | yes |

- 予算が長いほど探索回数が増える（441 → 3,783、約8.6倍）。
- 予算使用率はすべて 1.000（締切直前まで探索している）。
- 締切超過は 0 ms。締切確認が各シミュレーション後に入っているため、この局面では超過が観測されなかった。**超過はゼロを保証しない**（1シミュレーション分の遅れは起こり得る）ため、`max_deadline_overshoot_ms` として常時記録する。
- 1シミュレーションあたりの時間は一定ではない（441/1s = 0.44、3783/10s = 0.38 sims/ms）。木が深くなるほど1回が重くなるため、**「予算2倍＝探索2倍」にはならない**。

## 4. 実験B：simulation-capped の回帰（実測）
カードなし2人戦、`candidate=mcts(baseline=eval)`、seed 700–701の2ブロック、8試合、`--mcts-simulations 8`。

| 比較 | 結果 |
|---|---|
| 同一引数で2回実行 | `replay_hash` / 得点 / 順位 / 完了シミュレーション数 / 決断数がすべて一致（差分は実時間系フィールドのみ） |
| `--decision-budget-ms 0` と `10000` | 上記の意味フィールドに**差分なし**（simulation-capped は予算を使わない） |
| `mcts_deadline_active` | `false`、`deterministic_policy_selection` は `true` |

### 4.1 固定回数はポリシーごとに独立（レビュー指摘の修正）

`simulation-capped` の固定回数は **MCTS と PUCT で別々**に扱う。以前は focus 席のポリシーに関係なく `--mcts-simulations` を渡していたため、**PUCT focus が既定の32ではなくMCTSの64で走り、ログの `puct_simulations_config` は32のまま**という不一致があった。

実測（カードなし2人戦、seed 720の1ブロック、`--mcts-simulations 3 --puct-simulations 7`、`simulation-capped`）:

| focusポリシー | 決断あたり完了シミュレーション | 設定値 |
|---|---:|---|
| `mcts` | 3.0 / 3.0 | mcts=3, puct=7 |
| `puct` | 7.0 / 7.0 | mcts=3, puct=7 |

修正前は PUCT focus も 3.0 になっていた（MCTS の値を流用していた）。回帰テストは `simulation_capped_uses_each_search_policys_own_count`。

## 5. 実験C：実戦スモーク

条件：カードなし、`candidate=mcts`、`baseline=eval`、相手席 `greedy`、`tree_depth=2`。2人戦はseed 810–811の2ブロック（各arm 4試合）、4人戦はseed 811の1ブロック（各arm 4試合）。releaseビルド、ローカルWindows。

time-budgeted は **1手200 ms**（10秒ではない）。固定回数側は16回。**同じ探索量での比較ではない**ので、両モードの数値を強さの比較として読めない。

| 条件 | モード | 首位クレジット cand / base | 差 | 95%区間 | 単独1位率 | 期待順位 | 得点差 | 決断あたりシミュレーション | 最大決断時間 | 超過 |
|---|---|---:|---:|---|---:|---|---:|---:|---:|---:|
| 2人戦 | simulation-capped (16回) | 75.0 / 100.0 pp | −25.0 pp | [−50.0, +0.0] | 0.75 | 1.25 / 1.00 | +7.5 | 16.0 | 50 ms | 0 ms |
| 2人戦 | time-budgeted (200 ms) | 100.0 / 100.0 pp | ±0.0 pp | [+0.0, +0.0] | 1.00 | 1.00 / 1.00 | +13.2 | 321.6 | 203 ms | 2 ms |
| 4人戦 | simulation-capped (16回) | 25.0 / 75.0 pp | −50.0 pp | [−50.0, −50.0] | 0.25 | 3.00 / 1.25 | −7.3 | 16.0 | 141 ms | 0 ms |
| 4人戦 | time-budgeted (200 ms) | 50.0 / 75.0 pp | −25.0 pp | [−25.0, −25.0] | 0.50 | 1.75 / 1.25 | +5.2 | 170.5 | 201 ms | 1 ms |

読み方と限界：

- **強さの主張はしない。** 各arm 4試合で信頼区間は退化しており（幅0〜50pp）、必要試合数は数百〜千の桁である。上の表でMCTSが評価関数AIを上回っていないことは、この規模では何も意味しない。
- **固定回数と固定時間を同じ表の列として並べているが、比較してはならない。** time-budgeted は1手あたり約170〜322回探索しており、simulation-capped の16回の10〜20倍である。
- 予算200 msに対し、**決断全体（選択＋詳細生成）は最大203 ms**まで伸びた（超過1〜2 ms）。締切は探索のみを拘束し、選択後の詳細生成は拘束しないためである。10秒予算では無視できる量だが、**ゼロではない**ことを記録しておく。
- `timeout_rate = 1.000` は異常ではない。time-budgeted では「締切が探索を終わらせた」ことが正常であり、その割合が1.0になる。simulation-capped では締切を適用しないため 0.0 でなければならない。集約レポートの `timeout_rate_semantics` に明記している。
- フォールバック率は全条件で 0.000（探索結果が常に使えた）。

## 5.1 受け入れ条件の適用範囲（Issue #164 と #167 の切り分け）

`Issue #164` には「1手あたり最大10秒」と「**全手で10秒以内**」の両方が書かれている。本PRが満たすのは**探索部分**であり、**1手全体**ではない。達成済みと記述しない。

| 条件 | 状態 | 根拠 |
|---|---|---|
| 探索が時間予算を使い切る（64回で打ち切らない） | **達成** | §3（1s→441、10s→3783回、予算使用率1.000） |
| 予算が長いほど探索回数が増える | **達成** | §3 |
| 締切超過量を記録する | **達成** | `max_deadline_overshoot_ms`（実測0〜2 ms） |
| 期限切れ時に合法手を返す（探索しない） | **達成** | `an_already_spent_budget_returns_*`（MCTS・PUCT とも） |
| 固定回数モードの決定性・再演一致を維持 | **達成** | §4 |
| モードを CLI・ログ・集計・CI から指定できる | **達成** | §6 |
| **1手全体（探索＋詳細生成）が常に10秒以内** | **未達・範囲外** | §5（200 ms 予算で決断全体 203 ms）。探索後の詳細生成は予算外。**Issue #167 の A 項目**で扱う |

したがって「全手で10秒以内を保証した」とは主張しない。本PRが保証するのは「探索は締切で止まり、必ず合法手を返す」ところまでである。

## 5.2 残存リスク

1. **締切超過はゼロ保証ではない。** 締切は各シミュレーション後にしか確認できないため、1シミュレーション分の遅れが生じ得る。本計測では 0〜2 ms だったが、重い局面ではより大きくなる。`max_deadline_overshoot_ms` を常時記録して監視する。
2. **詳細生成は予算外。** 1手全体の10秒制限を厳密に守るには、選択後の `detail_policy.produce` も予算に含める設計変更が別途必要（本PRの範囲外）。
3. **`mcts_simulations` の意味がモード依存。** simulation-capped では固定回数、time-budgeted では相手席の回数。`--time-budget-max-simulations` が別途安全上限となる。
4. **PUCTも同じ2モードで動く。** マージ済みの `src/mcts_puct.rs` に `select_with_limit` を追加し、`MctsOutcome` の新フィールド（`elapsed_ms` / `overshoot_ms` / `mode`）を埋めるようにした。PUCT の `search_mode` も `--search-mode` で切り替わり、集約器のモード検査は PUCT にも同じく適用される。

## 6. CLI・評価ログ・CI

### `paired_eval`

| オプション | 既定 | 意味 |
|---|---|---|
| `--search-mode` | `simulation-capped` | `simulation-capped` / `time-budgeted` |
| `--mcts-simulations` | 64 | simulation-capped の固定回数。time-budgeted では**相手席**の回数 |
| `--time-budget-max-simulations` | 100000 | time-budgeted の安全上限（暴走防止） |
| `--decision-budget-ms` | 10000 | time-budgeted の1手予算。`time-budgeted` では `> 0` 必須 |

記録される追加フィールド:

- `search_mode`、`search_budget_enforced`
- `mcts_simulations_cap`、`time_budget_max_simulations`
- `mcts_simulations_per_decision`（決断あたりの完了シミュレーション数）
- `max_deadline_overshoot_ms`、`max_search_elapsed_ms`
- `mcts_deadline_active`、`deterministic_policy_selection`（モードから導出）

### 集約器 `paired_eval_stats.py`

- `--search-mode` を受け取り、行の `search_mode` と一致しなければ**拒否**する。
- 1つのレポート内でモードが混在したら**拒否**する（`inconsistent search modes`）。
- `time-budgeted` なのに `decision_budget_ms <= 0` の行は**拒否**する。
- `search_budget_enforced != (mode == time-budgeted)` を**拒否**する。
- `candidate_focus_deadline_control` と `*_10s_observed_pass` は **time-budgeted かつ mcts のときだけ** true になり得る。simulation-capped の結果で「10秒以内」を主張できない。

### アブレーション `eval_ablation.py`

比較可能性チェックに `search_mode` を追加。**固定回数のarmと固定時間のarmを同じ表に混ぜられない。**

### GitHub Actions `Paired AI Evaluation`

- `workflow_dispatch` 入力に `search_mode`（choice）と `time_budget_max_simulations`（number）を追加。既定は `simulation-capped` で、既存の実行条件を変えない。
- `time-budgeted` 選択時は `decision_budget_ms > 0` をジョブ側でも検証。
- 集約は `--search-mode "$SEARCH_MODE"` を渡し、Step Summary にモードと予算を記録する。
- PRスモークに `time-budgeted`（60 ms）の実走を追加した。**配管の確認**であり強さの測定ではない。
- 大量対戦は従来どおり専用ワークフローの手動起動でのみ実行し、通常CIは重くしない。

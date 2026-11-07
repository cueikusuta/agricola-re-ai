# M13 アブレーション実行基盤（Issue #106）

更新: 2026-11-07。**基盤の実装記録。強さの改善結果ではない。** 本ドキュメントは `scripts/eval_ablation.py` の契約を定める。首位率+5ppの判定は `docs/roadmap-m7-onward.md` 3節・3.2節、実施計画は `docs/ai-experiments-m12-m14.md` 4節（B-2）を正本とする。

## 1. 何を解く問題か

M13は「探索の各変更が、首位率改善にどれだけ寄与したか」を問う。ところが `paired_eval` の1回の実行は**候補AI 対 固定baseline**の比較であり、この数値だけでは「事前分布を変えた効果」と「リーフ評価を変えた効果」を分離できない。要因を1つずつ変えた**複数arm**を、同じseedブロック・同じbaselineゲーム上で比較する必要がある。

`scripts/eval_ablation.py` はそれを機械化する。1 arm = 1回の `paired_eval` 実行。

## 2. 2種類の比較を混同しない

| 比較 | 意味 | フィールド |
|---|---|---|
| arm内 | candidate 対 そのarmの baseline | `mean_first_credit_delta_pp_vs_own_baseline`, `paired_block_bootstrap_95ci_pp_vs_own_baseline` |
| arm間 | candidate 対 **control armのcandidate**（seedブロックで対応） | `mean_first_credit_delta_pp_vs_control`, `paired_block_bootstrap_95ci_pp_vs_control` |

B-2の問いに答えるのは**arm間**の差である。arm内の差は「baselineより強いか」しか言わない。

## 3. 比較可能性のゲート（fail closed）

arm間の差が解釈できるのは、次のすべてが成り立つときだけ。1つでも崩れたら集約は失敗する。

1. 全armが同じ `players` / `opponent` / `seed_start` / `first_block` / `seed_blocks` / `additional_action_space` / `baseline` / `decision_budget_ms` / `cards` を使う。
2. **全armのbaselineゲームが同一**である。`(block_index, candidate_seat)` ごとに `replay_hash`・首位クレジット・得点列を突合し、1件でも違えば拒否する。baselineが違えば「controlとの差」は変更の効果ではなく別ゲームの差になる。
3. 各armの行は `paired_eval_stats.validate_and_report` をそのまま通す（block/arm/seatの欠落・重複、順位連続性、首位クレジット、再演フラグ、時間計測など既存の全検査）。
4. `candidate` と `baseline` が同一ポリシーのarm、未対応ポリシー名、空のarm、control不在、arm 1個だけの実行は拒否する。

## 4. 統計

- arm間の差は `delta_block = candidate_credit_block(arm) − candidate_credit_block(control)` を作り、**seedブロック単位**で percentile bootstrap（反復数・乱数seedをレポートに固定記録、既定10,000反復・seed `20261106`）。
- 座席を独立標本として数えない。同点首位の `1/k` クレジットは `paired_eval` が記録した値を使う。
- 単位は**パーセントポイント**。`1.0` の割合は `100pp`。

## 5. 実行方法

arm定義はJSONで与える。

```json
{
  "players": 2,
  "seed_start": 900,
  "blocks": 2,
  "baseline": "random",
  "opponent": "greedy",
  "decision_budget_ms": 0,
  "arms": [
    {"label": "control_puct", "candidate": "puct"},
    {"label": "leaf_heuristic", "candidate": "puct", "puct_leaf": "heuristic"},
    {"label": "explore_c3", "candidate": "puct", "puct_c": 3.0},
    {"label": "mcts_sim2", "candidate": "mcts", "mcts_simulations": 2, "mcts_tree_depth": 2}
  ]
}
```

検索パラメータは **仕様レベル**（`arms` の外）に既定値として書き、**arm単位**で上書きできる。どちらも同じ名前を使う。

```json
{
  "players": 2, "seed_start": 900, "blocks": 2,
  "baseline": "eval", "opponent": "greedy",
  "puct_leaf": "terminal",
  "arms": [
    {"label": "control", "candidate": "puct"},
    {"label": "leaf_heuristic", "candidate": "puct", "puct_leaf": "heuristic"}
  ]
}
```

```sh
cargo build --release --bin paired_eval
python3 scripts/eval_ablation.py \
  --arms arms.json --control control_eval \
  --root ablation --run --output ablation/report.json
```

`--run` を外すと、`<root>/<label>/shard-*/matches.jsonl` に既にある結果を集約だけする（大量試合をActionsで回し、集約をローカルで行う使い方）。

## 6. 現時点で測れる要因と、まだ測れない要因

| 要因 | 状態 |
|---|---|
| 候補ポリシー（`random`/`greedy`/`eval`/`mcts`/`puct`） | 実行可能 |
| MCTS `mcts_simulations` / `mcts_tree_depth` | 実行可能（シミュレーション回数固定＝決定論的） |
| **PUCT の全パラメータ**（`puct_simulations` / `puct_tree_depth` / `puct_c` / `puct_prior_temp` / `puct_value_temp` / `puct_leaf` / `puct_root_limit` / `puct_final_prior`） | **実行可能（#167 B）**。arm単位で指定し、`paired_eval` のフラグへそのまま渡す。既定値は `PuctConfig::default()` と一致 |
| per-decision思考時間予算 `decision_budget_ms` | 実行可能（時間予算ありの比較は非決定論的。`deterministic_policy_selection` がfalseになる） |
| カードあり条件 | `scripts/paired_eval_stats.py --cards` と `paired_eval --cards` で実行可能（2人戦のみ）。M10 G1（4人40+40）未達のため4人戦カードありは不可 |
| ドラフト条件 | **未提供**（#155の慣行決定と M10 のプール整備が前提） |

### 6.1 パラメータ検証（fail closed）

- **未知のキーは拒否する。** 仕様レベル・arm単位のどちらでも、既知でないキーは実行前にエラーにする。typo したパラメータが既定値のまま「効果なし」として通るのを防ぐ。
- **値域は `paired_eval` と同じ。** `*_simulations` 1..=100000、`*_tree_depth` 1..=6、`puct_c` 0..=100、温度は有限かつ > 0、`puct_leaf` は `heuristic`/`terminal`、`puct_root_limit` 0..=100、`puct_final_prior` 0..=10。整数指定の項目に小数を書くと拒否する。
- **既定値も明示的に渡す。** arm が指定しなかったパラメータも既定値で `paired_eval` に渡し、レポートの `search_parameters` に実際の値を載せる。
- **有効／無効をレポートに残す。** 各armは `changed_search_parameters`（制御との差分）に加え、`effective_changed_search_parameters`（その実行で意味を持つ差分）を持つ。無効な差分は実行前に拒否されるため両者は通常一致するが、「この要因は有効である」ことを成果物から読み取れる。
- **PUCT専用パラメータを PUCT を使わない arm で変えると拒否する。** `candidate` も `baseline` も `puct` でない arm で `puct_*` を動かしても誰も読まないため、「効果ゼロ」の証拠に見えてしまう。
- **MCTS専用パラメータも同様に拒否する（レビューP1）。** `mcts_simulations` / `mcts_tree_depth` を、`candidate` も `baseline` も `mcts` でない arm で変えても読まれない。PUCT 同士の比較で MCTS の木の深さを動かす、といった指定は無効なのでエラーにする。
- **time-budgeted では回数パラメータの変更を拒否する（レビューP1）。** このモードの停止条件は壁時計であり、`mcts_simulations` / `puct_simulations` は**暴走防止の安全上限**にすぎない。ここで回数を変えても「効果ゼロ」しか測れないため、比較は `simulation-capped` で行うようエラーで促す。木の深さ（`*_tree_depth`）は探索構造を変えるので time-budgeted でも有効な要因として扱う。
- **計画段階で検証する。** 上記の検査は試合を1つも走らせる前に実行する（`check_plan`）。集約時の検証も残すが、無駄な計算をしてから落ちることはない。
- **1要因原則は全検索パラメータに適用する。** 制御armと比べて2つ以上変わる arm は拒否する（`changed_search_parameters` に差分を記録）。
- **モード混在は従来どおり拒否する。** `simulation-capped` と `time-budgeted` を同じ表に混ぜられない。

## 7. ローカル小規模スモーク（強さの主張ではない）

### 7.1 PUCT パラメータの1要因比較（#167 B、2026-11-07）

条件：カードなし2人戦、baseline `eval`、他席 `greedy`、seed 900〜901の2ブロック（各arm 4試合）、releaseビルド、ローカルWindows、`--iterations 500`。

| arm | 変更した要因 | 首位クレジット | control比 | 95%区間 | 最大決断時間 |
|---|---|---:|---:|---|---:|
| `control_puct` | （制御） | 100.0 pp | 0.0 pp | [0.0, 0.0] | 91.9 ms |
| `leaf_heuristic` | `puct_leaf` | 75.0 pp | −25.0 pp | [−50.0, 0.0] | **1.9 ms** |
| `explore_c3` | `puct_c` | 100.0 pp | 0.0 pp | [0.0, 0.0] | 153.5 ms |
| `prior_temp1` | `puct_prior_temp` | 100.0 pp | 0.0 pp | [0.0, 0.0] | 112.8 ms |
| `mcts_baseline` | `candidate` | 100.0 pp | 0.0 pp | [0.0, 0.0] | 155.5 ms |

読み方と限界：

- **強さの主張はしない。** 2ブロック（各arm 4試合）では区間幅が最大50 ppで退化しており、首位率の差は判定できない。上の表で言えるのは「配管が通り、各armが1要因だけ変えている」ことまでである。
- **`leaf_heuristic` の差は強さの証拠ではない。** 同じ表の最大決断時間が 91.9 ms → 1.9 ms と約48倍速いことは**コスト**の観測であり、首位率の改善とは別指標。最終的な採否は `docs/roadmap-m7-onward.md` 3.2節の手順（1,000試合/arm、事前登録した一次比較）で決める。
- `changed_factors_vs_control` が各armで1要素、`search_parameters` が実行時の実値、`baseline_games_are_shared: true` が付いていることをレポートで確認済み。

### 7.1.1 有効性検証の実測（#167 B レビューP1、2026-11-07）

| 指定 | 結果 |
|---|---|
| time-budgeted で `mcts_simulations` を変更 | **拒否**（exit 1）: `arm 'count' changes ['mcts_simulations'] in time-budgeted mode, where the simulation count is only a safety cap ...` |
| PUCT 同士の arm で `mcts_tree_depth` を変更 | **拒否**: `MCTS-only parameters` |
| MCTS 同士の arm で `puct_prior_temp` を変更 | **拒否**: `PUCT-only parameters` |
| simulation-capped で `puct_prior_temp` を変更 | 実行（1ブロック4試合、`changed_search_parameters.puct_prior_temp = {control: 3.0, arm: 1.0}`、`effective_changed_search_parameters = ["puct_prior_temp"]`） |
| time-budgeted で `mcts_tree_depth` を変更 | 有効な要因として受理（`effective_changed_search_parameters = ["mcts_tree_depth"]`） |

### 7.2 初期スモーク（2026-11-07 以前、PUCT 非対応時）

- 条件：カードなし2人戦、baseline `random`、他席 `greedy`、seed 900〜901の2ブロック、各arm 8試合、releaseビルド、`--decision-budget-ms 0`、ローカルWindows。
- 結果：`control_eval` 100.0pp、`greedy` 50.0pp（control比 −50.0pp）、`mcts_sim2` 0.0pp（control比 −100.0pp）。全armでbaselineゲーム同一（`baseline_games_are_shared: true`）。
- **2ブロックでは信頼区間が退化し（幅0）、統計的判断に使えない。** ここに記した数値は「配管が通る」ことの確認であり、MCTSの強さ・+5pp達成について何も主張しない。本番判定は `docs/roadmap-m7-onward.md` 3.2節の手順（1,000試合/arm、事前登録した一次比較）で行う。

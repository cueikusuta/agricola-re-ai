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
    {"label": "control_eval", "candidate": "eval"},
    {"label": "greedy", "candidate": "greedy"},
    {"label": "mcts_sim2", "candidate": "mcts", "mcts_simulations": 2, "mcts_tree_depth": 2}
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
| 候補ポリシー（`random`/`greedy`/`eval`/`mcts`） | 実行可能 |
| MCTS `mcts_simulations` | 実行可能（シミュレーション回数固定＝決定論的） |
| MCTS `mcts_tree_depth` | 実行可能 |
| per-decision思考時間予算 `decision_budget_ms` | 実行可能（時間予算ありの比較は非決定論的。`deterministic_policy_selection` がfalseになる） |
| PUCTの事前分布・目的関数・リーフ評価の分離 | **未提供**。`src/mcts_puct.rs` はPR #146で未マージであり、本基盤は既存 `src/mcts.rs` のパラメータだけを扱う。マージ後にarmとして追加する |
| カードあり条件 | **未提供**。`--cards` の受け渡しは別PRで扱う。M10 G1（4人40+40）未達のため4人戦カードありは不可 |
| ドラフト条件 | **未提供**（#155の慣行決定と M10 のプール整備が前提） |

## 7. ローカル小規模スモーク（強さの主張ではない）

- 条件：カードなし2人戦、baseline `random`、他席 `greedy`、seed 900〜901の2ブロック、各arm 8試合、releaseビルド、`--decision-budget-ms 0`、ローカルWindows。
- 結果：`control_eval` 100.0pp、`greedy` 50.0pp（control比 −50.0pp）、`mcts_sim2` 0.0pp（control比 −100.0pp）。全armでbaselineゲーム同一（`baseline_games_are_shared: true`）。
- **2ブロックでは信頼区間が退化し（幅0）、統計的判断に使えない。** ここに記した数値は「配管が通る」ことの確認であり、MCTSの強さ・+5pp達成について何も主張しない。本番判定は `docs/roadmap-m7-onward.md` 3.2節の手順（1,000試合/arm、事前登録した一次比較）で行う。

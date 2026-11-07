# M8-3: MCTS改善の分析とPUCT実装（Issue #110）

更新日：2026-11-06。**本ファイルは開発seedでの実装・計測記録。最終確認はheld-out seed・GitHub Actionsの1,000試合で別途行う。** ルール正確性の証明ではない。

## 1. 現行MCTSの敗因

対象：カードなし4人戦、追加タイルOFF、相手3席は評価関数AI、座席ローテーション、開発seed 870000-870009（40試合/arm）。

- baseline MCTS 32sim/depth2：首位クレジット **0.125**、平均順位 3.100、得点差 -3.09。
- 評価関数AI：首位クレジット 0.250、平均順位 2.500、得点差 0.00。

原因：

1. **候補配分**：全root候補へ均等にシミュレーションを配る。32シミュレーションでは各候補1〜3回で、勝率推定の分散が大きい。
2. **未展開子のランダム選択**：評価関数の知識を事前分布に使わない。
3. **終局プレイアウトのコストと分散**：1シミュレーションが終局までの数十手を含み、1手あたり約1.5ms。32シミュレーションで数十ms〜数百ms。

## 2. 実装した方式

`src/mcts_puct.rs`（新規）。既存 `MctsPolicy` は変更しない。

### A. 評価関数を事前分布に使うPUCT

節点の行動価値を

```
v(a) = 2 * projected_state_delta(a) + 0.5 * tactical_weight(a)
```

とする。これは `WeightedEvaluationPolicy`（評価関数AI）が最大化する値そのもの。`softmax(v / prior_temperature)` を事前分布 `P(a)` とし、選択は

```
argmax_a [ Q(a) + c_puct * P(a) * sqrt(N_parent + 1) / (1 + n(a)) ]
```

未訪問子は `Q=0`、`n=0` なので、最優先手から順に試される。

### B. ロールアウト改善

- `LeafMode::Terminal`：従来同様、評価方針のプレイアウトで終局まで進め、`1/k` の首位クレジットを得る。
- `LeafMode::Heuristic`：木の深さ制限でリーフ到達後、評価値のsoftmaxをクレジットとして使う（終局プレイアウトなし）。高速だが後述の制約あり。

### C. 候補削減・探索配分

`c_puct`、`root_candidate_limit`（上位N手のみ）、`final_prior_weight`（最終選択での事前分布の重み）を設定可能。非公開情報は `sample_game` 経由で `Observation` のみから生成し、相手手札・未来ラウンド・内部seedを読まない。

## 3. 開発seed結果

コマンド例（再現用）：

```sh
cargo build --release --locked --bin paired_eval
./target/release/paired_eval \
  --players 4 --seed-start 870000 --block-start 0 --blocks 10 \
  --candidate puct --baseline eval --opponent eval \
  --additional-action-space false --decision-budget-ms 0 \
  --puct-simulations 32 --puct-tree-depth 2 --puct-leaf terminal \
  --puct-final-prior 0 --output results.jsonl
```

| candidate | seed | 首位クレジット | 平均順位 | 得点差 | max | p95 | p99 |
|---|---|---:|---:|---:|---:|---:|---:|
| baseline MCTS 32sim/depth2 | 870000台 | 0.125 | 3.100 | -3.09 | 568ms | — | — |
| 評価関数AI | 870000台 | 0.250 | 2.500 | 0.00 | 2ms | 0.6ms | 0.7ms |
| **PUCT terminal 32sim/depth2** | 870000台 | **0.725** | **1.600** | **+5.26** | 627ms | 463ms | 627ms |
| **PUCT terminal 32sim/depth2** | 871000台 | **0.825** | **1.400** | **+7.18** | 557ms | 548ms | 557ms |
| PUCT heuristic 64sim/depth3（prior 3.0） | 870000台 | 0.300 | 2.225 | +1.98 | 11ms | — | — |
| PUCT heuristic 64sim/depth3（prior 0） | 870000台 | 0.000 | 3.800 | -15.06 | 11ms | — | — |

時間は1手全体（アクション選択＋詳細生成）の `focus_max_decision_ms`。10秒上限内。候補-評価関数AIの首位率差は **+47.5pp（870000台）/ +57.5pp（871000台）**（各40試合/arm、同一seed・座席交代）。

### 解釈と限界

- 事前分布型PUCT＋終局ロールアウトが有効。baseline MCTSと評価関数AIの双方を大きく上回った。
- **heuristicリーフは高速だが単独では弱い**。評価クレジット（softmax評価値）は評価関数AIの戦術項 `tactical_weight` を含まないため、`final_prior_weight` でアンカーしないと悪化する。既定ではterminalを採用し、heuristicは高速代替として提供する。
- 開発seed上の点推定であり、**held-outの最終確認前**。95%区間と必要試合数は集約レポートで算出する。
- 相手はすべて評価関数AI1種、カードなし4人戦に限る。2/3人、カードあり、ドラフトは未評価。
- 時間は開発機（Windows）の実測。ハードウェア差の影響を受ける。

## 4. held-out最終確認（GitHub Actions・標準CPUランナー）

- Run: [42255268299](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42255268299)（4シャード＋集約、全ジョブ成功、SHA `ecd138b`）。
- 条件：カードなし4人、追加タイルOFF、相手3席 `eval`、座席ローテーション、held-out seed **980000-980249** の250独立ブロック、**各arm 1,000試合（合計2,000試合）**。
- 候補：`puct` terminal 32sim/depth2、`c_puct=1.5`、`prior_temperature=3`、`final_prior_weight=0`、deadlineなし（シミュレーション数固定・決定論的）。
- 首位クレジット率（同率`1/k`クレジット）：

| 指標 | candidate: PUCT | baseline: 評価関数AI | 差 |
|---|---:|---:|---:|
| 首位クレジット率 | **0.820（82.0%）** | 0.250（25.0%） | **+57.0pp** |
| 平均順位 | 1.363 | 2.499 | -1.136 |
| 対戦相手平均との得点差 | +7.173 | 0.000 | +7.173 |

- 対応差（250ブロック単位のpercentile bootstrap、10,000回、seed `20261106`）95%区間：**[+54.4pp, +59.4pp]**。区間下限が0を大きく超え、点推定で+5pp以上。
- 必要ブロック数目安（観測分散、5pp差・80%検出力）：124（実際250実施）。
- 意思決定時間（focus席の1手全体＝アクション選択＋詳細生成）：p95 **180.9ms**、p99 **195.7ms**、最大 **195.7ms**。タイムアウト0、フォールバック0。10秒上限を大きく下回る。
- 別途、`decision-budget-ms=10000` の開発run（seed 872000台、5ブロック）でも最大317ms・タイムアウト0・フォールバック0で、期限到達なしに全シミュレーション完了。
- Artifact：`shard-0..3` の試合別JSONLと `paired-eval-report`（集約レポート）。保持期間後は取得不可のため、条件を本書に残す。

### 判断

held-out seedで **+57.0pp（95%CI +54.4〜+59.4pp）** を再現したため、**PUCT terminal 32sim/depth2 を採用候補として既定にする**。heuristicリーフは高速だが単独では弱く、既定にしない。

## 5. 次の作業

1. 2人/3人、カードあり、追加タイルON、10-7ドラフトでの再評価（別モードとして合算しない）。
2. 評価関数と整合する価値関数の学習（M14ゲート）。p95時間内でのsimulation増加。
3. 対戦相手を `greedy`・`mcts` へ変えたロバスト性確認。

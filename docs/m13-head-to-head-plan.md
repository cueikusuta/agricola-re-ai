# M13 判別力のある比較条件とアブレーション計画（#106 / #167）

更新: 2026-11-08。**本ファイルは計画と基盤の記録であり、強さの改善結果ではない。** 実測結果は `docs/m13-puct-vs-mcts-2026-11-08.md`（既存）にあり、本計画の実行結果はまだ無い（6節）。

## 1. 監査：既にあるもの／無いもの（C-1）

| 要素 | 状態 | 実体 |
|---|---|---|
| 座席ローテーション付きペア評価 | 実装済み | `src/bin/paired_eval.rs`、`scripts/paired_eval_stats.py` |
| seedブロック単位の95%区間 | 実装済み | `paired_block_bootstrap_95ci`（percentile bootstrap、反復数・seed固定） |
| 同一時間予算の比較 | 実装済み | `--search-mode time-budgeted --decision-budget-ms`、`DecisionBudget`（探索＋詳細生成を1つの時計で束縛） |
| 固定シミュレーション回数の比較 | 実装済み | `--search-mode simulation-capped`。時間予算と**混在拒否** |
| 1要因アブレーション基盤 | 実装済み | `scripts/eval_ablation.py`（計画検証・arm間比較・共有条件の拒否） |
| PUCT全パラメータのarm化 | 実装済み | prior温度・c_puct・リーフ評価・木の深さ・root制限・final prior |
| 4人戦の全席ローテーション | 実装済み | `--players 4`、1ブロック = 4席 × 2arm |
| **直接対戦（head-to-head）の明示** | **今回追加** | `paired_eval_stats` の `comparison_design` |
| **天井（飽和）検出** | **今回追加** | `paired_eval_stats` の `saturation` |
| **実験定義の機械検証** | **今回追加** | `eval_ablation.py --check-plan` と `docs/m13-head-to-head-arms.json` |
| 4人戦・時間予算1,000試合/arm | 未実行（時間制約） | 5.2節 |
| ドラフト条件 | 未提供 | #155 の慣行決定待ち |

## 2. なぜ既存条件では差を測れないか

`docs/m13-puct-vs-mcts-2026-11-08.md` の条件A（2人戦・200ms）は、候補と比較対象を**同じ `greedy` 相手**に別々に当てる共通相手ベンチマークだった。結果は PUCT 100.0% / MCTS 99.0% で、**両方が天井に張り付いた**。天井付近では +5pp の検出力が極端に落ちる。

対策は2つ。

1. **相手を強くする**：`opponent` を `baseline` と同じAIにする。2人戦では候補席が比較対象AIと直接対戦し、比較対象armは鏡（同士）になる。4人戦では候補席が「比較対象AI 3人」という固定の強い場に当たる。
2. **天井を検出して報告する**：`saturation.ceiling_limited` が真なら、その条件は +5pp の判定に使わない。

## 3. 条件定義（C-2 / C-3 / C-4）

### 条件C：2人戦・直接対戦・同一時間予算（主要）

| 項目 | 値 |
|---|---|
| モード | 2人戦・カードなし・追加タイルOFF |
| candidate / baseline / opponent | `puct` / `mcts` / `mcts` |
| 停止規則 | `time-budgeted` 200 ms（探索＋詳細生成） |
| seed | 940000–940499（500ブロック × 2席 = 各arm 1,000試合） |
| 解釈 | candidate arm = PUCT 対 MCTS の直接対戦。baseline arm = MCTS 同士の鏡。`comparison_design.head_to_head=true` |

鏡の baseline arm は**対称性の検査**に使う（同条件なら首位クレジットは約50%に近づく）。偏りが大きい場合は座席や開始プレイヤーの非対称を疑い、差の解釈を止める。

### 条件D：4人戦・固定の強い場・同一時間予算（補助）

| 項目 | 値 |
|---|---|
| モード | 4人戦・カードなし・追加タイルOFF |
| candidate / baseline / opponent | `puct` / `mcts` / `mcts` |
| 停止規則 | `time-budgeted` **50 ms**（まず実行可能性を確認） |
| seed | 950000–950249（250ブロック × 4席 = 各arm 1,000試合） |

4人戦は1試合あたりの決断数が多く、200 msでは1シャード65分の枠を超える見込み（既存記録）。**まず50 msで完走と区間幅を確認**し、必要なら 100 ms へ上げる。時間予算を変えた結果は別条件として扱い、合算しない。

### 条件E：4人戦・固定回数（参考、主要判定に使わない）

既存の条件B（32回固定）は PUCT +65pp だったが、**MCTS既定64回に対し32回で不利**であり、同一時間予算でもない。参考値として残し、主要判定には使わない。

## 4. アブレーション（C-5）

`docs/m13-head-to-head-arms.json`（条件Cと同条件）を `eval_ablation.py` で回す。**1 arm = 1要因**。

| arm | 変える要因 | 問い |
|---|---|---|
| `control_puct` | （制御） | 既定 prior 温度3・c=1.5・terminal leaf・深さ2 |
| `prior_temp1` / `prior_temp6` | `puct_prior_temp` | 事前分布の寄与 |
| `explore_c3` / `explore_c06` | `puct_c` | 探索係数 |
| `leaf_heuristic` | `puct_leaf` | リーフ評価方式（terminal rollout 対 heuristic） |
| `depth3` | `puct_tree_depth` | 探索の形 |

```sh
python3 scripts/eval_ablation.py \
  --arms docs/m13-head-to-head-arms.json --control control_puct \
  --check-plan --output /dev/null          # 計画だけ検証（試合をしない）
python3 scripts/eval_ablation.py \
  --arms docs/m13-head-to-head-arms.json --control control_puct \
  --root ablation/head-to-head --run --output ablation/head-to-head/report.json
```

- **時間予算そのもののアブレーションは1つのplanでできない。** `decision_budget_ms` は比較の前提（共有条件）であり、arm間で変えると「探索量が違う」比較になるため基盤が拒否する。予算の効果は**同一seedで別planを回し、arm間の首位率を並べる**別解析として扱い、同じ表に混ぜない。
- 固定回数でのアブレーションは `search_mode: simulation-capped` の別planで行う。時間予算の結果と合算しない。

## 5. 事前に決める判定規則（C-6）

結果を見る前に固定する。

1. **主要指標**は首位クレジット率（`1/k` 含む）。差は **絶対パーセントポイント**。
2. 合格は **点推定 +5pp 以上** かつ **差の95%区間下限 > 0**。区間が0を跨ぐ場合は「判定保留」。
3. 判定保留のときは、観測したばらつきから**検出力80%・両側5%**で必要ブロック数を再計算し、事前に決めた上限（1,000ブロック）まで増やす。途中結果で無制限に検定を繰り返さない。
4. `saturation.ceiling_limited=true` の条件は、たとえ差が出ても**強さの差として報告しない**。条件を変える。
5. `comparison_design.head_to_head=false` の結果を、直接対戦の結果として書かない。
6. 2人戦／4人戦、カードなし／あり、時間予算／固定回数は**別条件**。合算しない。

### 5.1 小規模パイロット（C-6）

本実行の前に、条件Cを **20ブロック（各arm 40試合）** で回して次を確認する。

- 完走率（`finished=false` が0）
- 天井に張り付いていないか（`saturation.ceiling_limited`）
- 1シャードの所要時間と、500ブロックへの外挿
- Artifact と集約レポートの整合

パイロットの結果は**強さの主張に使わない**。

### 5.2 実行規模の見積り

| 条件 | 決断数/試合（概算） | 1,000試合/arm の探索時間 | 判断 |
|---|---:|---|---|
| C: 2人・200ms | 約40 | 2,000試合 × 40 × 0.2s ≈ 4.4時間（4並列で約1.1時間） | 4シャードで実行可能。シャード上限65分に注意 |
| D: 4人・200ms | 約80 | 約8.9時間（4並列で約2.2時間） | シャード上限超過。50msへ下げる |
| D: 4人・50ms | 約80 | 約2.2時間（4並列で約33分） | 実行可能 |

（決断数は既存記録の実測からの概算。実測はパイロットで置き換える。）

## 6. 現在の状態

- 条件定義・計画検証・判定規則・飽和検出・直接対戦の明示を実装した。
- **実測は未実施**。理由は本セッションの権限: `gh workflow run`（Actions 起動）と `gh pr create` が拒否され、大量試合を実行できない。ローカルでの大量シミュレーションも方針（`AGENTS.md` 8）により行わない。
- したがって **+5pp の達成・未達・判定保留のいずれも報告しない**。既存の条件A/Bの数値は履歴として保持し、本計画の結果と混同しない。

## 7. 次に必要な作業

1. `paired-eval.yml` で条件Cのパイロットを実行する。

```sh
gh workflow run paired-eval.yml -f players=2 -f blocks=20 -f seed_start=940000 \
  -f candidate=puct -f baseline=mcts -f opponent=mcts -f cards=none \
  -f search_mode=time-budgeted -f decision_budget_ms=200
```

2. 集約レポートの `comparison_design.head_to_head` と `saturation.ceiling_limited` を確認する。
3. 問題なければ 500ブロックで本実行し、`docs/m13-puct-vs-mcts-2026-11-08.md` とは**別ファイル**に記録する（条件が違うため）。
4. アブレーションは `docs/m13-head-to-head-arms.json` を `paired_eval` の4シャードで回し、`eval_ablation.py` で集約する。

## 8. 限界

- 時間予算モードは壁時計依存で、同一SHA・同一seedでもビット単位一致は保証しない。再現性の主張は `simulation-capped` に限る。
- 4人戦の首位クレジットは同点 `1/k` を含む。単独1位率・期待順位・得点差を併記するが、判定は首位率を優先する。
- 相手AIを変えた条件（`greedy` 場と `mcts` 場）は別物であり、同じ表で比較しない。

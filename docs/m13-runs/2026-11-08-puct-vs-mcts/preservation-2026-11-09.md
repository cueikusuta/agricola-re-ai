# M13 評価データの長期保全（Issue #242、2026-11-09 実施）

正式PUCT対MCTS実験を含む期限付きGitHub Actions Artifact 3件（計21個）を失効前に保全し、
SHA-256・実行条件・コードSHA・Run IDを `run-manifest.json` に記録した。生JSONLを
`paired_eval_stats.py` で独立再集約し、数値・95%信頼区間・試合数が元レポートと一致することを検証した。

## 1. 保全対象

| Run | 分類 | 条件 | コードSHA | 失効日 |
|---|---|---|---|---|
| [#42515071537](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42515071537) | **正式** | 4人カードなし / PUCT(16sim, depth2) vs MCTS(16sim, depth2) mirror / simulation-capped / seed 965000..965249×250ブロック×1000試合/arm | `a0cd5301772d1a1de728b51493a66cc3b1afd082` | 2026-12-08 |
| [#42398823247](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42398823247) | 探索的 | 2人カードなし / PUCT vs MCTS, 共通相手greedy / time-budgeted 200ms | `1658c059b9d10ef9d3c9b2d49898615d0ad89493` | 2026-11-14 |
| [#42406723531](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42406723531) | 探索的 | 4人カードなし / PUCT vs MCTS, 共通相手greedy / simulation-capped 32sim | `1658c059b9d10ef9d3c9b2d49898615d0ad89493` | 2026-11-14 |

- 保存先: 本リポジトリ `raw-artifacts/*.zip`（21個、合計 約0.9MB／855,473バイト）。GitHub リポジトリ自体が恒久保管先であり、
  容量・権限の特別な制約は確認されなかった。生成物の作業コピーは `artifacts/`（gitignored）にもある。
- チェックサム: `raw-artifacts/SHA256SUMS.txt`（Artifact ZIP単位、GitHub REST API `/actions/artifacts/{id}/zip` から取得）。
- 取得日時: 2026-11-09（全Artifact `expired=false` をAPIで確認済み）。

## 2. 非公開情報の監査

`matches.jsonl` は試合単位の集計レコードのみを含む（座席別得点・順位・first credit、決定回数、
p50/p95/p99/max決定時間、探索設定フラグ、再演ハッシュ、公開用固定seed）。手札・山札・未公開カード等の
秘匿情報フィールドは存在しない（全キー73種を走査して確認）。`seed` は再演検証に必要な公開の固定評価seedであり、
AI観測から排除する内部seedの話（AGENTS.md規則7）とは別物である。カードなしのためカード原文も含まない。

## 3. 独立再集約（元レポートの再転記ではない）

方法: **16個のshard ZIP**（正式8＋旧2件各4）から展開した生JSONLを、`scripts/paired_eval_stats.py`
に固定条件を渡して再集約した。`paired-eval-pr-smoke` ZIP（2個）のdemo JSONLとreport ZIPは
再集約には使わない（スモーク用成果物のため）。

- 正式Run #42515071537: `--players 4 --seed-start 965000 --blocks 250 --candidate puct --baseline mcts --opponent mcts`
- #42398823247: `--players 2 --seed-start 900000 --blocks 500 --candidate puct --baseline mcts --opponent greedy --search-mode time-budgeted`
- #42406723531: `--players 4 --seed-start 930000 --blocks 250 --candidate puct --baseline mcts --opponent greedy --search-mode simulation-capped`

再集約物は `reaggregate/reaggregate-<run_id>-report.json`。結果:

| 値 | 元レポート | 再集約 |
|---|---|---|
| PUCT first credit | 87.8% | 87.8% |
| MCTS mirror first credit | 25.0% | 25.0% |
| 差 | +62.8pp | +62.8pp |
| 95%CI | [60.6, 64.9] | [60.6, 64.9] |
| 試合数 | 2,000（1,000/arm） | 2,000（1,000/arm、arm×250ブロック×4席で均等・重複なし） |

- 正式Runの再集約レポートは元レポートと**全フィールド一致**（差分ゼロ）。
- 探索的2件も数値は完全一致。差分は `comparison_design` と `saturation` の2フィールドのみで、
  これは両Runの後（#106, `700353d`）にレポートスキーマへ追加された項目であり、集計結果の不一致ではない。
- 完全性検査: 全行 `finished=true` かつ `replay_verified=true`、seed範囲 965000–965249、
  `(arm, block, seat)` の組2000種が丁度1回ずつ（`(arm, block)` 500種×4席）。
- 使用スクリプトのSHA対応: 再集約に使用した `scripts/paired_eval_stats.py` は正式Run時点
  （`a0cd530`）の版と**全く同一**（`git diff a0cd530 <branch> -- scripts/paired_eval_stats.py`
  は空）であることを確認した。bootstrapはseed 20261106で決定的。

## 4. ラベル付け（誤用防止）

`run-manifest.json` と本書は次の限界を明示する。新方策の強さ評価にこの数字を流用しないこと。

1. **同一思考時間ではない**: 正式Runは固定16 simulations / depth 2（simulation-capped）。壁時計時間の公平比較ではない。
2. **simulation-capped と time-budgeted は混ぜない**: 2人200msの結果（+1.0pp, CI [0.4, 1.7]）と4人16simの
   結果（+62.8pp）は別の実験であり、いかなる集計でも併合しない。
3. **カードなし・追加スペースOFF限定**: カード実装後の条件、4人カード、ドラフトは未実施。
4. **旧対照との歴史的成果**: +62.8ppは「現行PUCT vs MCTS(16sim)」の結果であり、Issue #219の
   「現行PUCT比+5pp」目標や将来の改良PUCT評価と混同しない。
5. **M14ゲート未達**: 本データはM13の記録であり、G1–G3のPASSを意味しない。

## 5. 再現方法

```bash
unzip raw-artifacts/run-42515071537-shard-0.zip -d /tmp/audit/42515071537/shard-0
# （残り7シャードも同様に展開）
python3 scripts/paired_eval_stats.py \
  --root /tmp/audit/42515071537 --players 4 --seed-start 965000 --blocks 250 \
  --candidate puct --baseline mcts --opponent mcts \
  --output /tmp/audit/reaggregate.json
# → raw-artifacts/SHA256SUMS.txt と reaggregate/ の報告値と比較
```

# M13/#106 評価変種の対照比較（4候補、すべて不採用・既定AIは不変）

実施日：2026-11-08。対象：カードなし、追加アクションスペースOFF。比較対象は既定の `EvaluationPolicy`（`eval`）。相手は全席 `eval`。

> 本ファイルは実験記録であり、強さの合否判定ではない。正本は `docs/roadmap-m7-onward.md` 3節、既存の敗因分析は `docs/ai-loss-analysis.md`。1候補目の `eval_veg` の詳細は `docs/m13-eval-veg-2026-11-08.md`。

## 1. 位置付け

`docs/ai-loss-analysis.md` の順位相関（2人戦の最大は `no_vegetables` −0.38、`late_third_worker` −0.20、4人戦は `begged` −0.16 / `no_animals` −0.19 / `empty_rooms` −0.18）から、設計仮説を変えた4つの変種を作り（うち `eval_room` は部屋と家族成長の2項目を同時変更する複合変種）、**同一seed・席順ローテーション・同一相手（eval）・同一計算予算（探索なし）**で対照比較した。既定AIは変更していない。

実装は `EvaluationVariant` と `VariantEvaluationPolicy`（`src/evaluation.rs`）。`paired_eval --candidate <arm>` で選択する。既定 `EvaluationPolicy` は `Baseline` のままで、既存テスト（`the_hoisted_baseline_keeps_the_per_action_ranking`）が挙動不変を固定している。

## 2. 候補と変更点

| arm | 変更内容 | 仮説の出所 |
|---|---|---|
| `eval_veg` | 野菜の1単位目を 7.0 → 8.0（穀物と同値） | `no_vegetables` |
| `eval_food` | 次回収穫の要求食料に +1 のバッファ | `begged` |
| `eval_veg_cond` | 調理手段がある、または食料が足りている場合のみ野菜1単位目を 8.0 | `eval_veg` の一律引き上げが2人戦で悪化したため、条件化で切り分け |
| `eval_room` | **2係数を同時変更する複合変種（厳密な1要因比較ではない）**：空き部屋の価値係数 12+残り → 18+1.5×残り、家族成長の係数 3.0 → 3.5 | `late_third_worker` / `empty_rooms` |

## 3. 方法

- **因果解釈の制約**：`eval_room` は housing と family_growth を同時変更したため、悪化した原因を部屋優先か増員優先かに分離できない。この候補は「成長重視の複合方策」として棄却するもので、1パラメータのアブレーション結果には数えない。厳密な1要因比較が必要なら、2項目を分けて別arm・別seedブロックで検証する。
- スクリーニング：2人戦 100ブロック×2席 = **各arm 200試合**、4人戦 50ブロック×4席 = **各arm 200試合**。seed 820000〜、対応差のpercentile bootstrap 10,000回（`scripts/paired_eval_stats.py`）。
- 全てローカル（Windows 11、release）。**M13正式受入（1,000試合/arm・未知seed・GitHub Actions）ではない。**
- 早期棄却規則：点推定が負、または95%区間上限が +5pp 未満で改善を示さない候補は、この段階で棄却し大規模評価へ進めない。**同一条件の大規模再実験は行っていない。**

## 4. 結果

### 2人戦（各arm 200試合）

| arm | 首位クレジット cand / base | 差 | 95%区間 | 期待順位 cand / base | 得点差 cand / base | 判定 |
|---|---|---:|---|---|---|---|
| `eval_veg`（既測、600試合/arm） | 0.4525 / 0.5000 | −4.75 pp | [−7.67, −1.92] | 1.547 / 1.500 | −0.773 / 0.000 | 棄却（有意悪化） |
| `eval_food` | 0.4050 / 0.5000 | −9.50 pp | [−15.5, −3.5] | 1.595 / 1.495 | −1.110 / 0.000 | 棄却（有意悪化） |
| `eval_veg_cond` | 0.4325 / 0.5000 | −6.75 pp | [−11.5, −2.0] | 1.565 / 1.495 | −0.835 / 0.000 | 棄却（有意悪化） |
| `eval_room` | 0.4450 / 0.5000 | −5.50 pp | [−12.0, +1.0] | 1.550 / 1.495 | −0.905 / 0.000 | 棄却（点推定が負・改善なし） |

### 4人戦（各arm 200試合）

| arm | 首位クレジット cand / base | 差 | 95%区間 | 期待順位 cand / base | 得点差 cand / base | 判定 |
|---|---|---:|---|---|---|---|
| `eval_food` | 0.2475 / 0.2500 | −0.25 pp | [−6.5, +5.75] | 2.470 / 2.500 | +0.317 / 0.000 | 棄却（差なし） |
| `eval_veg_cond` | 0.2750 / 0.2500 | +2.50 pp | [−0.5, +6.0] | 2.480 / 2.500 | −0.117 / 0.000 | 棄却（2人戦で有意悪化。4人戦の正の点推定は単独では採用根拠にしない） |
| `eval_room` | 0.1700 / 0.2500 | −8.00 pp | [−12.0, −4.0] | 2.865 / 2.500 | −2.923 / 0.000 | 棄却（有意悪化） |

処理時間：候補・基準とも探索なしのヒューリスティックで、4人戦のスループットは約21〜22試合/秒（このマシン）。1手10秒の上限とフォールバックは既存機構のままで、本変種は探索予算を持たない。

## 5. 結論

- **4候補すべて棄却。既定AIは変更しない。**首位率 +5pp の目標は未達であり、達成したとは記載しない。
- 野菜価格・食料バッファ・条件付き野菜価格という単一特徴の摂動と、**部屋価値＋家族増員の複合摂動**は、2人戦でいずれも**悪化**した。ただし複合変種の2係数は分解して評価していない。`eval_veg_cond` だけが4人戦で +2.5pp の点推定を示したが、2人戦で有意に悪化しており、モード依存の採用は根拠にしない。
- 示唆：ボトルネックは単一特徴の価格ではなく、**行動の連鎖（種→畑→収穫→給餌）と手番配分**にある可能性が高い。単発の重み変更では、序盤の食料・手番を犠牲にして局所価値を上げるだけになりやすい。
- 次に試すべき方向（未実施）：行動連鎖を明示的に評価する2段先読み（種の取得→播種の後状態）、または探索側（PUCT）の候補生成に播種・柵のマクロ行動を加える実験。いずれも本ファイルと同じ対応比較で判定する。

## 6. 再現手順

```bash
cargo build --release --locked --bin paired_eval
for ai in eval_food eval_veg_cond eval_room; do
  ./target/release/paired_eval --players 2 --seed-start 820000 --block-start 0 \
    --blocks 100 --candidate "$ai" --baseline eval --opponent eval \
    --additional-action-space false --output "target/screen-$ai-2p.jsonl"
  mkdir -p "target/sc2-$ai/shard-0"
  cp "target/screen-$ai-2p.jsonl" "target/sc2-$ai/shard-0/matches.jsonl"
  python scripts/paired_eval_stats.py --root "target/sc2-$ai" --players 2 \
    --seed-start 820000 --first-block 0 --blocks 100 \
    --candidate "$ai" --baseline eval --opponent eval \
    --output "target/sc2-$ai/stats.json"
done
```

4人戦は `--players 4 --blocks 50` に置き換える。

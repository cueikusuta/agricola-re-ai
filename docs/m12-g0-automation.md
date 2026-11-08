# G0 ゲートの自動化（#110 / #106）

M14 の G0 判定を **1回の手動実行**で「証拠生成 → 束縛 → 判定」まで通すワークフロー。2026-11-08 の初回判定（`docs/m12-g0-real-data-2026-11-08.md`）は `hold` で、理由は M8 証拠4項目の未提出だった。本ワークフローはその4項目を**同じコミット・同じ run の実行結果**から生成する。

## 1. 何をするか

`.github/workflows/g0-gate.yml`（`workflow_dispatch`）は次の順に動く。

| ジョブ | 内容 | Artifact |
|---|---|---|
| `m8-evidence` | `.github/workflows/m8-evidence.yml` を呼び、13ターゲットを実行して4検査の真偽値を導出 | `m8-evidence` |
| `dataset-plan` → `dataset-shard` → `dataset-aggregate` | 学習エピソードを分割生成し、全件を再演検証して1つの `episodes.jsonl` と `episodes-report.json` に集約 | `g0-dataset` |
| `eval-shard` (4並列) | `paired_eval` で seed ブロックを4分割し、各arm 1,000試合を生成 | `g0-matches-0..3` |
| `gate` | Manifest 作成 → `m8_evidence.py checks`（実行記録を matches へ束縛）→ `g0_evidence.py` → `dataset_split g0` | `g0-report` |

判定は `pass` のときだけジョブが成功する。`hold` / `fail` でも Artifact は保存するが、**ワークフローの結論は失敗**になる（未完了の証拠を緑で隠さない）。

## 2. 判定の意味

| 判定 | 意味 | ワークフロー |
|---|---|---|
| `pass` | 必要な証拠がすべて揃い、全検査が成功 | 成功 |
| `hold` | 必要な証拠が不足（未提出・未実行） | 失敗（Artifact あり） |
| `fail` | 実測または証拠に矛盾・不合格がある | 失敗（Artifact あり） |

`hold` と `fail` の内訳は `g0-report` の `checks[]` に検査IDごとに出る。`hold` の項目は「何を出せば判定できるか」を `detail` に書く。

## 3. 証拠の束縛（なぜ同一 run でなければならないか）

`checks_evidence.schema_version=2` は次の3つで束縛される。

1. `checks.code_sha` == `evidence.code_sha` == Manifest の `provenance.code_shas`（1値）。
2. `checks.matches_sha256` == 評価 matches 4ファイル連結の SHA-256（ゲートが再計算）。
3. `attestation.test_report_sha256` == 同一 run で実行したテスト結果のダイジェスト。

したがって**過去に成功した別 run の証拠を流用できない**。コードSHA・データSHAが変われば束縛が切れ、ゲートは `fail` を返す。再実行するときは同じ run で全ジョブを回す。

## 4. 実行方法

```sh
gh workflow run g0-gate.yml \
  -f episodes=1000 -f dataset_shards=4 -f dataset_policy=eval \
  -f dataset_seed_start=920000 \
  -f train_range=920000:920249 -f validation_range=920250:920399 \
  -f test_range=920400:920999 \
  -f players=2 -f blocks=500 -f eval_seed_start=920500 \
  -f candidate=eval -f baseline=random -f opponent=greedy \
  -f cards=none -f extra=false \
  -f search_mode=simulation-capped -f decision_budget_ms=0
```

- `blocks=500` × `players=2` 席 × 2arm = 各arm 1,000試合。
- `test_range` は評価 seed を必ず含む（評価 seed が train/validation に入るとゲートが `fail` する）。
- `candidate` / `baseline` は G0 の配管検証用であり、**強さの比較ではない**（`not_a_strength_claim: true`）。
- `search_mode=simulation-capped` のみG0判定対象。M8の「固定seed・固定探索回数で再現」は時間予算実行の方策決定再現性を保証しないため、`time-budgeted` はG0入力から除外し、`decision_budget_ms=0` のみ許可（#187 P2）。時間予算の性能評価はG0とは独立した統計評価として扱う。
- 標準の GitHub-hosted `ubuntu-24.04` のみ。GPU・有料ランナーは使わない。

## 5. 現在の状態（2026-11-08 時点）

- 自動化は実装済み。ローカルで検証した範囲:
  - 学習データ経路: 40エピソード（2シャード、seed 930000–930039）を生成し、全件 finished・破損0・重複0、`dataset_split manifest` が `usable: true`（train 10 / validation 6 / test 24、学習サンプル 3,207）。
  - 証拠導出: 13ターゲット・必須39テストを実行し、`all_checks_ok: true`。
  - ゲート: Rust 49テスト・Python 187テスト（`tests/m12_dataset_split.rs`, `tests/test_m8_evidence.py`, `tests/test_g0_evidence.py`）で pass / hold / fail の分岐と fail closed を確認。
- **実データによる G0 の再判定は未実施**。理由は本セッションの権限:
  - `gh workflow run`（Actions 起動）が拒否された。
  - `gh pr create`（PR 作成）が拒否された。
  - したがって「実際の GitHub Actions 実行 URL」を持つ証拠を作れず、**`pass` を主張できない**。`hold` のまま据え置く。
- ローカルで matches を合成して checks を作ることは**行っていない**（実行していないテストを証拠にしないため）。

## 6. 次に必要な作業

1. 権限のあるセッションまたは人間が上記コマンドで `g0-gate.yml` を実行する。
2. `g0-report` Artifact の `report.json` を確認し、`verdict` と `checks[]` を `docs/` に記録する。
3. `pass` の場合のみ G0 達成として報告する。`hold` / `fail` の場合は検査IDごとの不足をIssueへ残す。
4. G0 が `pass` しても、**大規模学習の開始は別判断**（`docs/ai-experiments-m12-m14.md` の G1〜G3）。

## 7. 既知の制約

- Artifact 保持は30日。長期監査で当時の matches を再計算する必要がある場合は、保持期間と容量を確認して別途保存する。
- `attestation` はテストの再実行ではなく、実行記録の完全性・内部整合・SHA 束縛の検証である（`docs/m8-evidence.md` 4節）。
- 4人戦・カードありは、M10 の適法カードプールが40+40に達していないため G0 の対象にしない（`cards=verified_basic_2p` は2人戦のみ）。


## #185 P1の証拠信頼境界

`m8-evidence` はこのワークフローから呼び出される同一runで必須テストを実行する。`actions/download-artifact@v4` は外部runを指定せず同一runの成果物だけを利用する。`m8_evidence.py checks --trusted-current-run` はGITHUB_SHA/REPOSITORY/RUN_IDに一致しないテスト結果を拒否する。Rustではコミットに取り込まれた要件ファイルから全必須テストID/件数/ダイジェストを再導出する。G0が成功したと言えるのは、これらの検査を通過したGitHub Actions実行とそのArtifactが確認できた場合のみ。

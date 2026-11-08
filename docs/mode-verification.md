# モード別シミュレーター検証（Issue #108 / #110）

`--batch` は最終得点の集計しか出さないため、**試合が再演できたか・各行動後に不変条件が成立したか・保存復帰したか・観測が秘匿されたか・同一seedで再現するか**を判定できない。本書は、カードなしの全サポートモード（2・3・4人 × 追加アクションスペースON/OFF）でそれを1試合ごとに検証する仕組みを説明する。

実装：

- `src/verification.rs`：1試合分の検証ロジックと機械可読な結果型
- `src/bin/mode_verify.rs`：seed範囲を回して `games.jsonl` と `summary.json` を出力するCLI
- `tests/mode_verification.rs`：必須CIに残す小さなスイープ（各モード2 seed）
- `.github/workflows/mode-verification.yml`：1,000試合目標の手動実行（通常PR CIとは分離）

## 1. 検証項目と、その限界

| チェック名 | 何を証明するか | 何を証明しないか |
|---|---|---|
| `completed_fourteen_rounds` | エンジンがラウンド14で終局し、行動ログが空でない | ルール解釈が公式に正しいこと |
| `final_state_invariants` | 終局状態が `Game::validate` を通る | 途中状態の不変条件（次項が担当） |
| `replay_matches_log` | `Game::replay` が全イベントを再適用し、前後ハッシュが一致する。`apply_recorded` は各行動後に `Game::validate` を呼ぶため、**行動ごとの不変条件**もここで検査される | ログに記録されない外部要因 |
| `save_restore_round_trip` | `save_json` → `load_json` が同じ状態ハッシュを返す。`load_json` 自身がイベント連鎖・版・初期手札を再検証する | ブラウザUIの永続保存（未実装） |
| `observation_hides_private_hands` | 各視点の `Observation` の手札が本人の手札と完全一致し、相手手札・未配布カード・`hidden_round_deck`/`rng` をJSONに含まない | AIが履歴から論理的に推定できる情報の制限（別途M11の観測履歴） |
| `seed_reproducible` | 同一設定・同一ポリシーで同じ最終ハッシュになる | ポリシー内部の壁時計依存（eval/greedy/randomは持たない） |

**1,000試合完走は安定性の試験であり、ルール正確性の証明ではない**（`docs/roadmap-m7-onward.md` 4.1）。本ツールは「完走した」だけでなく「再演・保存復帰・秘匿・再現が成立した」ことを要求するが、公式ルールとの照合は #108 の期待結果テストが担う。

検証は**カードなしのみ**を対象とする。カードありはM10の公式照合済みプール成立が前提であり、本ツールで任意のカードセットを「検証済み」と扱わない。

## 2. ローカル実行

```bash
cargo build --release --locked --bin mode_verify
./target/release/mode_verify \
  --players 4 --seeds 1000 --seed-start 700000 \
  --policy eval --out results/mode-4p-extra-false
# 追加アクションスペースONは --additional-action-space を付ける
# 分割実行は --shard 0 --shards 4（seedを index % shards で分割）
```

- 出力：`<out>/games.jsonl`（1試合1行の証拠）と `<out>/summary.json`（チェック別の合計・失敗一覧・平均時間・`GITHUB_SHA`）。
- 1試合でもチェックに失敗した場合、CLIは終了コード1を返す。
- 失敗は seed とチェック名で再現できる。同じ `--seed-start` と seed を指定すれば同じ試合になる。

## 3. GitHub Actions（大量実行）

`Mode-wide Simulator Verification` を手動実行する（`workflow_dispatch`）。

- 入力：`seeds`（各モードの試合数、既定1,000）、`seed_start`、`policy`。
- 6ジョブ（2/3/4人 × 追加タイルOFF/ON）が並列に走り、各モード同じseed範囲を使う。
- 各ジョブは `games.jsonl` / `summary.json` をArtifactへ保存し、`aggregate` ジョブが6モード全部の存在と `failed_games == 0` を検査する。1モードでも欠落・失敗があれば集約ジョブが失敗する。
- 通常PRの必須CIには含めない。必須CI側は `tests/mode_verification.rs` の小スイープ（12試合）だけを実行する。

## 4. 実行記録

### 4.1 ローカル予備（実装確認）

2026-11-08、Windows 11 / `cargo build --release`。4人・60 seed・eval・追加タイルOFFで60試合すべて6チェック合格、平均約102 ms/試合（再演・保存復帰・再試行を含む）。

### 4.2 ローカルで6モード × 1,000試合

同じ2026-11-08、Windows 11 / release、`--policy eval`、seed 730000〜730999（各モード同一範囲）。

| モード | 追加タイル | 試合数 | 合格 | 失敗 | 平均 ms/試合 |
|---|---|---:|---:|---:|---:|
| 2人 | OFF | 1,000 | 1,000 | 0 | 39.9 |
| 2人 | ON | 1,000 | 1,000 | 0 | 44.7 |
| 3人 | OFF | 1,000 | 1,000 | 0 | 62.7 |
| 3人 | ON | 1,000 | 1,000 | 0 | 79.9 |
| 4人 | OFF | 1,000 | 1,000 | 0 | 149.2 |
| 4人 | ON | 1,000 | 1,000 | 0 | 132.9 |

合計**6,000試合、チェック失敗0**。全試合で6チェック（完走・終局不変条件・再演/行動毎不変条件・保存復帰・観測秘匿・seed再現）が合格した。

この実行は**ローカルWindows機**であり、ロードマップが原則とするGitHub-hostedランナーでの実行ではない。ハードウェア・負荷が異なるため、ms/試合は性能保証ではない。

### 4.3 GitHub Actions（未実施）

`Mode-wide Simulator Verification` の `workflow_dispatch` は、本セッションでは**権限で拒否**され実行できなかった（大量評価の外部実行に承認が必要）。そのため公式な1,000試合のActions実行URL・SHA・Artifactは未取得である。実行できる環境では次を行う。

```bash
gh workflow run mode-verification.yml --ref main \
  -f seeds=1000 -f seed_start=700000 -f policy=eval
```

実行後、Run URL・実行SHA・モード別の失敗数を本節へ追記する。**未実行の結果をここへ書かない。**

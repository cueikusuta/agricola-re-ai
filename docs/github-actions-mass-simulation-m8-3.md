# M8-3 GitHub Actions大量対戦・評価基盤の詳細設計（Issue #110）

更新: 2026-11-06。**設計のみ。大量試合は未実行。** M8-1 #108（GLM担当）・M8-2 #109のルールエンジンを変更しない。

## 1. リポジトリの実装上の前提

- 既存の `.github/workflows/ci.yml` は `push(main/develop)`、PR、手動起動。変更検出後に rustfmt/clippy/cargo nextest/seeded benchmark を走らせ `CI required` で集約する。60秒目標を維持し、M8-3は**別workflow**。
- 現行 `src/main.rs`: `--batch N --players 2|3|4 --seed S` はカードなしバッチ（結果は集約表示）、`--benchmark N` / `--benchmark-cards N` は **2人戦専用**。 `src/benchmark.rs::compare_policies_in_mode` は固定seedにつき席交換2試合だが、出力は全体の `MatchSummary` に集約される。
- 4人カードあり、カード版、1試合1行の結果、再演ハッシュ、途中失敗seedと個々の思考時間、複数AIを組み合わせた4席ローテーションに必要な安定CLI/JSONLは **現在未提供**。既存 `--batch` の集約表示だけから信頼区間や異常seedを捏造してはならない。
- GitHub-hostedの標準CPU `ubuntu-24.04` を使用し、有料larger runner/self-hosted/GPUは使用しない。公開リポジトリの標準GitHub-hosted runnerはGitHub公式の無料対象。ただし **Artifact保存容量・保持期間等の上限** は別途監視する（[公式課金文書](https://docs.github.com/en/billing/concepts/product-billing/github-actions)）。

## 2. 専用ワークフロー（実装前の契約）

`.github/workflows/mass-simulation.yml`、`name: Mass Simulation (manual)`、`workflow_dispatch` のみ。デフォルトブランチで手動起動できるようにし、`permissions: contents: read`、`concurrency: group=mass-simulation-${{ github.ref }}-${{ inputs.experiment_id }}; cancel-in-progress: false` 相当で、同一experiment_idの重複起動を予防。実際のYAML式の構文・値制限はコード化時に検証。

### 入力（例）
| 入力 | 例 | 制約 |
|---|---|---|
| `experiment_id` | `m8-3-smoke-001` | 文字種・長さ制限。Artifact識別子 |
| `games_per_arm` | `1000` | 正整数。**座席数の倍数**、上限を設定 |
| `players` | `4` | 2/3/4（#108の合格モードのみ） |
| `cards` | `none` | `none/verified`、未対応を拒否 |
| `additional_action_space` | `false` | bool |
| `draft_mode` | `deal_7` | #104実装前はカードなし時 `none` のみ |
| `candidate_ai`, `baseline_ai`, `opponent_profile` | `M7` / `eval` / `eval×3` | **実装されたAI識別子のみ** |
| `seed_start` | `420000` | u64、連続範囲のオーバーフロー拒否 |
| `shards` | `4` | 1〜設定上限。実行枠と試合数から選択 |
| `decision_budget_ms` | `10000` | 対戦モードの上限。オフライン速度試験は別記 |
| `replay_check` | `true` | 再演検証の有無をManifestに記録 |

M8-3の初期1000試合は**各候補AIにつき1000**の意味。4人 × 250 seedブロック = 1000試合/arm、比較2armなら合計2000試合。2人なら500ブロック/arm ×2席。1モード・1比較条件に固定し、モード横断集計をしない。

## 3. データ分散・座席とseedの一意性

1. 評価条件を `condition_id = SHA256(players, ruleset, deck_version, opponent_profile, budget, engine_version)` で固定。各ジョブが取り扱うseedブロックを `block_index in [0, total_blocks)` とする。
2. `block_seed = seed_start + block_index`（算術オーバーフローは失敗）、シャード `shard_id = block_index % shards`。すべての `arm × seat (0..players-1)` を**同一seedブロック内**で実行。ペア/4席の組をシャード境界で分割しない。
3. 全4席で候補AIを1回ずつ配置し、残席の相手AIを固定。基準AIにも**同じseed×seat条件**を与え、独立の別シードを引かない。相手座席が同条件で比較可能になるよう、必要なら配置順も固定する。
4. 一意キー `(condition_id, block_index, arm_id, candidate_seat)`、再試行も同じキー・同じseed。集約時にキーの重複・欠落・条件違いをチェック。リトライによる二重計上は禁止。
5. 乱数の内部状態・未公開カードを**AIの観測に渡さない**。同一seedの利用は評価者/シミュレータ側に限定（AIの取得可能性を否定する非干渉テストを別途必須とする）。
6. `replay_check` は保存/復元・全ログ再演のハッシュ一致を確認し、ルール不正を「14ラウンド完走」で代替しない。

## 4. GitHub Actionsジョブ構成（依存が整ってから実装）

1. `plan`：入力バリデーション、#108対応モード、必要カード数/候補AI、ブロック数、シャード数、容量/時間上限を検証し、シャード定義とmanifest案を出力。値の検証失敗時は**成功扱いしない**。
2. `simulate`（matrix、標準CPU）：`cargo build --release --locked` またはSHA固定成果物を利用し、 `--evaluation-shard ... --format jsonl` 相当の**新CLI（後述）**で同一ブロックの試合を一括実行。 `timeout-minutes`、`fail-fast: false`、`set -euo pipefail`、出力カウント検証。個別失敗は `errors.jsonl` へ記録してジョブ失敗。各シャード `upload-artifact@v4`。
3. `aggregate`（`if: always()`）：全シャードArtifactの存在・結果・seed・一意キー・行数・リプレイハッシュを検証し、欠落/重複/失敗があれば赤判定。全件良好ならブロック単位の差・95%区間・必要数計画を機械生成、`report.json`/`report.csv` をArtifact保存。上流失敗時に `continue-on-error` で隠さない。
4. `report`：Run URL、Commit SHA、実行条件、CIへの影響、失敗seed一覧をStep Summaryに記録。Artifactsの `retention-days` は必要最短（例7日）、長期保管は別途判断。
5. 通常CIは既存 `ci.yml` のまま。PRへのRequired扱い、他PR上での自動キックやスケジュール実行を設定しない。

### 最小の新CLI/テスト契約（#109/#108と競合する間は実装しない）

`evaluate-shard --manifest PATH --shard-index I` のようなバイナリまたは独立CLIが、対戦処理と推論を交換可能な安定APIを介して呼び出す。入力は正規化JSON、出力は1試合1行のJSONL。**ルール合法性は既存エンジンの単一の検証器が決定**。カードあり4人戦等の未実装モードは `unsupported_mode` として即座に失敗する。外部からコマンド文字列を直接シェル実行させない。CLI改修・4人ポリシーのシート割当は #108/#109 のAPI契約を確定して別Issue/PRに切る。

## 5. Artifactスキーマ（版管理、最小項目）

- `manifest.json`: `schema_version`, `experiment_id`, `condition_id`, `engine_sha`, `ruleset_version`, `deck_id/hash`, `AI_id/hash`, `opponent_profile`, `mode`, `players`, `seed_start`, `blocks`, `arm_ids`, `shard_id`, `host_os`, `cpu_model`, `logical_cores`, `rustc_version`, `build_profile`, `decision_budget_ms`, `run_id/attempt`, `started/finished_at`.
- `matches.jsonl`/ `matches.csv`: `block_index`, `seed`, `arm_id`, `candidate_seat`, `rounds_completed`, `finished`, `scores_by_seat`, `rank_by_seat`, `tie_count`, `first_place_credit`, `candidate_score`, `opponents_mean_score`, `wall_ms`, `decisions`, `replay_hash`, `error_code`. 同率首位`k`人の候補に `1/k`、単独首位率も別項目。
- `errors.jsonl`：`seed/block/arm/seat`、失敗カテゴリ、検証段階、最後のイベント・再現コマンド。秘密カード値・個人情報は不要。
- `metrics.json`：`games/sec`, `decisions/sec`, `mean/p50/p95/p99/max_decision_ms`, `timeout_rate`, `fallback_rate`, `cpu_seconds`, `wall_seconds`.
- `report.json`：完走率、重複/欠落数、新旧の首位クレジット・期待順位・得点差、差の95%CI、再計算した追加ブロック数、統計手法・再抽選seed/反復数、元Run URL。CSV列とJSON字段は仕様テストで固定。

## 6. 統計・必要試合数

- **一次指標**：首位クレジット（同率1位は1/k）の新旧差。補助：期待順位・平均/分布得点差・計算コスト。対戦相手構成・モードを揃え、異なる評価条件を無条件に混合しない。
- 同一 `seed×seat` の新旧差を求め、同一seedブロックの全席をまとめた `d_b` を主標本としてブロック・ブートストラップ（例固定10,000回、固定bootstrap seed）で両側95%区間。**4席を4つの独立seedと扱わない**。ブロックサイズと観測SD、反復法、事前に定めた判定時点を保存。
- 初期 1000 games/arm は4人なら250独立seedブロック（完全相関時は実効サンプル数が試合数より小さい）。補助として各armの首位クレジット95%区間も出す。
- +5ポイントを **点推定で達成かつ改善差の95%区間下限>0** のとき暫定採用。区間が0を跨げば保留。下限>+0.05を必須とする強い主張とは区別。
- 参考独立二群近似（80%検出力・両側α=.05・差=.05）は、4人戦 `p0=.25, p1=.30` で各arm **約1,250試合**、`p0=.50, p1=.55` で約1,560試合。ただし座席/seed相関とペア化の効果で大きく変わるため、固定必要量ではない。現実の必要ブロック数は事前に決めた検出力で `ceil(((1.96+0.84)*SD(d_b)/0.05)^2)` として**実測後**再計算（最低ブロック数や最大追加数も事前宣言）。
- 途中確認で追加試験を無制限に繰り返さない。例：初期1000→事前宣言の1回追加→最終判定。適応型なら逐次検定のα消費を別途設計。

## 7. 概算コスト・リスク・導入ゲート

- 実行コスト `CPU time ≈ num_games * measured_cpu_seconds_per_game`、壁時計 `≈ build/startup + max(shard_runtime) + aggregate`。現在の実測値がないため**秒数/無料枠消費を断定しない**。まず4〜16 seedブロック/1〜2 shardのスモークを行い、実測に基づいて1000試合の所要・ジョブ数を決める。
- 公開repoの標準ホストCPUランナーでの実行料金は無料対象（公式文書に従う）が、利用枠、同時実行、Artifact容量・保持期間、悪用防止制限は存在する。有料GPU/larger runner禁止。
- クォータ回避・無限実行を防ぐため `games_per_arm`, `shards`, `timeout-minutes`, 最大Artifact容量、`concurrency` の上限と操作権限を設定。
- M8-1モード別公式期待結果試験・M8-2合法入力検証が通るまで「正確なシミュレータ」と主張しない。
- 1手10秒は**人間対戦用の別受入試験**で全手に適用。オフライン大量バッチでは処理速度のため別予算で試すことがあり、その結果を10秒制約の成功と混同しない。

## 8. 実装の順序と合格条件

1. **設計PR（本PR）**：モード/JSONL/統計/seed分散/失敗条件を確定。
2. #108/#109のAPI確定後にCLI契約と最小2人・カードなしの1試合JSONL化。独立Issue/PRで実装・固定seedの単体テスト。
3. 計画/集約スクリプト単体テスト（異常行、重複、欠落、hash違い、統計固定seed）、1〜2 shardのActionsワークフローの実ラン結果をPRに添付。
4. 4人カードなしで4席×新旧AI、カードあり/ドラフトは#105/#104の対応完了後に個別拡張。公式検証済みモードごとに1000試合→異常0/再演一致→差と区間を報告。
5. `CI required` の60秒目標と大量実行のRunを分離したまま、通常CIに性能低下・待ち行列への影響がないか監査。

**未完了**：`evaluate-shard` CLI、実ジョブ、Artifact、性能測定、CIリンク、1000試合の結果はまだ存在しない。本PRの文書にそれらの合格実績を記入しない。
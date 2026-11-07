# M12 学習データ分割と M14 G0 ゲート（Issue #167 C）

更新: 2026-11-07。対象: `src/dataset_split.rs`（新規）、`src/bin/dataset_split.rs`（新規）、`tests/m12_dataset_split.rs`（新規）、`docs/episode-dataset-pipeline.md`。

**本ドキュメントは分割・ゲートの実装契約と小規模な動作確認の記録。学習は開始していない。G0 の合格を本番データで主張していない。**

## 1. 解いた問題

- `docs/ai-experiments-m12-m14.md` 2節は train/validation/test を分け、seed 値の範囲だけでなく生成器バージョンとデッキ版でも区別し、同一初期状態の漏れを検査することを求めている。既存の `src/episode_data.rs` は1試合の記録・再演検証・観測のみの `TrainingSample` を持つが、**分割を機械可読に固定し、漏洩を拒否する層が無かった**。
- 同 6節 G0 は「M8/M12 の正確性・再現性・未知seed評価・データ版管理」を学習開始前のゲートとする。合否を**機械可読**に残す仕組みが無かった。
- したがって本PRは **(a) 分割Manifest** と **(b) G0 判定レポート** を追加する。学習・モデル・大規模試合は含まない。

## 2. 再利用した既存基盤

| 既存 | 本PRでの使い方 |
|---|---|
| `episode_data::EpisodeRecord` / `from_jsonl` | 分割対象。1試合1行の JSONL をそのまま入力にする |
| `EpisodeRecord::verify()` | 分割前に全件を再演検証。1件でも不一致なら Manifest を作らない |
| `episode_data::inspect_dataset` / `DatasetReport` | 件数・完走・重複seed・破損・provenance を G0 の入力にする |
| `EpisodeRecord::training_samples(viewer)` / `TrainingSample::assert_public()` | 非公開情報の混入検査。全席・全ターンで実行する |
| Episode API（`record_self_play` → `record_episode` → `api::Episode`） | 分割の入力データは Episode API 経由で記録された `EpisodeRecord`。本モジュールはその再演検証結果を信用の前提にする |

## 3. 分割Manifest（`src/dataset_split.rs`）

```rust
pub struct SplitPlan { train: (u64,u64), validation: (u64,u64), test: (u64,u64) }  // 両端含む
pub fn build_manifest(records: &[EpisodeRecord], plan: SplitPlan)
    -> Result<DatasetSplitManifest, String>
```

`build_manifest` は次を**すべて**満たさなければ `Err` を返す（fail closed）。呼び出し側が「検証済み」と誤認できる Manifest を受け取らない。

| 検査 | 拒否する内容 |
|---|---|
| 範囲の非重複 | `SplitPlan` の範囲が重なる（1つの seed が2分割に入り得る） |
| seed の帰属 | どの範囲にも入らない seed。**既定の分割へ黙って落とさない** |
| seed の一意性 | データセット内に同じ seed が2回 |
| 再演一致 | `EpisodeRecord::verify()` が失敗する記録（初期状態ハッシュ・行動列・終局ハッシュ） |
| provenance の一様性 | `format` / `engine_version` / `rules_version` / `card_mode` / カードセット内容ハッシュ / `schema_version` / `code_sha` / AI版のいずれかが複数値 |
| カード条件 | `none` / `verified_basic` 以外のカードモード（未検証カードを学習データへ混ぜない） |
| 空の分割 | 3分割のいずれかが0試合、または学習サンプル0件 |
| 初期状態の分離 | 同じ `initial_state_hash` が2つ以上の分割に現れる |
| 非公開情報 | いずれかの席の `TrainingSample` が `assert_public()` に失敗する |

`initial_state_hash` は seed と別に検査する。seed が違っても同じ初期局面なら test は「未知の局面」ではないため。

Manifest には分割ごとの `seeds` / `episodes` / `training_samples` / `initial_state_hashes`、データセット全体の provenance、`private_sample_leaks`、`issues` を記録する。`seed_isolation_problems()` と `initial_state_isolation_problems()` は**読み込んだ Manifest を再検査**する（ファイルは編集され得るため）。

## 4. G0 判定（`evaluate_g0`）

入力は Manifest・`DatasetReport`・任意の evidence JSON。

| 判定 | 条件 |
|---|---|
| `pass` | 全チェックが pass。**データ側の検査だけでは決して pass にならない** |
| `hold` | 失敗は無いが必要な証拠が無い（evidence 未指定を含む） |
| `fail` | データか証拠に矛盾がある |

データ側チェック（Manifest と `DatasetReport` から機械的に決まる）:

| id | pass 条件 |
|---|---|
| `dataset_integrity` | `corrupt` 空・`duplicate_seeds` 空・`total == finished` |
| `split_manifest_valid` | Manifest に issue も私的サンプル漏洩も無い |
| `provenance_uniform` | engine/rules/cards/code が各1値 |
| `seed_isolation` | 同一 seed が2分割に無い |
| `initial_state_isolation` | 同一初期状態が2分割に無い |
| `public_training_samples` | 全サンプルが公開情報のみ |
| `verified_card_condition` | カードモードが検証済み条件 |

証拠側チェック（evidence JSON がある場合のみ判定。無ければ全て `hold`）:

| id | pass 条件 |
|---|---|
| `m8_rule_evidence` | `m8_1_rule_evidence: true` |
| `m8_legal_api_evidence` | `m8_2_legal_api_evidence: true` |
| `evaluation_scale` | 検証済みカード条件の各モードで `games_per_arm >= 1000` |
| `evaluation_replay` | 各モードの `replay_mismatches == 0` |
| `evaluation_legality` | 各モードの `legal_action_violations == 0` |
| `evaluation_observation_noninterference` | 各モードの `observation_noninterference_passed == true` |
| `evaluation_ci_reproducible` | 各モードの `ci_reproducible == true` |
| `evaluation_anomalies` | 全モードの `anomalies == 0`（失敗時のみ現れる） |
| `artifact_traceable` | evidence の `code_sha` が Manifest の `code_shas` と一致し、全モードに `artifact_url` がある |

- 未検証カード条件のモードは `evaluated_modes` に入れず、そういうモードしか無ければ `evaluation_*` は **hold**。未検証カードを本番に混ぜた場合はデータ側 `verified_card_condition` で **fail**。
- レポートは `verdict` / `checks[]`（`id`・`status`・`source`・`detail`）/ `dataset_episodes` / `dataset_training_samples` / `evaluated_modes` / `not_a_strength_claim: true` / `next_action` を持ち、そのまま Artifact にできる。

### evidence JSON スキーマ

```json
{
  "schema_version": 1,
  "code_sha": "<生成コードのSHA。Manifest と一致必須>",
  "m8_1_rule_evidence": true,
  "m8_2_legal_api_evidence": true,
  "modes": [
    {
      "mode": "cardless_2p",
      "players": 2,
      "card_mode": "none",
      "games_per_arm": 1000,
      "anomalies": 0,
      "replay_mismatches": 0,
      "legal_action_violations": 0,
      "observation_noninterference_passed": true,
      "ci_reproducible": true,
      "artifact_url": "https://github.com/.../actions/runs/..."
    }
  ]
}
```

## 5. CLI

```text
dataset_split manifest --in episodes.jsonl --out split-manifest.json \
    --train 100:499 --validation 500:649 --test 650:799
dataset_split g0 --manifest split-manifest.json --dataset-report report.json \
    [--evidence g0-evidence.json] --out g0-report.json
```

- `manifest` は分割が使えないとき **exit 2**（`split ranges overlap`、`seed ... outside every declared split range` など）。
- `g0` は `pass` のみ **exit 0**、`hold` / `fail` は **exit 1**。ゲート未成立を成功扱いにしない。
- 範囲は両端を含む `LOW:HIGH`。片方だけ広げる誤用を避けるため、範囲外 seed は既定値に落とさずエラーにする。

## 6. 小規模な動作確認（2026-11-07、ローカルWindows、release）

**学習は行っていない。** 以下は配管とゲート判定の確認であり、G0 の本番合格ではない。

```sh
episode_export --episodes 4 --seed 100 --players 2 --out episodes.jsonl --code-sha localsha
episode_inspect --in episodes.jsonl --report report.json
dataset_split manifest --in episodes.jsonl --out manifest.json \
    --train 100:100 --validation 101:101 --test 102:103
dataset_split g0 --manifest manifest.json --dataset-report report.json --out g0.json
```

| 実行 | 結果 |
|---|---|
| `manifest` | `usable: true`、train 1試合 / validation 1試合 / test 2試合、学習サンプル 316 件、provenance 各1値 |
| `g0`（evidence なし） | `hold`、pass 7 / fail 0 / hold 8、**exit 1** |
| `g0`（evidence あり・`code_sha: localsha`） | `pass`、pass 15 / fail 0 / hold 0、exit 0 |
| `manifest`（範囲を重ねる） | `split ranges overlap: [(Train, Validation)]`、exit 2 |
| 重複 seed・範囲外 seed・provenance 混在・未検証カードモード・破損記録 | `tests/m12_dataset_split.rs` の各テストで拒否を固定 |

**重要**: 上の `pass` は evidence JSON の各項目を満たした場合にゲートが `pass` を返すことの確認であり、**1,000試合/arm の実測や M8-1/M8-2 の証拠を提出した意味ではない**。本番の G0 判定は、Actions の実測 Artifact と M8 の証拠を evidence JSON にまとめて実行する。

## 7. テスト

- `tests/m12_dataset_split.rs`：17件。分割の正常系（JSON round-trip、2人・4人）、範囲重複、範囲外 seed、重複 seed、provenance 混在、未検証カードモード、破損記録、非公開情報の非混入、初期状態の共有検出、編集された Manifest の再検査、G0 の hold/fail/pass 条件（証拠なし・短い試合数・SHA不一致・M8証拠欠落・不正手・未対応カードモード・破損データ）。
- 既存 `tests/m12_episode_data.rs` / `tests/m12_api.rs` / `tests/m12_episode_api_bridge.rs` は変更なしで成功（`src/episode_data.rs` と Episode API を変更していない）。
- `cargo fmt --check` / `cargo clippy --all-targets` / `cargo test` を最新HEADで確認。

## 8. 未解決・範囲外

1. **本番の G0 判定は未実施。** evidence JSON はまだ実データで埋まっていない（1,000試合/arm のモード別実行と M8-1/M8-2 の証拠が必要）。したがって「G0 合格」とは主張しない。
2. **大規模学習は開始しない。** G0 が `pass` になるまで G1（教師信号の価値）へ進まない方針を維持する。
3. 分割は seed 範囲の宣言のみで、割合指定・層化（カードモード別・人数別）は未提供。モード混在は provenance 一様性チェックにより**拒否**されるため、モード別に別データセットを作る前提である。
4. カードあり条件（`verified_basic`）のデータ生成・分割は、2人戦のプールに限り既存 `episode_export --cards` で可能だが、G0 の本番 evidence は未整備。
5. ドラフト込みエピソードの分割は、`DraftEpisode` を持つ記録でも同じ経路で扱えるが、専用テストは未追加（#155 の慣行確定後に拡張）。

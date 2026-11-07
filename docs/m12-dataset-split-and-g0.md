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

入力は Manifest・`DatasetReport`・任意の evidence JSON（schema v2）と、**evidence が指す matches ファイルの実体**。判定は次の3値。

| 判定 | 条件 |
|---|---|
| `pass` | 全チェックが pass。データ側の検査だけでは**決して** pass にならない |
| `hold` | 失敗は無いが必要な証拠が無い（evidence 未指定・matches 未供給・attestation 未提出を含む） |
| `fail` | データか証拠に矛盾がある（捏造・改変・条件不一致・試合数不足など） |

### 4.1 レビューP1の修正：自己申告では合格しない

旧版は `games_per_arm: 1000` や真偽値を evidence に書けば合格し得た。**現在は scale・再演・完走・ブロック完全性を matches ファイルから再計算し、digest を照合する。** evidence に「試合数」を書く欄は無い。

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

証拠側チェック（実ファイルから再計算）:

| id | 内容 |
|---|---|
| `evidence_schema` | `schema_version == 2`。旧 v1（申告型）は **fail** |
| `evaluation_mode_conditions` | 重複モード・未検証カード条件・Manifest の条件/人数との不一致を **fail**。dataset の条件をカバーするモードが1つも無ければ **hold**（暗黙の除外を通さない） |
| `evaluation_matches_loaded` | 宣言された matches が読めない/未供給なら **hold**、読めても壊れていれば **fail** |
| `evaluation_matches_digest` | 宣言 `matches_sha256` と実ファイルの SHA-256 が一致しなければ **fail** |
| `evaluation_scale` | **行から再計算**した games/arm が 1,000 未満なら **fail**（「plumbing test であり G0 pass ではない」と detail に明記） |
| `evaluation_completion` | `finished` でない行・14 ラウンドでない行があれば **fail** |
| `evaluation_replay` | `replay_verified` でない行があれば **fail** |
| `evaluation_block_completeness` | 各ブロックが **全座席 `0..players-1` × 両arm を各1行** 持つことを要求。座席欠落・範囲外座席・重複・`rows != blocks × players × 2` で **fail** |
| `evaluation_seed_blocks` | 1ブロック1seed・別ブロックでseed重複なし・`seed - block_index` のオフセットが全行で同一、を要求（再レビューP1-b） |
| `evaluation_split_discipline` | 評価 seed が **ロックされた test 範囲の外**（train/validation を含む）なら **fail** |
| `checks_attested` / `m8_*` / `evaluation_observation_noninterference` / `evaluation_ci_reproducible` | checks ファイルが **hash で束縛**され、その中の `code_sha` と `matches_sha256` が evidence と一致するときだけ判定。無ければ **hold**（未検証の主張は合格に使わない）。束縛が壊れていれば **fail** |
| `artifact_traceable` | `artifact_url` が空、または `code_sha` が Manifest と不一致なら **fail** |

### 4.2 seat rotation と seed ブロックの扱い

`paired_eval` の1行は **(seed block, arm, seat) の1試合**である。したがって1つの seed ブロックには `players × 2 arms` 件のゲームが属するのが正常であり、「1 seed = 1 試合」と仮定しない。

- `evaluation_block_completeness` は `(block, arm, seat)` が各1回ずつ存在し、**座席集合が `0..players-1` と完全一致**し、総行数が `blocks × players × 2` と一致することを要求する（2人戦=4行/ブロック、4人戦=8行）。1席欠け・範囲外座席・重複行は fail。
  - 以前は「実際に現れた座席数」を基準にしていたため、**seat 0 だけを1,000ブロック並べると「1,000 games/arm・完全」と判定できた**（再レビューP1-a）。現在は宣言された人数を基準にする。
- `evaluation_seed_blocks`（再レビューP1-b）は次を要求する。
  - 同じブロックの全行が同一 seed。
  - 異なるブロックは異なる seed（同じ seed の使い回しは fail）。
  - `seed - block_index` が全行で同一（`paired_eval` は `seed = seed_start + block_index` で書くため、1実験は1オフセット）。範囲がずれた手編集データは fail。
  - したがって「1 seed を 1,000 行に使い回して 1,000 独立試合と数える」経路はない。正しい 1,000/arm は 2人戦で 500 ブロック、4人戦で 250 ブロックになる。
- `evaluation_scale` は「1 arm あたりの行数」で数える（= ブロック数 × 席数）。
- `DatasetSplitManifest.seat_rotation_note` と `player_counts` にこの契約を記録し、G0 は `player_counts` でモード条件を照合する。
- 分割は seed 単位であり、**同一ブロックの席を分割をまたいで分けない**。

### 4.3 evidence JSON スキーマ（v2）

```json
{
  "schema_version": 2,
  "code_sha": "<生成コードのSHA。Manifest と一致必須>",
  "modes": [
    {
      "mode": "cardless_2p",
      "players": 2,
      "card_mode": "none",
      "artifact_url": "https://github.com/.../actions/runs/...",
      "matches_sha256": "<matches_paths を連結した SHA-256。ゲートが再計算して照合>",
      "matches_paths": ["shard-0001/matches.jsonl", "shard-0002/matches.jsonl"],
      "checks_path": "checks.json",
      "checks_sha256": "<checks ファイルの SHA-256>"
    }
  ]
}
```

- `matches_paths` は **evidence ファイルからの相対パス**として解決する。`scripts/g0_evidence.py` がその形で書き出す。
- `checks_path` / `checks_sha256` は任意。無い場合、attestation 依存の4チェックは `hold` になり、全体も `pass` にならない。
- checks ファイルは **`schema_version == 2` 必須**（再レビューP2）。未知バージョンは `checks_attested` で **fail**。
- checks ファイル（schema v2）は `code_sha`・`matches_sha256`・`m8_rule_evidence`・`m8_legal_api_evidence`・`observation_noninterference_passed`・`ci_reproducible` を持つ。

### 4.4 evidence の生成（手入力の排除）

```sh
python scripts/g0_evidence.py \
  --mode cardless_2p --players 2 --card-mode none \
  --matches <run>/shard-0001/matches.jsonl [--matches <...>/shard-0002/matches.jsonl] \
  --artifact-url https://github.com/.../actions/runs/<id> \
  --code-sha "$(git rev-parse HEAD)" \
  --out <work>/evidence.json
```

生成器は SHA-256 を**実ファイルから**計算し、`(block, arm, seat)` の重複を検出したら失敗する。手で数値を書く経路を残さない。

## 5. CLI

```text
dataset_split manifest --in episodes.jsonl --out split-manifest.json \
    --train 100:499 --validation 500:649 --test 650:799
dataset_split g0 --manifest split-manifest.json --dataset-report report.json \
    [--evidence g0-evidence.json] --out g0-report.json
```

- `manifest` は分割が使えないとき **exit 2**（`split ranges overlap`、`seed ... outside every declared split range` など）。
- `g0` は `pass` のみ **exit 0**、`hold` / `fail` は **exit 1**。ゲート未成立を成功扱いにしない。
- `g0` は evidence の `matches_paths` / `checks_path` を evidence からの相対で読み、再計算に使う。
- 範囲は両端を含む `LOW:HIGH`。片方だけ広げる誤用を避けるため、範囲外 seed は既定値に落とさずエラーにする。

## 6. 小規模な動作確認（2026-11-07、ローカルWindows、release）— レビューP1後の再実施

**学習は行っていない。G0 の本番合格ではない。** 以下は「偽の証拠では合格しない」ことを実データで確認した記録である。

```sh
# 分割対象の学習データ（4試合）
dataset_split manifest --in episodes.jsonl --out manifest.json     --train 100:101 --validation 110:110 --test 120:1600
# 評価アーティファクト（1ブロック・2人戦 = 各arm 2試合）
paired_eval --players 2 --seed-start 911 --block-start 0 --blocks 1 ...
# evidence は実ファイルから生成する（数値を手入力しない）
python scripts/g0_evidence.py --mode cardless_2p --players 2 --card-mode none     --matches run/shard-0001/matches.jsonl --artifact-url https://example.invalid/run/1     --code-sha test-sha --out evidence.json
dataset_split g0 --manifest manifest.json --dataset-report report.json     --evidence evidence.json --out g0.json
```

| 入力 | 判定 | 根拠（fail / hold の id） |
|---|---|---|
| evidence なし | `hold`（exit 1） | 証拠系11チェックが hold |
| 実データ 4行（各arm 2試合）＋実 evidence | **`fail`（exit 1）** | `evaluation_scale`: `2 games/arm recomputed from 4 rows (1000 required)`。**「plumbing test」であり pass にしない** |
| 同上で `matches_sha256` を `0…0` に改変 | **`fail`（exit 1）** | `evaluation_matches_digest` |
| 旧 v1 evidence（`games_per_arm: 1000` を手書き） | **`fail`（exit 1）** | `evidence_schema`（`schema_version` が 2 でない） |
| 実データ＋実 evidence（attestation なし） | `fail`（exit 1） | `evaluation_scale` に加え `m8_rule_evidence` ほか4件が hold |
| 1,000試合/arm 相当の合成行＋checks ファイル束縛 | `pass`（テスト内） | 数値は行から再計算。**実評価の代用ではない** |
| 500ブロック×2席（正しい回転）、attestation なし | `hold`（exit 1） | fail なし。checks ファイルが無いため pass にしない |
| **1,000ブロック×seat 0 のみ**（再レビューP1-a の反例） | **`fail`（exit 1）** | `evaluation_block_completeness`（`evaluation_scale` は 1000/arm で pass してしまう） |
| **全ブロックが同一 seed**（再レビューP1-b の反例） | **`fail`（exit 1）** | `evaluation_seed_blocks` + `evaluation_block_completeness` |
| 1ブロック内で arm ごとに別 seed | **`fail`（exit 1）** | `evaluation_seed_blocks`（mixed seeds） |
| `seed = 2 × block + 120`（オフセット不整合） | **`fail`（exit 1）** | `evaluation_seed_blocks`（offset conflicts） |
| checks ファイルの `schema_version: 1` | **`fail`（exit 1）** | `checks_attested`（`checks schema 1 is not 2`） |

**重要**: 最後の `pass` は `tests/m12_dataset_split.rs` の合成行（500 ブロック × 2 席 × 2 arm）で、ゲートの算術と hash 束縛が成立することを示すだけである。実試合 1,000/arm の証拠ではなく、**本番G0合格ではない**。

## 7. テスト

- `tests/m12_dataset_split.rs`：**39件**。分割の正常系（JSON round-trip、2人・4人、`player_counts`）、範囲重複、範囲外 seed、重複 seed、provenance 混在、未検証カードモード、破損記録、非公開情報の非混入、初期状態の共有検出、編集済み Manifest の再検査、**再計算（scale・ブロック完全性・重複・未再演・切り詰め行）**、**座席ローテーション（seat 0 のみ／4人戦の1席欠け／範囲外seat）**、**seed ブロック（seed 使い回し／同一ブロック異seed／オフセット不整合）**、**checks スキーマ（v1 拒否・v2 受理）**、G0 の hold/fail/pass（旧 v1 スキーマ拒否・少数試合の fail・digest 捏造・checks 束縛切れ・モード重複・条件不一致・未検証カード・train seed での評価・未再演行・破損データ）。
- `tests/test_g0_evidence.py`：**8件**。digest が実ファイルから計算されること、シャード連結、`(block, arm, seat)` 重複の拒否、未検証カードモード拒否、checks の digest 記録、**evidence に試合数を書く欄が無いこと**。
- 既存 `tests/m12_episode_data.rs` / `tests/m12_api.rs` / `tests/m12_episode_api_bridge.rs` は変更なしで成功（`src/episode_data.rs` と Episode API を変更していない）。
- `cargo fmt --check` / `cargo clippy --all-targets` / `cargo test` を最新HEADで確認。

## 8. 未解決・範囲外

1. **本番の G0 判定は未実施。** 1,000試合/arm のモード別実行と、M8-1/M8-2・観測非干渉・CI 再現の checks ファイルがまだ無い。したがって「G0 合格」とは主張しない。
2. **attestation は hash で束縛するが署名ではない。** checks ファイルを丸ごと捏造すれば理論上は通る。実運用では Actions のログ/Artifact と突き合わせる前提であり、暗号学的な保証は持たない（必要なら別途署名を検討）。
3. **評価 seed のロックは範囲で判定する。** test 範囲（例 `120:1600`）を宣言し、その外の seed での評価を fail にする。学習データに含まれる test 試合数より評価件数が多いことは正常。
2. **大規模学習は開始しない。** G0 が `pass` になるまで G1（教師信号の価値）へ進まない方針を維持する。
3. 分割は seed 範囲の宣言のみで、割合指定・層化（カードモード別・人数別）は未提供。モード混在は provenance 一様性チェックにより**拒否**されるため、モード別に別データセットを作る前提である。
4. カードあり条件（`verified_basic`）のデータ生成・分割は、2人戦のプールに限り既存 `episode_export --cards` で可能だが、G0 の本番 evidence は未整備。
5. ドラフト込みエピソードの分割は、`DraftEpisode` を持つ記録でも同じ経路で扱えるが、専用テストは未追加（#155 の慣行確定後に拡張）。

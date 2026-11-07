# M12 エピソードデータ生成・検証パイプライン（Issue #106）

更新: 2026-11-07。**データ整合性・完走・再演の基盤。勝率・強さ・95%区間の主張は含まない。**

## 1. 目的と範囲

`src/episode_data.rs`（PR #154）が定義する `EpisodeRecord` を大量に生成し、各レコードを再演検証して、機械可読な Artifact として残す。これは M12「評価・学習環境の本格整備」のうち、**データ生成と完全性検証**を担当する。新旧AIの首位率比較・95%信頼区間・必要試合数の再推定は `paired-eval` ワークフロー（PR #146 / #110）の担当であり、本パイプラインはそれを行わない。

- 生成対象: カードなし／検証済み2人カードプールの自己対戦。3・4人カード戦とドラフト込みは、公式照合済みプール（#105/#115/#139）とドラフト配布（#104）が前提のため**対象外**。未検証カードを混ぜない。
- 生成AI: 空間行動は `eval` / `greedy` / `random` のいずれか（全席同一）。詳細決定はカードなし `SimpleDetails`、カードあり `CardChoiceDetails`。
- 出力は1試合1行の JSONL。`seed`・イベント列・再演ハッシュを含む**信頼できるコーディネータ側データ**であり、学習入力 `TrainingSample` とは別物（他人の手札・内部seedを含まない）。

## 2. 実行経路

`.github/workflows/episode-data.yml`（`workflow_dispatch` のみ。通常PRの CI required には含めない）。

| ジョブ | 内容 |
|---|---|
| `unit` | PRでも実行。`scripts/episode_dataset.py` の計画・集約単体テスト（ネットワーク不要）。 |
| `plan` | 入力検証と、重複しない seed ブロックの分割を出力。 |
| `shard` (matrix) | 各シャードで `cargo build --release --locked --bins` 後、`episode_export` で生成し、`episode_inspect` で全件再演検証。 |
| `aggregate` | 全シャードArtifactを検証し、件数・seed被覆・provenance を突合して `summary.json` を保存。 |

入力: `episodes`（1..20000、`shards`で割り切れること）、`players`（2/3/4）、`shards`（1..16）、`policy`（eval/greedy/random）、`seed_start`、`cards`（2人検証済みプール）、`extra`（追加アクションスペース）。

安全上限はスクリプト側でも強制する。`shards` は 1..16、`episodes` は 20000 以下、seed 上限 `10^12`。標準の GitHub-hosted `ubuntu-24.04` のみを使い、有料 larger runner / GPU は使わない。

## 3. Artifact スキーマ

- `<shard>/meta.json`（`scripts/episode_dataset.py` が計画から書く）: `schema_version`, `shard_index`, `shards`, `total_episodes`, `episodes`, `seed_start`, `seed_end`, `players`, `policy`, `additional_action_space`, `cards`, `commit_sha`。
- `<shard>/episodes.jsonl`: `EpisodeRecord`（`schema_version=1`）の1試合1行。失敗seedが混ざっても成功分は書く。
- `<shard>/errors.jsonl`: 生成に失敗したseedと理由（JSONL）。
- `<shard>/report.json`: `episode_inspect` が出力する `DatasetReport`。`total`/`finished`/`duplicate_seeds`/`corrupt` に加え、`seed_min`/`seed_max`/`distinct_seeds`/`total_turns`/`total_events` と provenance（`formats`/`engine_versions`/`rules_versions`/`card_modes`/`code_shas`）を持つ。
- `<shard>/export.log`・`export-error.log`・`inspect.log`・`inspect-error.log`: 生ログ。
- `summary.json`: `aggregate` の出力。`scope="episode-data-integrity-and-completeness"`, `not_win_rate_or_strength_claim=true`, 合計件数、seed 範囲、provenance、シャード別内訳。

`aggregate` は次をすべて満たさない限り**非0終了**する（成功扱いにしない）: meta の全フィールド一致、`corrupt`・`duplicate_seeds` が空、`total==finished==シャード件数`、`player_counts`/`code_shas`/`card_modes` が期待値、`seed_min/seed_max` が計画ブロックと一致、全シャードの provenance が一致、合計件数と seed 被覆が要求と一致。

## 4. ローカルでの小規模実行

```bash
cargo build --locked --bins
python scripts/episode_dataset.py shard \
  --episodes 4 --players 2 --seed-start 700 --shards 2 --shard 0 \
  --policy eval --sha local \
  --out target/episode-data-shard-0 \
  --binary target/debug/episode_export \
  --inspect-binary target/debug/episode_inspect
python scripts/episode_dataset.py aggregate \
  --episodes 4 --players 2 --seed-start 700 --shards 2 \
  --policy eval --sha local --out target
```

分割と M14 G0 判定は `docs/m12-dataset-split-and-g0.md`（`dataset_split`）で行う。本パイプラインの JSONL をそのまま入力にできる。

`episode_export` の直接利用:

```bash
episode_export --episodes 8 --players 4 --policy greedy --out out.jsonl --code-sha <sha> --errors errors.jsonl
episode_inspect --in out.jsonl --report report.json
```

## 5. テスト

- `tests/test_episode_dataset.py`: 計画の連続・非重複、不正入力の拒否、生成コマンドの組み立て、集約の正常系と破損・重複seed・件数不一致・seed範囲不一致・commit不一致・provenance不一致・シャード欠落の拒否（14件）。
- `tests/m12_episode_data.rs`: `DatasetReport` の再演検証・重複検出（既存）。provenance 集計は同レポートに含まれる。
- CI: `.github/workflows/ci.yml` の rustfmt/clippy/test が `episode_data.rs` と新規バイナリを含む。大量生成は通常CIに含めない。

## 6. 未完了・対象外

- ドラフト込みエピソードの生成（`DraftEpisode` の付与）は、ドラフト配布ルール（#155）と検証済みプール（#105/#139）が確定してから別途接続する。
- 3・4人カード戦、`choose_10_7`/`draft_10_7_simultaneous` は未検証プールのため生成しない。
- 勝率・首位率・95%区間・必要試合数の算出は `paired-eval` 側。本パイプラインだけで「AIが強い」「ルールが完全」とは主張しない。

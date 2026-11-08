# M8 証拠の生成と G0 への束縛（#108 / #109 / #110）

M14 の G0 ゲートは、matches から再計算できる検査と、**コードの実行結果にしか由来しない検査**を分けて扱う。後者は次の4項目で、以前は checks ファイルの `true` を人手で書くしかなかった。

| 検査 | 対応 Issue | 内容 |
|---|---|---|
| `m8_rule_evidence` | #108 | 公式ルールの期待結果テスト（得点・主要改善・収穫・追加タイル・人数別アクション） |
| `m8_legal_api_evidence` | #109 | 合法詳細行動APIが既定ポリシー外の合法手を受理し、不正手を状態不変で拒否する |
| `evaluation_observation_noninterference` | #108 | 非公開手札・未公開ラウンド順・内部RNGの変更が公開観測を変えない |
| `evaluation_ci_reproducible` | #110 | 同一seed・同一条件で対戦と再演が一致する |

この4項目を**人手の `true` ではなく実行結果から導出**するのが `scripts/m8_evidence.py` である。

## 1. 何を証拠にするか

`docs/m8-evidence-requirements.json` が「必要なテスト」の正本で、各テストに次を持たせる。

- `id`: `<target>::<test_fn>`。実在するテスト名でなければならない。
- `mode`: 対象モード（`cardless_2p` / `cardless_2p_additional` / `cardless_4p` / `verified_basic_2p`）。
- `expectation`: 期待結果を自分の言葉で書いたもの。
- `basis`: 公式典拠のキー（`basis_references` が実ページを指す）。

要件は追加・削除できるが、**削除して検査を消すことはできない**。チェックごとに `tests` が空ならスクリプトが失敗する。

## 2. 生成手順

```sh
# 1. 必須ターゲットを1ターゲット1ログで実行する
mkdir -p m8-logs
for t in rule_regressions official_improvements ... integration_acceptance; do
  cargo test --release --test "$t" > "m8-logs/$t.log" 2>&1
done

# 2. ログから証拠を導出する（欠落・失敗・ignored は false）
python3 scripts/m8_evidence.py collect \
  --requirements docs/m8-evidence-requirements.json \
  --log-dir m8-logs \
  --code-sha "$(git rev-parse HEAD)" \
  --run-url "https://github.com/<owner>/<repo>/actions/runs/<id>" \
  --out m8-evidence/m8-evidence.json

# 3. 評価 matches へ束縛し、checks_evidence.schema_version=2 を作る
python3 scripts/m8_evidence.py checks \
  --tests m8-evidence/m8-evidence.json \
  --requirements docs/m8-evidence-requirements.json \
  --matches results/shard-0/matches.jsonl --matches results/shard-1/matches.jsonl \
  --matches results/shard-2/matches.jsonl --matches results/shard-3/matches.jsonl \
  --code-sha "$(git rev-parse HEAD)" \
  --run-url "https://github.com/<owner>/<repo>/actions/runs/<id>" \
  --out m8-evidence/checks.json

# 4. 既存 checks がテスト結果と矛盾しないか再検証する（改変検出）
python3 scripts/m8_evidence.py verify \
  --tests m8-evidence/m8-evidence.json \
  --requirements docs/m8-evidence-requirements.json \
  --checks m8-evidence/checks.json
```

`checks.json` は `g0_evidence.py` の `--checks` に渡す。G0 ゲートは `checks_sha256` と、checks ファイル内の `code_sha` / `matches_sha256` を再計算して照合する。

GitHub Actions では `.github/workflows/m8-evidence.yml` が 1〜2 を実行し、`m8-evidence` Artifact を残す。G0 ゲート用ワークフローはこのワークフローを `workflow_call` で呼ぶので、**証拠と評価が同一コミット・同一 run で生成される**。

## 3. ゲート側が検証すること

`src/dataset_split.rs` の `validate_attestation` が checks ファイルの `attestation` を検査する。

- `producer` が `scripts/m8_evidence.py` であること。
- `run_url` が GitHub Actions の run URL であること。
- `test_report_sha256` と `requirements_sha256` が SHA-256 形式であること。
- `required_tests > 0`、`failed_tests == 0`、`missing_tests == 0`、`passed_tests == required_tests`。
- 4項目それぞれに `required > 0` があり、`passed == required`。
- **各項目の真偽値が attestation の通過数と一致すること**（片方だけ書き換えると矛盾で fail）。

判定は次のとおり。

| 状態 | 判定 |
|---|---|
| checks ファイルがない | `hold`（未提出） |
| checks ファイルはあるが attestation がない（人手の `true`） | `hold`（証拠なし） |
| attestation が上記の条件に反する | `fail`（矛盾） |
| checks の SHA-256 が一致しない / `code_sha`・`matches_sha256` が別物 | `fail` |
| すべて満たす | `pass` |

## 4. この仕組みが証明しないこと

- **テストの再実行ではない。** ゲートはテストスイートを走らせないので、attestation は「実行記録の完全性と内部整合」を検証する。改変検出は `verify` と CI の再生成（同一入力から同一 `checks.json` が再現すること）で行う。
- **公式ルールの全状態空間の証明ではない。** 要件に載った期待結果テストが通ったことの記録である。ルール解釈の疑義は引き続き Issue へ分離する。
- **強さの主張ではない。** G0 は実験土台のゲートであり、首位率の比較は M13 の別実験。

## 5. 既知の制約

- `integration_acceptance` の100試合再演テストを含むため、`m8-evidence.yml` は通常 CI ではなく手動・呼び出し専用ワークフローに置く。
- Artifact の保持は30日。長期監査で生ログが必要な場合は、保持期間と容量を確認して別途保存する。

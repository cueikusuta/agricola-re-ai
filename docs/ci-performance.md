# CIの実行条件・高速化方針

## 目的
- 必要なテストを削らず、GitHub-hosted Ubuntu上の通常のコード変更PRを**60秒以内**に近づける。
- READMEや `docs/**` のみを変更したPRでは、Rustのビルド・テスト・ベンチマークを実行しない。
- CIが必須チェックに指定されてもドキュメントだけのPRを待機状態にしない。

## 変更ファイルごとの判定

| 変更対象 | rustfmt | clippy | 全Cargoテスト（release） | M1/M2ベンチマーク |
| --- | --- | --- | --- | --- |
| `README.md`, `docs/**`, Markdownのみ | スキップ | スキップ | スキップ | スキップ |
| `tests/**` のみ | 実行 | 実行 | 実行 | スキップ |
| `src/**`, `Cargo.toml`, `Cargo.lock` | 実行 | 実行 | 実行 | 実行 |
| `.github/workflows/**`, ツールチェーン設定、`build.rs` | 実行 | 実行 | 実行 | 実行 |
| `scripts/**` のみ | 実行 | 実行 | 実行 | スキップ |
| 手動 `workflow_dispatch` | 実行 | 実行 | 実行 | 実行 |

PR時はPRのベースブランチとの差分、main/developへのpush時は直前コミットとの差分を判定する。PRとpushの重複実行を避けるため、`issue/**` へのpushではなくPRでCIを走らせる。未公開PRブランチの検査が必要なら手動実行する。

判定には `dorny/paths-filter@v4` を使用。ワークフロー自体を `paths` でスキップするとGitHubの必須ステータスがPendingのままになる場合があるため、ジョブ単位の `if` で分岐する。最後の `CI required` は必ず実行し、変更に対応するジョブがすべて成功したことを検証する。ブランチ保護の必須チェックには `CI required` を指定することを推奨。

## 実行時間
- 従来の `cargo test --locked --all-targets` はデバッグビルドで、400試合のテストに約90.6秒、別の再現テストに約39.8秒かかった。
- 新CIでは **`cargo test --release --locked --all-targets`** を使用する。テスト数・アサーション・対象コードは変えないが、プロファイルがreleaseに変わるため、デバッグ専用の挙動に差がないか今後も確認する。
- `Swatinem/rust-cache@v2` は依存パッケージのビルドをキャッシュする。同じreleaseキーをCargoテストとベンチマークで共有し、キャッシュ保存はテストジョブだけに限定する。
- 変更されていないソースに対して独立ベンチマークを再実行しない。ベンチマークは通常CIで対象コードの変更時にM1 OFF/ON各100試合、M2の基準seed 7100と未使用seed 26000の両設定を測る。
- CI所要時間は並列実行ジョブの最遅時間＋変更判定＋ゲート確認。キャッシュがない初回、キュー待機、GitHub側の負荷で60秒を超える場合がある。**未計測の目標達成は宣言しない。**

## 確認項目
1. コード変更PRでrustfmt/clippy/release全テスト/M1/M2の各ジョブが実行され、CI requiredが成功する。
2. testsのみの変更PRでベンチマークだけがスキップされる。
3. docsのみの変更PRでRustジョブがスキップされ、CI requiredが成功する。
4. 手動実行で全ジョブが選択される。
5. 冷・温キャッシュそれぞれの最遅ジョブとCI全体の時間を確認する。

公式の参考: [GitHub 必須チェックのスキップ仕様](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)、[dorny/paths-filter](https://github.com/dorny/paths-filter)、[Rust cache](https://github.com/Swatinem/rust-cache)、[Cargo test --release](https://doc.rust-lang.org/cargo/commands/cargo-test.html)。

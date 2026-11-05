# CIの実行条件・高速化方針

## 目的
- 必要なテストを削らず、GitHub-hosted Ubuntu上の通常のコード変更PRを**60秒以内**に近づける。
- READMEや `docs/**` のみを変更したPRでは、Rustのビルド・テスト・ベンチマークを実行しない。
- CIが必須チェックに指定されてもドキュメントだけのPRを待機状態にしない。

## 変更ファイルごとの判定

| 変更対象 | rustfmt | clippy | 全Rustテスト（nextest/release） | M1/M2/M7ベンチマーク |
| --- | --- | --- | --- | --- |
| `README.md`, `docs/**`, Markdownのみ | スキップ | スキップ | スキップ | スキップ |
| `tests/**` のみ | 実行 | 実行 | 実行 | スキップ |
| `src/**`, `Cargo.toml`, `Cargo.lock` | 実行 | 実行 | 実行 | 実行 |
| `.github/workflows/**`, ツールチェーン設定、`build.rs` | 実行 | 実行 | 実行 | 実行 |
| `scripts/**` のみ | 実行 | 実行 | 実行 | スキップ |
| 新規のソース・設定パス、その他のMarkdown以外の変更 | 実行 | 実行 | 実行 | 実行 |
| 手動 `workflow_dispatch` | 実行 | 実行 | 実行 | 実行 |

PR時はPRのベースブランチとの差分、main/developへのpush時は直前コミットとの差分を判定する。PRとpushの重複実行を避けるため、`issue/**` へのpushではなくPRでCIを走らせる。未公開PRブランチの検査が必要なら手動実行する。

判定には `dorny/paths-filter@v4` を使用し、`predicate-quantifier: every` で除外規則を同じファイルに適用する。`docs/**` と `**/*.md` だけをRustチェック対象から除外し、`tests/**` と `scripts/**` だけをベンチマーク対象から追加除外する。未知の新規ソース・設定ファイルは安全側に倒して全ジョブを実行する。ワークフロー自体を `paths` でスキップするとGitHubの必須ステータスがPendingのままになる場合があるため、ジョブ単位の `if` で分岐する。最後の `CI required` は必ず実行し、変更に対応するジョブがすべて成功したことを検証する。ブランチ保護の必須チェックには `CI required` を指定することを推奨。

## 実行時間
- 従来の `cargo test --locked --all-targets` はデバッグビルドで、400試合のテストに約90.6秒、別の再現テストに約39.8秒かかった。
- CIでは **`cargo nextest run --release --locked --all-targets`** を使用する。既存の `cargo test --release --locked --all-targets` と同じテストターゲットを実行する（両コマンドともドキュメントテストは対象外）。nextestはテストをプロセス単位で分離し、複数テストバイナリを並列実行するため、実行速度が上がる。バージョンは脆弱性修正済みの **0.9.151** を指定する。
- `Swatinem/rust-cache@v2` はビルドキャッシュを保存する。テストとベンチマークは `release` のキャッシュを共有し、テストジョブだけが保存する。性能計測はM1・M2・M7を**1ジョブにまとめてビルドを1回に統一**する。
- 変更されていないソースに対して独立ベンチマークを再実行しない。通常CIでは実行コードの変更時に、M1 OFF/ON各100試合、M2の基準seed 7100（50シード）と未使用seed 26000（20シード）の両設定、M7のカードありseed 901〜910（各10シード・先後交代）を計測する。
- CI所要時間は並列実行ジョブの最遅時間＋変更判定＋ゲート確認。キャッシュがない初回、キュー待機、GitHub側の負荷で60秒を超える場合がある。**未計測の目標達成は宣言しない。**
- Ubuntu runnerにはRustツールチェーン・rustfmt・clippyが最初からあるため、各ジョブの追加rustupセットアップを外す。Ubuntu 26への移行中にランナーの差による結果の不安定化を抑えるため、現段階では `ubuntu-24.04` に固定している。

## 実測値と高速化実験の記録

| コミット | 主な設定 | GitHub Actions全体 | 判定 |
| --- | --- | ---: | --- |
| `794b28d` | Cargo release全テスト＋分離M1/M2 | 67秒 | 60秒超過 |
| `1961193` | nextest 0.9.151、その他据え置き | **50秒** | 60秒以内を実測達成 |
| `f0c79bd` | nextest、通常runnerの一部でツールチェーン準備が停滞 | 283秒 | GitHub側準備時間の変動で超過 |
| `b80babc` | Ubuntu 24.04固定・preinstalled rustfmt | 107秒 | キュー待機約34秒＋冷えたreleaseビルドによって超過 |
| `c30d6a0` | nextest+統合性能計測（release） | 115秒 | キュー待機約30秒＋Rustテストの冷えたreleaseビルド45秒により超過 |
| `c01fe56` | CIテストのみ独立 `ci-fast` (opt2/codegen32)、初回冷キャッシュ | 132秒 | キュー待機約60秒＋依存パッケージ初回コンパイルのため超過。温キャッシュは別途測定 |
| `e8e9421` | ci-fast温キャッシュ＋統合計測 | 97秒 | テスト43秒・clippy48秒のほか、最終ゲートのRunner割当てで約33秒待機し超過 |
| `3413b7b` | 本番release nextest・統合性能計測・4並列 | 122秒 | CI全成功、ジョブの割当て待ち約50秒・性能ジョブ開始遅延を含む |
| `906f5d8` | M7追加MCTS通しテスト・本番release nextest | 62秒 | 全ゲート成功。Clippyの事前バージョン照会に5秒など短縮余地が残る |
| `215e466` | version-probeだけ除去した全テスト | 102秒 | 全ゲート成功、依存キャッシュはhitだが本体releaseコンパイル53秒 |
| `90466cd` | 初回sccache試験、バージョンprobe除去 | 82秒 | 全ゲート成功、sccache 1件miss・1件write（自作ライブラリ）。温キャッシュ確認前 |
| `0c5fc72` | 次回sccache再利用（テストのみ） | 71秒 | 全ゲート成功。sccache 1/1 hit、自作libのreleaseビルド17.6秒 |
| `43d0cea` | ベンチマークのリリースコンパイルにもsccache | 97秒 | 全成功。2ジョブとも1/1 cache hitだが、テストでCargo index待機約27秒 |
| `1740e3e` | オフラインCargo事前判定を試験 | 79秒 | 全成功。cache hitでもoffline fetch判定は失敗し、オンライン実行へフォールバックした。方法を撤回 |

M1/M2のリリースビルド統合は成功した。コンパイル成果物の再利用策として、`mozilla-actions/sccache-action@v0.0.11` をnextestと統合ベンチマークのreleaseビルドだけに適用した。初回は1件cache missだったが温キャッシュでは両ジョブで各1件hitを記録し、自作libの再コンパイルは短縮した。`cargo fetch --locked --offline` の事前判定はキャッシュ復元後でも失敗するため、本CIでは採用しない。CI軽量化Issue #97では、Ubuntu 24.04に組込み済みRustのバージョン照会ステップだけを削除する。Rust本体、rustfmt、clippy、nextest、全ベンチマークおよび最終ゲートは維持する。削減量の正確な判断はPRのCI成功後の実測に基づく。`ci-fast` は冷・温キャッシュの両方で60秒を超え、release方式の最速50秒に及ばなかったため撤回した。releaseキャッシュ共有とUbuntu同梱のRustツール利用を採用。最終構成の時間は最終CIログで確認する。GitHub-hostedランナーの割当てやキュー待機はリポジトリ内の設定だけで常時60秒以内に保証できない。CIの所要時間を報告するときはコミットとrun URLを付ける。

## #99 Cargo registry索引キャッシュ検証（2026-11-06）

| 対象 | GitHub Actions run | 実測 | 結果 |
| --- | --- | ---: | --- |
| main `607f813`（変更前） | [42067107964](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42067107964) | 89秒 | 全ゲート成功。Cargo index更新に約11秒、nextest step 53秒 |
| PR #100 `be51501`（索引キャッシュ初回） | [42070243738](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42070243738) | **64秒** | 全ゲート成功。初回は索引キャッシュなしで `rand` のoffline解決ができず、通常オンライン実行へフォールバック。次のCI実行で温キャッシュを検証 |

`actions/cache` で `~/.cargo/registry/index` を `Cargo.lock` ハッシュ別に保存する。Rustテストのjobだけが保存し、Clippy・統合ベンチマークのjobは読み取り専用にする。各jobで `cargo fetch --locked --offline` が成功した場合だけ後続のCargoをオフライン動作させる。索引またはパッケージが不足している場合は**必ずオンラインへ戻す**ため、ネットワークを理由にテストを省略しない。変更前のCI品質ゲートはすべて維持する。

## 確認項目
1. コード変更PRでrustfmt/clippy/nextest/release全テストとM1/M2/M7統合性能計測が実行され、CI requiredが成功する。
2. testsのみの変更PRで統合ベンチマークだけがスキップされる。
3. docsのみの変更PRでRustジョブがスキップされ、CI requiredが成功する。
4. 手動実行で全ジョブが選択される。
5. 新規の未登録パス（例 `examples/new.rs`）を変更したPRでも、Rustチェックとベンチマークの両方が実行される。
6. 冷・温キャッシュそれぞれの最遅ジョブとCI全体の時間を確認する。

公式の参考: [GitHub 必須チェックのスキップ仕様](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)、[dorny/paths-filter](https://github.com/dorny/paths-filter)、[Rust cache](https://github.com/Swatinem/rust-cache)、[Cargo test --release](https://doc.rust-lang.org/cargo/commands/cargo-test.html)。

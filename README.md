# Agricola Revised AI — M1

アグリコラ（2016年改訂版）の2人対戦AIを目指すRustプロジェクト。**未完成の実装作業版**です。

- M1: 2人戦ルールの正確な14ラウンド・再現性テスト
- M2: 評価関数AI
- M3: MCTS
- M4: 3–4人・対戦UI
- M5: カード・非公開情報

設計の正本: `docs/design.md`、依頼: `docs/implementation-request.md`。

## 仕様の固定値

- Rust Stable + Cargo、Windows 11で開発、UbuntuでGitHub Actions。
- Side Jobなし。職業・小改善カードなし。
- 追加アクションタイルは `additional_action_space_enabled` でON/OFF（初期OFF）。
- シード付き乱数を用途ごとに分離。AIへ非公開ラウンド順やシードを渡さない。
- UIを実装しない。

## ローカルで実行する場合

```bash
cargo fmt --check
cargo test --all-targets
cargo run -- --seed 42
cargo run -- --seed 42 --additional-action-space
cargo run -- --seed 42 --log-file game-log.json --state-file final-state.json
```

⚠️ このアーカイブは実行・コンパイルの検証ができていない。実装範囲と未完了事項は `docs/implementation-status.md` を参照。

公式資料（ルールブック本文・画像・カード原文はリポジトリへ収録しない）:

- https://lookout-spiele.de/upload/en_agricolare.html_Rules_Agricola-RE_EN.pdf
- https://lookout-spiele.de/upload/en_agricolare.html_Appendix_Agricola-RE_EN.pdf

## GitHubへの作成とIssue登録（Windows 11）

GitHub CLI `gh` のインストールと `gh auth login` の実行後、展開した
プロジェクトのルートから `./scripts/bootstrap-github.ps1` を実行してください。
公開リポジトリ `cueikusuta/agricola-re-ai` と11件のIssueを作り、
README・設計ドキュメントだけを初期コミットとして送ります。
**ソースコードは別途、各Issueに対応するブランチとPRへ分割してください。**
重複作成を防ぐため、同じスクリプトの再実行は想定していません。

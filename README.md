# Agricola Revised AI — M1

アグリコラ（2016年改訂版）の2人対戦AIを目指すRustプロジェクト。**M1の統合レビュー候補（Rust実装・CI検証済み、main未マージ）**です。

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

M1の実装・テストと受け入れ状況は `docs/m1-acceptance.md` および `docs/implementation-status.md` を参照。GitHub ActionsのRustfmt/Clippy/Cargo test/100試合計測を確認済み。公式ルール上のあらゆる組み合わせの形式的証明や、人間による最終レビューは別途必要です。

公式資料（ルールブック本文・画像・カード原文はリポジトリへ収録しない）:

- https://lookout-spiele.de/upload/en_agricolare.html_Rules_Agricola-RE_EN.pdf
- https://lookout-spiele.de/upload/en_agricolare.html_Appendix_Agricola-RE_EN.pdf

## 初回セットアップ用の参考手順（完了済み）

GitHub CLI `gh` のインストールと `gh auth login` の実行後、展開した
プロジェクトのルートから `./scripts/bootstrap-github.ps1` を実行してください。
初回のみ公開リポジトリ `cueikusuta/agricola-re-ai` と11件のIssueを作り、
README・設計ドキュメントだけを初期コミットとして送ります。
**ソースコードは別途、各Issueに対応するブランチとPRへ分割してください。**
重複作成を防ぐため、同じスクリプトの再実行は想定していません。

## M1の統合検証ブランチ

最新版の統合実装は `issue/45-m1-final-stables-audit` に積み上げてあります。公式ルールを確認した修正（厩の建築場所を含む）を追加し、Rustfmt・Clippy・Cargo test・ON/OFF各100試合のGitHub Actionsが成功しています。統合レビューとmainへの反映は別の作業です。

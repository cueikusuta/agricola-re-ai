# Agent instructions

1. ルールの正本は2016年改訂版Lookout公式ルール。基本設計・既存仕様の正本は `docs/design.md`、**M7-C以降の工程・AI評価・受け入れ基準の正本は `docs/roadmap-m7-onward.md`**。相違があれば報告する。
2. 1 Issue = 1 ブランチ = 1 PR。全PRに実装内容・テスト結果・曖昧なルール・公式典拠を記す。
3. ルール解釈不明時は推測しない。該当規則のページ・選択肢・影響をIssueに残し、人間の決定待ち。
4. 履歴上のM1では職業・小改善カード、Side Job、3人以上、UI、MCTS、高度な評価関数・定石ライブラリを作らない方針だった。**現在はM7が統合済み**であり、M1の段階制約を現在の機能禁止と誤解しない。Side Jobの非実装方針は継続。
5. 追加スペースは初期OFF、ONで2人用4択を排他処理、林の貯蓄は非選択時も維持する。
6. 公式PDF・カード画像・カード原文データを入れない。自作コードと自分の言葉による説明のみ。
7. ルールエンジンからAI・細部決定ポリシーを独立させ、AI観測から未公開カードと内部シードを排除する。
8. テストを各PRに添え、Cargo fmt/clippy/testと固定シード再現をゲートにする。通常PRの軽量CIは60秒以内を目標（ホスト変動は絶対保証しない）。長時間の1,000試合評価・Playwright完走・学習は別ジョブで実施する。**大量シミュレーション・自己対戦・AI比較は原則GitHub ActionsのGitHub-hosted標準CPUランナーで実行**し、ローカルは小規模デバッグ用とする。
9. ライセンスファイルは作成しない。

10. **M7以降の開発・AI評価・自己レビュー・GitHub Actions大量試合運用（旧第10〜13項）**：詳細は [`docs/agents/ai-experiments.md`](docs/agents/ai-experiments.md) を参照。
14. **独立レビュー・指摘修正・レビュー終了条件（旧第14〜16項）**：詳細は [`docs/agents/reviewer.md`](docs/agents/reviewer.md) を参照。
17. **自動実行時の確認省略範囲**：ユーザーが当該タスクで `auto` 等の自動実行を指示した場合、次の操作は対象タスク（1 Issue=1目的=1ブランチ=1PR）の範囲内で**チャットでの都度確認を省いて進めてよい**。ただし、この文書だけでアプリ・サンドボックス・GitHubの権限や承認が付与されるわけではない。
    - **シェルの初期カレントディレクトリはこのリポジトリ直下**（`PWD` = リポジトリ）。コマンドの頭に `cd <絶対パス> &&` を付けない（不要な上、コマンドごとの承認要求につながる）。そのまま実行し、別ディレクトリは `--manifest-path` など引数で指定する。
    - ローカル検証: `cargo fmt` / `cargo clippy` / `cargo test`、`node --check`、feature ブランチでの `git add` / `git commit`。
    - リモート: feature ブランチへの push（`git push -u origin <branch>` 形式）、`gh pr create` / `gh issue create` / `gh pr comment`。`main` や他セッションのブランチへは push しない。
    - 開発依存: Playwrightスモーク用の `npm install --ignore-scripts --no-audit --no-fund`。
    - プロジェクト内で当該タスク自身が生成した、識別可能で不要な一時ファイル（`target/` 内の該当ファイルなど）の作成・削除。他タスクの生成物や共有ディレクトリの一括削除は行わない。成果物やPR差分には含めない。
    - アプリ・サンドボックス・GitHubが承認を要求した場合は正規の承認フローに従う。本項を理由に要求を無視しない。承認が拒否された場合は回避・迂回せず、その事実と理由をPRまたは最終報告に記録して独立して実行可能な作業へ進む。
    - 引き続き禁止: `main` への直接コミット、force push、`git reset --hard`、`git clean -fd`、他セッションの作業ブランチの変更・上書き、外部サービスへの無断送信、秘密情報の読み出し・掲載。

18. **PR検証のActions逃がし（2026-11-17）**：PR作成時の検証はGitHub Actionsを主体とし、ローカル実行は対象を絞る。ワーカーは `cargo fmt --check` / `cargo clippy --all-targets --all-features --locked -- -D warnings` と、自分が追加・変更したテスト（`cargo test --lib <カードID または 変更モジュール>`）のみをローカルで実行する。プール数・MCTSダイジェストを動かした場合に限り `cargo test --test m6_verified_pool` と `cargo test --lib mcts::tests::sample_game_distribution_digest_is_stable` を追加で実行する。**`cargo test --locked` の全件実行はローカルの必須手順としない** — `ci.yml` の `cargo test` ジョブ（nextest `--release --locked --all-targets`）が全PRで全件を実行し、`pipeline-gates.yml` が仕様・台帳・ブロッカーのPythonゲート3本を全PRで実行する。PR本文のゲート欄にはローカル測定値ではなく**Actionsのチェック名・結論・run URL**を記録する。レビューとマージの判断も同じくActionsのチェック状態（review対象HEAD SHAの check-runs）を根拠とし、レビュー時のローカル全件再実行は行わない。Actionsが赤い場合は失敗ジョブのログをrun URLから読んで原因を直し、pushする。詳細は `docs/ci-performance.md` の「PR検証のActions逃がし」節。`Mode-wide Simulator Verification` / `G0 gate` などmain統合時の大量検証の扱いは第13項（[`docs/agents/ai-experiments.md`](docs/agents/ai-experiments.md)）のまま変更しない。

19. **カード実装供給・レビュー・作業単位の運用方針（Issue #530）**：詳細は [`docs/agents/operations-policy.md`](docs/agents/operations-policy.md) を参照。新規のカード実装 Issue・Kanban カードは仕組み単位の複数カードバッチを既定とし、**1枚だけの実装カードを新規に切らない**（即時適用）。補充・優先順位・復帰の決定スクリプト化と、仕様・テスト契約・CI を主役にするレビューへの移行は別セッションで実施する。移行が統合されるまでは現行の独立レビューとマージゲートを維持する。

## 役割別規約

- AI実験・大量試合運用（第10〜13項）：[`docs/agents/ai-experiments.md`](docs/agents/ai-experiments.md)
- 独立レビュー・再レビュー（第14〜16項）：[`docs/agents/reviewer.md`](docs/agents/reviewer.md)
- レーンBカード量産ワーカー最小規則：[`docs/agents/card-worker.md`](docs/agents/card-worker.md)

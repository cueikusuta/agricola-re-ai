# レーンB カード量産ワーカー最小規則

共通原則はルート直下の [`AGENTS.md`](../../AGENTS.md) を参照する。本書は、既存エンジン語彙のみで完結するカード実装（レーンB）を担当するワーカーの最小規則を定義する。

## 1. 対象カードの正本と選定条件
- 正本は `data/bga-card-specs/{ID}.json` とする。実装前に必ず対象カードのJSON全文を確認する。
- レーンBで実装してよいカードは、次のすべてを満たすものに限定する：
  - `review.status == "bga_checked"` かつ `review.unresolved_fields` が空である。
  - 既存の `CardEffect`（および既存の `PassiveRule` / `HarvestFieldRule` / `CardRequirement` / `ScoreBonusRule` / `CountKind` 等）の組み合わせのみで、仕様のすべての前提条件・効果・`post_play` を正確に表現できる。
- 候補の抽出には `python3 scripts/analyze_card_unlocks.py` を使用する。

## 2. 変更許可ファイル限定
レーンBのPRで変更してよいファイルは次に限定する（1目的=1ブランチ=1PR）：
- `src/cards.rs`（対象カードの `CardDefinition` 定義および単体テスト）
- `tests/` 配下の対象カード用テストファイル（必要な場合のみ）
- 対象カードの `data/bga-card-specs/{ID}.json`（`review.implementation_status` 等の状態更新が必要な場合のみ）
- プール数や検証台帳の整合更新が必要な場合の該当ドキュメント／CSV（対象カード分のみ）

次のような広域・基盤ファイルの変更はレーンBの範囲外であり禁止する：
- `src/engine.rs`, `src/card_events.rs`, `src/harvest.rs`, `src/score.rs`, `src/registry.rs`, `src/model.rs`, `src/pending.rs`, `src/validation.rs` などのエンジン本体
- `.github/workflows/**`, `Cargo.toml`, `Cargo.lock`
- 対象外カードの定義・仕様・テスト

## 3. 既存 `CardEffect` 以外の変更禁止
- `CardEffect` / `PassiveRule` / `HarvestFieldRule` / `CardRequirement` / `ScoreBonusRule` / `CountKind` 等に**新しいバリアントやフィールドを追加してはならない**。
- 既存バリアントの挙動・シリアライズ形式・`CardSet::content_hash` の既存カード分を変えてはならない。
- 既存語彙だけでは表現できないことが判明した場合、無理に近似せず作業を止め、語彙拡張・エンジン改修タスク（別Issue/PR）へ分離する。

## 4. 推測実装の禁止・著作権配慮
- 仕様の解釈に曖昧さ・矛盾・未定義タイミングがある場合は推測で実装しない。該当カードの `review.status` を無理に維持・昇格させず、疑問点と選択肢をIssueまたはPRに記録して人間の判断を待つ。
- 公式PDF・カード画像・カード原文テキストはリポジトリおよびPR本文に含めない。自作コードと自分の言葉による要約のみを用いる。
- 効果の未実装部分をダミー（空効果・無条件化・近似）でごまかさない。

## 5. 提出前ローカル必須ゲート4本
PR作成・push前に、次のローカル必須ゲート4本をすべて通過させること。

1. **フォーマット検査**: `cargo fmt --all -- --check`
2. **静的解析**: `cargo clippy --locked --all-targets --all-features -- -D warnings`
3. **対象テスト実行**: `tests/<arg>.rs` があれば `cargo test --locked --test <arg>`、なければ `cargo test --locked --lib <arg>`
   - プール数やMCTSダイジェストを動かした場合に限り、`cargo test --test m6_verified_pool` と `cargo test --lib mcts::tests::sample_game_distribution_digest_is_stable` も追加実行する。
4. **Python仕様・台帳・ブロッカーゲート（3本一式）**:
   - `python3 scripts/validate_bga_card_specs.py`
   - `python3 scripts/card_verification_blockers.py`
   - `python3 scripts/validate_card_catalog.py`

これらを一括実行する補助スクリプトを利用すること：
- Linux / macOS / Git Bash: `scripts/pre_pr_gate.sh <test_target_or_card_id>`
- Windows PowerShell: `scripts/pre_pr_gate.ps1 <test_target_or_card_id>`

全件Rustテスト（`cargo nextest run --release --locked --all-targets`）とパイプラインゲートは GitHub Actions（`CI required` / `Pipeline Gates`）で確認し、PR本文にはチェック名・結論・run URLを記録する（`AGENTS.md` 第18項参照）。

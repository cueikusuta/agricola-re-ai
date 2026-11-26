## 概要

PR #494 (A016 踏み粘土) の修正:
1. `scripts/build_bga_card_specs.py` の A016 エントリに `implementation_status: "implemented"` を追加し、spec JSON との同期を取った
2. `free_fence_wood` (A088 生け垣管理人) との組み合わせテスト `A016-free-fence-wood` を builder と Rust テストの両方に実装
3. PR 本文をこの修正内容に書き換え

## 変更内容

### 1. Builder スクリプトの同期修正 (`scripts/build_bga_card_specs.py`)

A016 エントリに以下を追加:
- `"implementation_status": "implemented"` — spec JSON の review セクションと整合
- テストケース `A016-free-fence-wood` — A088 の無料柵木材 3 と A016 の粘土代替の組み合わせ動作を検証

### 2. 生成ファイルの更新

`python scripts/build_bga_card_specs.py --cards A016 --force` で再生成:
- `data/bga-card-specs/A016.json` — `test_case_ids` に `A016-free-fence-wood` 追加、`implementation_status: "implemented"` 維持
- `data/bga-card-tests/A016.json` — 新テストケース `A016-free-fence-wood` 追加（6 柵セグメント、無料木材 3 → 粘土 3 支払い）

### 3. Rust 統合テストの実装 (`tests/a016_treading_clay.rs`)

`a016_free_fence_wood_combination` テストを完全実装:
- A016 と A088 の両パッシブ効果（`fence_material_substitution`, `free_fence_wood=3`）を手動で有効化
- 6 柵セグメント建設時に「無料木材 3 → 不足分 3 を粘土で支払い」の動作を検証
- 既存 6 テストと合わせて計 7 テストが全件通過

## ゲート確認（ローカル実測）

- `cargo fmt --check` ✅
- `cargo clippy --all-targets --all-features --locked -- -D warnings` ✅
- 追加テスト `cargo test --test a016_treading_clay` ✅ (7/7 passed)
- 既存テスト `cargo test --lib` ✅ (505/505 passed)

## Actions チェック結果

（PR 作成後の Actions 実行を待機）

## 関連ファイル

- `scripts/build_bga_card_specs.py` — builder 定義・テストケース追加
- `data/bga-card-specs/A016.json` — spec 同期（自動生成）
- `data/bga-card-tests/A016.json` — テストケース追加（自動生成）
- `tests/a016_treading_clay.rs` — 統合テスト実装

**レビュー待ちで完了。マージはレビュー通過後に実施。**
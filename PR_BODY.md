## Summary

PR #372 の P1 指摘への対処: `load_json` の旧セーブ移行ブロックが実質的にデッドコードになっていた問題を、旧形式のみの v5 fixture と修正前 RED / 修正後 GREEN の実測証跡で固定。

## Problem

現行の `with_well_schedule_2p_seed7_r4.json` fixture は新旧混在形式（トップレベルにも `public` にも `well_food_schedule` / `card_uses` が非空）だったため、`load_json` の移行分岐（`public.*` が空のときのみ移設）が**一度も通らない**。移行ブロックを丸ごと削除しても `legacy_v5_fixture_loads_and_replays` が緑のまま（レビュー側で実測）。さらに fixture の `log.integrity_hash` が `null` で、元 P1 の「旧セーブが integrity mismatch で拒否される」経路も再現していなかった。

## Changes

1. **旧形式のみの v5 fixture を新規作成** (`tests/fixtures/v5/old_format_well_schedule_2p_seed7_r4.json`)
   - トップレベルに `well_food_schedule` / `card_uses` 非空、`public.*` には両欄なし
   - `log.initial_public_state` にも両欄なし（空オブジェクトなし）
   - `log.integrity_hash: null`（pre-Step3 互換）
   - 既存 `with_well_schedule_2p_seed7_r4.json` から `public.*` と `log.initial_public_state.*` を除去して生成

2. **混在 fixture を純粋新形式に修正** (`tests/fixtures/v5/with_well_schedule_2p_seed7_r4.json`)
   - トップレベルの `well_food_schedule` / `card_uses` を削除
   - `public.*` と `log.initial_public_state.*` のみを正本とする

3. **テスト追加・更新** (`tests/m1.rs`)
   - `legacy_v5_old_format_fixture_migrates_correctly`: 旧形式 fixture のロード→移行検証→再演照合→整合性検証→継続プレイ→往復保存を一連で検証
   - `legacy_v5_fixture_loads_and_replays`: コメント修正（移行テスト→新形式ロードテスト）

## RED / GREEN 実測証跡

### RED: 移行ブロック削除状態（`314c775` 相当）で旧形式 fixture が失敗

```bash
# src/model.rs の移行ブロック（1089-1106行）をコメントアウト
cargo test legacy_v5_old_format_fixture_migrates_correctly -- --nocapture
```

```
thread 'legacy_v5_old_format_fixture_migrates_correctly' panicked at tests/m1.rs:945:9:
assertion `left == right` failed: well_food_schedule round 5 should migrate to public
  left: None
 right: Some([0])
test legacy_v5_old_format_fixture_migrates_correctly ... FAILED
```

→ 移行なしでは `public.well_food_schedule` が `None` のまま（トップレベルの値が移設されない）

### GREEN: 移行ブロック有効で同テストが通過

```bash
# 移行ブロックを復元
cargo test legacy_v5_old_format_fixture_migrates_correctly -- --nocapture
```

```
test legacy_v5_old_format_fixture_migrates_correctly ... ok
```

→ `public.well_food_schedule.get(&5) == Some(&vec![0])` 等、移行が正しく動作

### 既存テスト全緑確認

```bash
cargo test --test m1
# 32 tests passed (新テスト含む)

cargo test --test layer0_publicstate_roundtrip
# 6 tests passed

cargo test --test replay_integrity
# 4 tests passed

cargo test --test final_integrity
# 5 tests passed
```

## Verification

- `cargo fmt --check` ✓
- `cargo clippy --locked --all-targets --all-features -- -D warnings` ✓
- `cargo test --test m1` (32 tests) ✓
- 既存 fixture のロード・再演テスト継続合格 ✓

## Base SHA

fe036db569fcb014d6553d3f4697977dccd7bd2b (PR #372 HEAD before this fix)

## Related

- PR #372 (load_json 移行・v5 fixture 追加) - 元 PR
- PR #357 (Layer 0 正本化) - マージコミット 3024302
- レビュー記録: https://github.com/cueikusuta/agricola-re-ai/pull/372#issuecomment-6994476033
- 親レビューカード: t_3c3f5aeb

# Freebuff on_play 獲得カード実装 — 結果

## 第1章 A帯（A054, A165）

- `src/cards.rs::verified_band_a_cards()` に on_play 獲得部分を追加。
  - A054 信用貸し: `MaximumOccupations(3)` + food 5。
  - A165 猪飼い: `MinimumPlayers(4)` + food 1。
- `tests/onplay_gain_ch01_a.rs`: メタデータ・前提条件・food 増分の 2 テスト成功。
- `data/bga-card-specs/A054.json` / `A165.json`: `implementation_status` を `in_progress` に更新。
- 除外カードと質問は `PROGRESS.md` に記録。

## 第2章 B帯（B025, B103, B116, B155）

- `src/cards.rs::verified_band_b_cards()` に on_play 獲得部分を追加。
  - B025 パン焼き棒: wood 1 コスト、food 1。
  - B103 畑の取引人: wood 1 + reed 1。
  - B116 岸辺の林業者: wood 1。
  - B155 美術教師: `MinimumPlayers(4)`、wood 1 + reed 1。
- `tests/onplay_gain_ch02_b.rs`: メタデータ・コスト・増分の 4 テスト成功。
- 仕様 4 枚の `implementation_status` を `in_progress` に更新。

## 第3章 C帯（C026, C086）

- `src/cards.rs::verified_band_c_cards()` に on_play 獲得部分を追加。
  - C026 脱穀棒: wood 1 コスト、food 2。
  - C086 家畜の餌係: grain 1。
- `tests/onplay_gain_ch03_c.rs`: コスト・増分の 2 テスト成功。
- 仕様 2 枚の `implementation_status` を `in_progress` に更新。

## 第4章 D帯（D062, D117）

- `src/cards.rs::verified_band_d_cards()` に on_play 獲得部分を追加。
  - D062 ビールタップ: wood 1 コスト、food 2。
  - D117 木の専門家: wood 2。
- `tests/onplay_gain_ch04_d.rs`: コスト・増分の 2 テスト成功。
- 仕様 2 枚の `implementation_status` を `in_progress` に更新。

## 第5章 E帯（E082）

- `src/cards.rs::verified_band_e_cards()` に E082 不当利得の on_play 獲得部分（food 1）を追加。
- `tests/onplay_gain_ch05_e.rs`: 1 テスト成功。
- 仕様 E082 の `implementation_status` を `in_progress` に更新。

## 最終検証（PR 前）

- `cargo fmt --check`: exit 0
- `cargo clippy --all-targets --all-features --locked -- -D warnings`: exit 0
- 新規テスト 5 本 / 11 ケース: すべて成功
- `python scripts/validate_bga_card_specs.py`: exit 0、`errors: []`（`in_progress` 11）
- 対象外カードと `needs_resolution` の質問一覧は `PROGRESS.md` に記録。
- PR は作成まで。マージしない。

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

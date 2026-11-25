# Freebuff on_play 獲得カード実装 — 結果

## 第1章 A帯（A054, A165）

- `src/cards.rs::verified_band_a_cards()` に on_play 獲得部分を追加。
  - A054 信用貸し: `MaximumOccupations(3)` + food 5。
  - A165 猪飼い: `MinimumPlayers(4)` + food 1。
- `tests/onplay_gain_ch01_a.rs`: メタデータ・前提条件・food 増分の 2 テスト成功。
- `data/bga-card-specs/A054.json` / `A165.json`: `implementation_status` を `in_progress` に更新。
- 除外カードと質問は `PROGRESS.md` に記録。

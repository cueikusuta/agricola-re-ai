# 実装×仕様JSON 突合監査 結果（DeepAI）

対象: `origin/main` = `89c38ab`（2026-11-21）。詳細な方針は `docs/impl-spec-audit.md`。
章の区切りごとに追記する。

## 第0章 監査基盤（基底ブランチ `research/deepai-impl-spec-audit`）

- 対象同定: `registry::build_default_registry()` を一時バイナリで走査し、
  `external_id/kind/cost/points/effects` を出力。**77枚**（A29/B21/C10/D8/E9）。
  一時バイナリは監査後に削除し、成果物へ含めない。
- `builtin_implemented_cards()` は帯別プールを含まず63枚のため、枚数同定には使わない。
- 判定基準は コスト／前提／発火点／対象／得られるもの の5点。表現差
  （`MinimumPlayers` と `bga.eligibility`）は不一致に数えない。
- 全体サマリ: 一致44 / 不一致9 / 仕様未作成9 / 他PR進行中4 / 基盤2 / 表現差5
  （横断の post_play 10枚は別掲）。実装77枚と一致することを確認。
- 横断所見:
  - `post_play: left_neighbor_hand` のエンジン未実装（実装済み10枚、仕様全体85枚）。
    基盤Issueへ分離し、仕様・テスト契約は文献どおり維持する。
  - registry の人数別定義の畳み込み（B102/B121）は未マージ #368 の範囲。
  - 語彙の揺れ（B098/B099 の `game_end` 等）は語彙統一Issueへ送る。
- 人間チェックリスト Q-1〜Q-9 を `docs/impl-spec-audit.md` §6 に記録。

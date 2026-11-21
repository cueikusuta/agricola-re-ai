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
- 全体サマリ: 一致42 / 不一致8 / 仕様未作成9 / 他PR進行中4 / 基盤2 / 表現差5 /
  文献未確定3 / post_playのみ課題4 = 77枚。
  post_play は主判定に重ねる別軸で、実装済み10枚（A005/A008/B006/B007/C004/C005/
  D004/D005/E007/E008）が該当。
- 横断所見:
  - `post_play: left_neighbor_hand` のエンジン未実装（実装済み10枚、仕様全体85枚）。
    既存 Issue #365 が追跡中。本監査は #365 の対象枚数の根拠を更新し、
    仕様・テスト契約は文献どおり維持する。
  - registry の人数別定義の畳み込み（B102/B121）は未マージ #368 の範囲。
  - 語彙の揺れ（B098/B099 の `game_end` 等）は語彙統一Issueへ送る。
- 既存の人間決定待ちを再発見: C005 の進歩の語義（#349）、B006 の「人物の数」（#351）。
  いずれも決定せず、文献の日英文言を判断材料として記録する。
- 人間チェックリスト Q-0〜Q-9 を `docs/impl-spec-audit.md` §6 に記録。

## 第1章 A帯（29枚・ブランチ `research/deepai-impl-spec-audit-A`）

- 監査枚数29。判定: 一致21 / 不一致2 / 表現差3 / 文献未確定1 / post_playのみ2。
- 不一致（修正）2件:
  - **A042 森の湖畔荘**: 文献（日本語ページ・英語一覧）は「森林」のみを対象とする。
    実装と仕様は「森林/小森/林」を対象にしていた。A116 木こりは同じ文献で
    「木材の累積スペース」と書き分けられているため、A042は森林1スペースが正しい。
    実装を `Forest` のみに修正、仕様を `space: forest` に、テスト契約に
    `A042-copse-not-triggered` を追加。ビルダー（`scripts/build_bga_card_specs.py`）も同期。
  - **A077 背負いかご**: 出した時のレンガ1は一致。第二効果「いずれかのプレイヤー
    （自分を含む）が猪市場を使うたびレンガ2」が実装に欠落していた。旧コメントは
    「any-player条件が表現できない」としていたが、`AfterAnyWorkerGain`（C141/A050で使用）の
    追加により解消済み。実装に第二効果を追加し、自分/他人の猪市場使用と非発動の回帰テストを追加。
- 修正なしで記録したもの:
  - A133/A143/A155: 人数適格を `MinimumPlayers` と `bga.eligibility` で表現する差。不一致ではない。
  - A101 調理器具商: 文献は「掘り土窯・東洋かまど」も対象と補足するが、エンジンは
    基本大進歩10種のみでカード進歩を表現できない（Q-2）。
  - A005/A008: 効果・コスト・前提は一致。使用後の「左隣の手札」は #365 の基盤待ち。
- 検証: `cargo test --lib cards::`（121件）、`cargo test --test ch03_onplay_per_count`（26件）、
  `cargo clippy --all-targets --all-features --locked -- -D warnings` 成功。
  仕様JSONはビルダー出力と意味的に一致（CIの再生成一致ゲートで確認）。

# result-freebuff.md — 課題B（DeepAI 第1〜5章 遡及レビュー）

担当: Freebuff（課題B）。作業ブランチ `research/freebuff-retro-review`（origin/main `65f2c2b` から分岐）。
成果物: `FREEBBUFF_REVIEW.md`。**コード変更なし**（報告のみの PR）。

## 開始時点の確認（2026-11-15）

- main は `65f2c2b`（PR #318 マージ後）。`git fetch` で origin/main を取得して分岐。
- 依頼文の「#311〜#318 = 第1〜7章」は実際と1つずれている。#316 は照合方針の文書PRで、
  DeepAI 章ではない。対象は #311〜#315 の12枚とした（`FREEBBUFF_REVIEW.md` §0）。
- レビュー用の一時テストはすべて削除済み。`git diff` はクリーン。

## 第1章（#311: C076 / C106 / C111）

- 経路一致: C076 木2=木2、C106 食料2=食料2 で一致。**C111 は legacy `start_round_rewards` が
  未対応で legacy 5 / dispatcher 6 と不一致**（F-2, P2）。
- dispatcher テスト: C106・C111 にあり、C076 にはない。変異テストでは C106 は dispatcher+legacy、
  C111 は dispatcher のみが検出（legacy 側テストが存在しない）。
- 章の主張（新規8件、632 passed）は再測と整合。

## 第2章（#312: C116）

- 経路一致: legacy `cards::play` は2番目の効果を発火しない（実測 木0）。PR 本文・doc コメントに
  明記済みで意図的な差分（F-3, P3）。
- dispatcher テスト: あり。変異（木1→2）も dispatcher テストのみが検出。
- 章の主張（新規2件、634 passed）は再測と整合。

## 第3章（#313: D100 / D105 / D109）

- 経路一致: D105 食料1/小麦1、D109 食料4 で一致。D100 は cards.rs と card_events の legacy 複製が
  同じ結果（4点）。
- **dispatcher テストが0枚**（F-5, P1）。変異は D105/D109 とも legacy テストだけが検出。
- D100 は `--lib` の範囲で閾値変異（4→3）を検出できず（F-5）。3点カテゴリを含むケースが必要。
- D100 のカテゴリ名リストと閾値 `4` が2箇所に重複（F-4, P3）。
- 章の主張（新規4件、638 passed）は再測と整合。

## 第4章（#314: E117）

- 経路一致: 畑フェイズで木2=木2 で一致。
- **dispatcher テストが0枚**（F-5）。変異は legacy テストのみが検出。
- 章の主張（新規2件、640 passed）は再測と整合。

## 第5章（#315: E107 / E111 / E113 / E118）

- 経路一致: E107 食料4、E111（小進歩なし/ありで 食料1/0・木1/0）、E113 野菜1、E118 木1 ——
  すべて legacy と dispatcher で一致。
- **dispatcher テストが0枚**（F-5）。変異は4枚とも legacy テストのみが検出。
- 章の主張（新規4件、644 passed）は再測と整合。

## 横断の発見（最重要）

- **F-1（P0）**: dispatcher が発火サイトを判別していない。`HarvestFieldGain` /
  `HarvestFieldGainIfNoMinorPlayed` / `StartRoundGain*` はすべて After フェイズで条件が常に真のため、
  `dispatch_after_worker` でも発火する。実測:
  - `dispatch_after_worker(DayLaborer)` で C106 食料+2、E117 木+2、E107 食料+4、
    E111 食料+1・木+1、C111 食料+1。
  - エンジン全体: C106 を出して日雇い → 食料 3 → 7（正しくは 5）。
  - DeepAI 以前の B057 でも再現（食料+1）。既存機構の欠陥だが、第1〜5章のカードが露出を増やした。
  - `registry.rs` には `EventKind`（RoundStart/Harvest/WorkerPlaced…）の索引があるが、
    dispatcher が参照していない。サイト判定の設計は既存で、配線が欠けている。
- **F-5（P1）**: 第3〜5章の9枚に dispatcher 経路テストがない（#317 で指摘された問題と同じ作り）。
- 再測: 追加 `#[test]` 数 8/2/4/2/4/5 は各 PR 主張と一致。プリセット35枚の `content_hash` は
  `a5faf2d` と `65f2c2b` で同一（`e62c95c7…`）。v5 フィクスチャ差分なし。
  HEAD の `cargo test --locked` は **649 passed / 0 failed**（PR #317 の主張と一致）。

## 次の再開位置

- 全章（第1〜5章）の報告を `FREEBBUFF_REVIEW.md` に記載済み。追加章は依頼上なし。
- F-1 は状態遷移の中核に触れるため、修正は別 Issue / 別 PR とし、独立再レビューを付すこと。
- F-5 の dispatcher テスト追加は章ごとに分割可能（D帯・E帯で2 PR 程度）。

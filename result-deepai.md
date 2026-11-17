# result-deepai.md — DeepAI 長時間タスク（3部構成）の区切りごとの記録

> 対象リポジトリ: `cueikusuta/agricola-re-ai`。ルールの正本は2016年改訂版 Lookout 公式ルール。
> 本ファイルは部・章ごとに追記する。詳細な根拠は各ブランチの本編文書にある。

## 第1部（2026-11-17）— 遡及レビュー第3弾（監査修正10件のクローズ確認）

- ブランチ: `research/deepai-retro-review-3`（`origin/main` = `1a176a0` から分岐）
- 本編: `DEEPAI_RETRO_REVIEW_3.md`
- 方針: 文書のみ。Rust のプロダクトコードは変更していない。変異・一時テストは復元／削除済み。

### 対象PRの状態（開始時）

| PR | 状態 | CI |
|---|---|---|
| #337 GrowWithMinor | merged | green |
| #340 カードプレイ順序 | open / CONFLICTING | green（当時） |
| #341 sample_game 決定化 | merged | green |
| #334 増分検証 check 6 | merged | green |
| #333 井戸の予約復元 | open / CONFLICTING | green |
| #329 dispatcher サイト区別 | merged | green |
| #335 OnPlayGainPerCount 章 | merged | green |
| #352 停止・再開のセーブ/リプレイ | open / MERGEABLE | **clippy fail** |
| #353 EndGameBonus / E111 / 多重付与 | open / MERGEABLE | **clippy fail** |
| #332 E099 / E100 | open / CONFLICTING（#327/#328 未マージ） | green（当時） |

### 判定の要約

- **閉じた（main）**: #337 の P0 2件、#341 の3件、#334 の check 6、#329 の F-1、
  #335 の P0 4件、#333 の指摘（#341 が同一内容をマージ済み）。
- **未クローズ**:
  - #340 の P1-1a/b/c（main 未反映。PR は conflict）
  - #352 の save/load/replay E2E（二重記録は直ったが、テスト自身が load 失敗を assert）
  - #353 の EndGameBonus（main は依然 `true`）
  - #332 の E100（日英 wiki の条件が OR/AND で矛盾）
- **新規の重大指摘**:
  - P1: #340 の C116 自己発動回帰（ブランチで木2、main は木1。`played[..len-1]` の除外が消えた）
  - P1: #340 の `dispatch_play_after` に `EventKind::CardPlayed` が無い（rebase 後に AfterWorker 誤発火）
  - P1: #352 の「save/load 成功」がテストで裏づけられていない
  - P1: #353 の `use_limit` をイベント種別に分割する設計（実カードは `use_limit: Some` が0枚）
  - P1: #332 の E100 日英矛盾（OR 実装。AND なら最大13点の評価が変わる）
- **P2**: #334 の 6b-ii テスト欠落（変異が生存）、#335 の B006/B007/C005 語義、
  #353 の多重付与テストが無効、#353 の `integrity_hash` 非互換、#332 の spec 欠落、
  #337 の公式典拠不足。
- **P3**: #329 の旧セーブ `event_kind` 既定値、#341 の `pending` 未復元、
  #352 の `start_round` 部分更新、#332 の `start_round` エラー握りつぶし。

### 変異テスト

M1 #337 grow 削除 → 8/12 赤、M2 #334 6b-iii 無効化 → 1赤、
M3 #334 6b-ii 無効化 → **0赤（未検出）**、M4 #329 Harvest サイト → 6赤、
M5 #329 StartRound サイト → 3赤、M6 #341 well/card_uses 空 → 3赤、
M7 #335 C004 cost → 2赤、M8 #340 C116 自己発動 → **ブランチのみ失敗（木2）**。

### マージ推奨

1. #352 / #353 は CI 緑化までマージしない。#353 は E111 の設計をやり直す。
2. #340 は rebase + C116 自己除外 + `EventKind` 設定が必須。
3. #332 は E100 の人間決定までマージしない。
4. #333 は close（#341 で解決済み）。#334 の 6b-ii テストは別小PR。

### 人間決定待ち（第1部で発生）

- H-1 (P1) E100: 「1つ以上」は6種のうち1種（日）か、6種すべて（英）か。
- H-2〜H-4: B006 の supply、B007 の最下行、C005 の improvement 語義（既存 Issue #351/#348/#349）。
- H-5: #337 の家族増加→小進歩の順序の公式典拠。

## 第2部 — 未着手

## 第3部 — 未着手

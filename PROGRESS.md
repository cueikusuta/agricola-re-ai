# PROGRESS — 基本ルール仕様書作成 (Issue #399)

- **ブランチ**: `research/qwen-base-rules-spec`（origin/main `9bbce14` 起点）
- **成果物**: `docs/base-rules-spec.md`（＋このファイル）
- **正本**: Lookout Games 公式『Agricola Revised Edition』（2016）ルールブック＋付録（英語PDF）
  - ルールブック: https://lookout-spiele.de/upload/en_agricolare.html_Rules_Agricola-RE_EN.pdf
  - 付録: https://www.lookout-spiele.de/upload/en_agricolare.html_Appendix_Agricola-RE_EN.pdf
- **PR**: 作成まで。マージ禁止。

---

## 章ごとの進行状況

| 章 | 範囲 | 状態 | コミット | push済み |
|---|---|---|---|---|
| 0 | 目的・範囲・出典・用語・記法 | ✅ 完了 | dcde906 | ✅ |
| 1 | ゲームの準備とプレイヤー数ごとの違い | ✅ 完了 | dcde906 | ✅ |
| 2 | ラウンドとステージの構成 | ⏳ 次 | — | — |
| 3 | アクションスペースの一覧と効果 | ⏳ 予定 | — | — |
| 4 | 家族と労働者の扱い | ⏳ 予定 | — | — |
| 5 | 収穫フェイズ（畑・供給・繁殖） | ⏳ 予定 | — | — |
| 6 | 食料供給と物乞いの処理 | ⏳ 予定 | — | — |
| 7 | 家と改築 | ⏳ 予定 | — | — |
| 8 | 柵と小屋（厩）と農場の拡張 | ⏳ 予定 | — | — |
| 9 | 得点計算の全項目と同点の処理 | ⏳ 予定 | — | — |
| 10 | 補遺: カード基本機構 | ⏳ 予定 | — | — |
| 11 | 既存文書との食い違い一覧 | ⏳ 予定 | — | — |
| 12 | 未確定の一覧 | ⏳ 予定 | — | — |

---

## 現在地

- **作業完了章**: 0, 1
- **次に着手する章**: 2（ラウンドとステージの構成）
- **直近のコミット**: `dcde906` — `docs/spec: base rules spec ch.0-1 — purpose/scope/sources, setup & player count (#399)`
- **ブランチリモート追跡**: `origin/research/qwen-base-rules-spec` (ahead 1)

---

## 未確定・食い違いの見込み（現時点の把握）

| 種別 | 件数（暫定） | 備考 |
|---|---|---|
| 公式と既存文書の食い違い (第11章) | ~12 | rule-decisions/design/source-request 等との差異。章ごとに追加記録。 |
| 未確定項目 (第12章) | ~10 | 公式本文で決まらない・解釈が割れる箇所。推測で確定せず保留。 |

---

## 次のアクション

1. 第2章（ラウンドとステージの構成）を書く → コミット＋push → PROGRESS.md 更新
2. 以降、同様に章ごとに進める
3. 全章完了後、PR作成（#399 を参照）
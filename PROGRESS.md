# PROGRESS.md — 現在地（エージェント別）

このファイルは担当エージェントごとに節を分けて追記する。相手の節・ブランチには触れない。
（main には未作成のため、本タスクで新規作成した。他担当の節は各担当ブランチから追記される。）

## DeepAI — 実装×仕様JSON 突合監査（ブランチ `research/deepai-impl-spec-audit`）

対象 `origin/main` = `89c38ab`。main にマージ済みの実装カード **77枚**（A29/B21/C10/D8/E9）を
`registry::build_default_registry()` から同定し、仕様JSON・実装・テスト契約を突き合わせる。

- 監査方針・対象同定手順・判定基準・全77枚のサマリ・横断所見・人間チェックリスト:
  `docs/impl-spec-audit.md`
- 章ごとの結果: `result-deepai-impl-spec-audit.md`（区切りごとに追記）
- 章ブランチ（すべて基底 `research/deepai-impl-spec-audit` から分岐）:
  - 第1章 A帯: `research/deepai-impl-spec-audit-A`
  - 第2章 B帯: `research/deepai-impl-spec-audit-B`
  - 第3章 C帯: `research/deepai-impl-spec-audit-C`
  - 第4章 D帯: `research/deepai-impl-spec-audit-D`
  - 第5章 E帯: `research/deepai-impl-spec-audit-E`

### 現在地

- [x] 基底: 監査方針・対象同定（77枚）・全体サマリ・横断所見・質問チェックリスト
- [x] 第1章 A帯（29枚）: 不一致2を修正（A042 森のみ／A077 他家の豚市場）。#411
- [x] 第2章 B帯（21枚）: 不一致2を修正（B050 印刷点1／B061 前提=職業3）。#412
- [x] 第3章 C帯（10枚）: 不一致0。C005 の論点を Issue #349 へ追記。#413
- [x] 第4章 D帯（8枚）: 不一致4を修正（D004／D005／D105／D100）。#414
- [x] 第5章 E帯（9枚）: 不一致0。仕様未作成5と Q-5/Q-6 を記録（PR作成）

全5章完了。残件は `docs/impl-spec-audit.md` §5（横断）・§6（Q-0〜Q-9）と各章の「修正せず記録」に集約した。
再開するときは、この「現在地」と `result-deepai-impl-spec-audit.md` の末尾の章から続ける。

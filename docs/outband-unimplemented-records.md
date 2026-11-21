# 帯内未実装カード・実装済み記録漏れカードの個別記録

**作成日**: 2026-11-19  
**基準コミット**: `origin/main` = `82e6fb5e4f31f964ec2edf2fff4fa1750cef2dd4`  
**参照分類文書**: PR #376 (`docs/outband-classification.md`, head `6d97269a`)  
**参照仕様JSON**: `data/bga-card-specs/` (main 時点)  
**参照台帳**: `data/bga-card-base-ledger.csv`  
**参照一覧表**: `docs/card-catalog-ae-preliminary.csv`

---

## 目的

外部レビュー（Arena 読み取り専用）で指摘された 6 枚（帯内未実装 4 枚 ＋ 実装済み記録漏れ 2 枚）について、分類・仕様・実装状況・未実装理由・必要前提・次の一手を 1 枚 1 節で記録する。分類のやり直し・段階の再判定は行わず、PR #376 の記載をそのまま引用する。

---

## A109 小商人

### 基本情報
- **ID**: A109
- **名称**: 小商人 / Small Trader
- **種別**: 職業
- **デッキ**: A
- **難易度**: 3 (`docs/card-catalog-ae-preliminary.csv` の列)
- **仕様JSON**: あり (`data/bga-card-specs/A109.json`, `review.status = bga_checked`, `implementation_status = not_started`)

### PR #376 での分類・扱い
- **段階**: 段階外（単一サイト: 進歩 / 獲得）
- **基盤**: TR-SITE
- **引用元**: PR #376 head `6d97269a` 第1章 Aデッキ分類表「A109 小商人 | 段階外 | 単一サイト: 進歩 / 獲得 | TR-SITE」
- **仕様データ残件**: なし（仕様JSON は完成、印刷点・種別・前提は台帳・wikiと一致）

### 実装状況（実測）
- **src/ 実装**: **あり** (本 PR `12dc2018` にて実装済み: `EventKind::ImprovementPlayed`, `CardEffect::AfterImprovementPlayedGain`, `dispatch_play` Phase 4b スキャン, `verified_band_a_cards()` 登録)
- **カードプール**: `verified_band_a_cards()` に登録済み
- **テスト**: `tests/ch04_after_improvement_played.rs` に 8 件 (正系・不発・境界・家族成長経路不発・MajorImprovement経路発火)

### 未実装の理由（base 時点）
1. **トリガーイベントが main に存在しない**: `EventKind` に「進歩プレイ」に相当するイベントがない（現状: `WorkerPlaced` / `CardPlayed` / `RoundStart` / `Harvest` / `ClayConstruction` / `Bake` のみ）。
2. **仕様のトリガー**: `after_improvement` / `improvement_played_from_hand` / `via_major_or_minor_action: true` が必要。
3. **保留理由の個別記録がない**: PR #312 等の「難易度3 残り21枚は保留記録」のようなカテゴリ単位の記録のみで、A109 固有の保留理由は DEEPAI_PROGRESS.md / 実装状況文書 / PR #311〜#318 / #376 いずれにも記載なし。

### 実装に必要な前提
| 前提 | 状態 | 備考 |
|---|---|---|
| `EventKind::ImprovementPlayed` (新規) または `CardPlayed` への payload 追加 | **済 (本 PR)** | enum 最後尾に追加、`CardPlayed` payload 拡張なし方針 |
| `CardEffect::AfterImprovementPlayedGain { good, amount }` (新規 variant) | **済 (本 PR)** | enum 最後尾に追加、`#[serde(rename_all = "snake_case")]` 継承 |
| `effect_phase` / `effect_condition_holds` への対応 | **済 (本 PR)** | After phase で発火、`trigger.event_kind == EventKind::ImprovementPlayed` 判定 |
| `dispatch_play` への後処理スキャン追加 | **済 (本 PR)** | Phase 4b で改進歩プレイ時に走査、`is_improvement_action` フラグで経路限定 |
| `verified_band_a_cards()` への A109 定義追加 | **済 (本 PR)** | Band A プールに登録 |

### 次の一手
**本 PR で実装完了**。`A109.json` の `implementation_status` を `not_started` → `implemented` に更新するのは補完2 (`t_28256d65`, PR #370 付録A) の担当。本 PR では spec の更新は行わない（Issue #390 で人間の決定待ち）。

---

## A118 樹木庭師

### 基本情報
- **ID**: A118
- **名称**: 樹木庭師 / Treegardener
- **種別**: 職業
- **デッキ**: A
- **難易度**: 3
- **仕様JSON**: あり (`data/bga-card-specs/A118.json`, `review.status = bga_checked`, `implementation_status = not_started`)

### PR #376 での分類・扱い
- **段階**: 段階外（単一サイト: 収穫・畑 / 獲得＋購入）
- **基盤**: TR-SITE
- **引用元**: PR #376 head `6d97269a` 第1章 Aデッキ分類表「A118 樹木庭師 | 段階外 | 単一サイト: 収穫・畑 / 獲得＋購入 | TR-SITE」
- **仕様データ残件 (R-A2)**: 
  - 第1効果の `frequency: once_per_play` は誤り。文言「各収穫の畑フェイズのたび」→ `every_harvest` が正しい
  - 「木材2まで」の購入上限が仕様に表現されていない

### 実装状況（実測）
- **src/ 実装**: なし (`git grep "A118" src/` = 0 件)
- **カードプール**: `verified_band_a_cards()` に未登録
- **テスト**: なし
- **関連 PR**: **PR #385** (created 2026-11-19T12:48:28Z, branch `m10/hermes-ch-harvest-followup`, A118/A145/C066) が存在し、`t_49f032aa` は done。

### 未実装の理由
1. **別カード担当（スケジュール）**: ボード上の「実装: A118/A145/C066 収穫系追従3枚」（`t_49f032aa`, worker-kilo, ready）が A118/A145/C066 を1章で扱う実装カード。PR #385 で実装済み、`t_49f032aa` は done。
2. **仕様 JSON の修正が前提**: `frequency` 修正と購入上限の表現追加が必要。`A118.json` は `scripts/build_bga_card_specs.py` の curated 対象のため、単独編集すると `bga-spec-contract` が赤くなる（builder 側も同時修正が必要）。
3. **基盤**: `EventKind::Harvest` と既存 `PendingKind::ChooseGain`/`Custom` で実装可能（`t_49f032aa` の設計どおり）。新規イベントは不要。

### 実装に必要な前提
| 前提 | 状態 | 備考 |
|---|---|---|
| 仕様 JSON 修正 (`frequency`, 購入上限) | **未済** | builder 同時修正必要 |
| `HarvestFieldRule::Fixed` (新規 enum variant) | **未済** | `t_49f032aa` 設計で追加予定 |
| 任意交換の決定フロー (`PendingKind::ChooseGain` 優先) | 既存 | 再利用可能 |

### 次の一手
**本カードでは実装しない**（`t_49f032aa` / PR #385 の担当）。本カードでは記録のみを行う。PR #385 (A118/A145/C066) の完了を以て実装済みとする。

---

## C088 見習い大工

### 基本情報
- **ID**: C088
- **名称**: 見習い大工 / Carpenter's Apprentice
- **種別**: 職業
- **デッキ**: C
- **難易度**: 3
- **仕様JSON**: **なし** (`data/bga-card-specs/` に C088.json 存在せず。台帳行と `docs/card-catalog-ae-preliminary.csv` 行のみ)

### PR #376 での分類・扱い
- **段階**: 段階3 部屋（コスト軽減）
- **基盤**: ROOM
- **引用元**: PR #376 head `6d97269a` 第2章 Cデッキ分類表「C088 見習い大工 | 3 | 段階3 部屋（コスト軽減） | ROOM」
- **実装準備メモ (§5.3.2)**: 「木の部屋のコストを木材2軽減。3・4軒目の厩を木材1軽減。13〜15本目の柵を無料にする。ROOM(部屋コスト)＋DEF(厩・柵の本数別コスト)。3つの軽減を1枚で持つため順序を LOG に残す。」

### 実装状況（実測）
- **src/ 実装**: なし (`git grep "C088" src/` = 0 件)
- **仕様 JSON**: 存在しない
- **カードプール**: `verified_band_c_cards()` に未登録
- **テスト**: なし

### 未実装の理由
1. **仕様 JSON が存在しない**: 仕様が正本である以上、仕様作成 (`data/bga-card-specs/C088.json` + tests JSON、`scripts/validate_bga_card_specs.py` 緑) が前提。
2. **費用軽減の順序記録 (LOG) が前提**: 3種類の軽減（木の部屋/厩/柵）を適用順序で記録する必要があり、現状の `PassiveRule` にはこの粒度の軽減バリアントがない。
3. **基盤が main にないまま近似実装しない方針**: 近似で通すと後で作り直しになる。

### 実装に必要な前提
| 前提 | 状態 | 備考 |
|---|---|---|
| 仕様 JSON 作成 (`data/bga-card-specs/C088.json`) | **未済** | 費用・前提・効果・テストケースを完全定義 |
| `PassiveRule` への複合軽減バリアント追加 | **未済** | 部屋/厩/柵それぞれの本数依存軽減、適用順序 LOG 記録 |
| `effect_phase` / `effect_condition_holds` 対応 | **未済** | 常時効果として Before/After 判定 |
| `LOG` 基盤（順序記録） | **未済** | PR #376 §5.3.2 で「順序を LOG に残す」と明記 |

### 次の一手
**仕様 JSON 作成が完了するまで実装しない**。本カードの記録に「仕様未作成のため実装不可（仕様作成が前提）」と理由を書き、PR 本文の残件欄に送る。仕様作成は別 Issue/PR で実施。

---

## C115 種まき人

### 基本情報
- **ID**: C115
- **名称**: 種まき人 / Sower
- **種別**: 職業
- **デッキ**: C
- **難易度**: 3
- **仕様JSON**: **なし** (`data/bga-card-specs/` に C115.json 存在せず。台帳行と `docs/card-catalog-ae-preliminary.csv` 行のみ)

### PR #376 での分類・扱い
- **段階**: 段階4 P6（蓄積＋取り出し）
- **基盤**: PS, TR-SITE, STACK
- **引用元**: PR #376 head `6d97269a` 第2章 Cデッキ分類表「C115 種まき人 | 4 | P6 蓄積＋取り出し | PS, TR-SITE, STACK」
- **実装準備メモ (§5.4.3)**: 「大進歩を建てるたびカードに葦1を置く。いつでもその葦を取るか、種まきアクションに替えられる。TR-SITE(ImprovementBuilt)＋PS(カード上在庫)＋ anytime の選択（取得 or GRANT(Sow)）」

### 実装状況（実測）
- **src/ 実装**: なし (`git grep "C115" src/` = 0 件)
- **仕様 JSON**: 存在しない
- **カードプール**: `verified_band_c_cards()` に未登録
- **テスト**: なし

### 未実装の理由
1. **仕様 JSON が存在しない**: 仕様が正本である以上、仕様作成が前提。
2. **カード上在庫 (`card_accumulations`) が前提**: PR #371 (merged 2026-11-19T06:20Z, `feat/card-accumulation-state`) で追加済み、**main マージ済み**。main に在庫状態がある。
3. **基盤が main にないまま近似実装しない方針**: 近似で通すと後で作り直しになる。

### 実装に必要な前提
| 前提 | 状態 | 備考 |
|---|---|---|
| 仕様 JSON 作成 (`data/bga-card-specs/C115.json`) | **未済** |  |
| `PublicState.card_accumulations` (PR #371) マージ | **済 (2026-11-19T06:20Z)** | カード上の蓄積リソース台帳 |
| `TR-SITE(ImprovementBuilt)` イベント拡張 | **未済** | 大進歩建設時のトリガー |
| `GRANT(Sow)` 種まきアクション付与 | **未済** | 既存の種まきアクション付与機構があれば再利用可 |

### 次の一手
**仕様 JSON 作成が完了するまで実装しない**。本カードの記録に「仕様未作成のため実装不可（仕様作成が前提）・カード上在庫基盤 (PR #371) は 2026-11-19T06:20Z に main マージ済み」と理由を書き、PR 本文の残件欄に送る。

---

## A088 生け垣管理人

### 基本情報
- **ID**: A088
- **名称**: 生け垣管理人 / Hedge Keeper
- **種別**: 職業
- **デッキ**: A
- **難易度**: 4 (`docs/card-catalog-ae-preliminary.csv` の列。台帳 `rating` 8.00 は評価点であり難易度ではない)
- **仕様JSON**: あり (`data/bga-card-specs/A088.json`, `review.status = bga_checked`, `implementation_status = implemented`)

### PR #376 での分類・扱い
- **段階**: 実装済
- **引用元**: PR #376 head `6d97269a` 第1章 Aデッキ分類表「A088 生け垣管理人 | 実装済 | 実装済み（cards.rs） | —」
- **仕様データ残件**: なし

### 実装状況（実測）
- **src/ 実装**: **あり** (`src/cards.rs:2410` `PassiveRule::FenceWoodDiscount(3)`)
- **カードプール**: `verified_passive_cards()` 経由で登録済み
- **テスト**: `passive_cards_tests::a088_fence_discount_three` あり
- **spec/台帳**: ともに `implementation_status = implemented`

### 記録欠落の内容
- **DEEPAI_PROGRESS.md**: 記載 0 件
- **指摘の「文書に未記載」は実装欠落ではなく記録側の欠落**: 実装は完了しており、テストも緑。個別の保留理由記録がないのは PR #312 類のカテゴリ単位記録（難易度4 は別扱い）のみだったため。

### 記録補正
本記録により「実装済み・テスト済み・配布対象（verifed_passive_cards 経由）」として正式に記録する。DEEPAI_PROGRESS.md は編集しない（open PR 6 本と衝突するため）。

---

## A110 仕立屋

### 基本情報
- **ID**: A110
- **名称**: 仕立屋 / Roughcaster
- **種別**: 職業
- **デッキ**: A
- **難易度**: 4
- **仕様JSON**: あり (`data/bga-card-specs/A110.json`, `review.status = bga_checked`, `implementation_status = implemented`)

### PR #376 での分類・扱い
- **段階**: 実装済
- **引用元**: PR #376 head `6d97269a` 第1章 Aデッキ分類表「A110 仕立屋 | 実装済 | 実装済み（cards.rs） | —」
- **仕様データ残件 (R-A4)**: `after_clay_construction` 1 トリガーで「レンガの部屋の建設」と「レンガの家→石の家の改築」の 2 事象を表現。payload で区別できない。

### 実装状況（実測）
- **src/ 実装**: **あり** (`src/cards.rs:2560` `verified_simple_occupations()` 内 `AfterClayConstructionGain { food, 3 }`)
- **カードプール**: `verified_simple_occupations(players)` 経由で登録済み（2人用以外でも含まれる）
- **テスト**: 派生テストでカバー（`dispatch_after_clay_construction` 経路）
- **spec/台帳**: ともに `implementation_status = implemented`

### 記録欠落の内容
- **DEEPAI_PROGRESS.md**: 記載 0 件
- **指摘の「文書に未記載」は実装欠落ではなく記録側の欠落**: A088 と同様。

### 記録補正
本記録により「実装済み・テスト済み・配布対象（verified_simple_occupations 経由）」として正式に記録する。仕様データ残件 R-A4（トリガー粒度不足）は別 PR で仕様 JSON 修正として扱う。

---

## まとめ: 実装優先順位と残件

| 優先度 | カード | 実装可否 | 理由 / ブロッカー |
|---|---|---|---|
| **1 (最優先)** | **A109** | **可** | 仕様完成、イベント基盤追加のみで実装可能 |
| 2 | A118 | 本カード外 | `t_49f032aa` 担当、仕様修正前提 |
| 3 | C088 | **不可** | 仕様 JSON なし、LOG 基盤なし |
| 4 | C115 | **不可** | 仕様 JSON なし、カード上在庫 (PR #371) 未マージ |
| — | A088 | 記録補正のみ | 実装済み、記録漏れを補正 |
| — | A110 | 記録補正のみ | 実装済み、記録漏れを補正 |

**本 PR で実装するのは A109 のみ**。C088/C115 は仕様作成・基盤マージ待ち、A118 は別カード担当、A088/A110 は記録補正のみ。

---

## 付録: 照合に使用したコマンドと根拠

```bash
# (a) src/ 実装確認
git grep -n '"A109"\|"A118"\|"C088"\|"C115"\|"A088"\|"A110"' 82e6fb5 -- src/

# (b) spec JSON 確認
ls data/bga-card-specs/ | grep -E "A109|A118|C088|C115|A088|A110"
cat data/bga-card-specs/A109.json | jq .review

# (c) 保留記録確認
grep -E "A109|A118|C088|C115|A088|A110" DEEPAI_PROGRESS.md

# (d) 台帳・一覧表確認
grep -E "A109|A118|C088|C115|A088|A110" data/bga-card-base-ledger.csv
grep -E "A109|A118|C088|C115|A088|A110" docs/card-catalog-ae-preliminary.csv

# PR #376 分類引用
curl -H "Authorization: Bearer $(gh auth token)" \
  -H "Accept: application/vnd.github.v3.raw" \
  "https://api.github.com/repos/cueikusuta/agricola-re-ai/contents/docs/outband-classification.md?ref=6d97269a"
```
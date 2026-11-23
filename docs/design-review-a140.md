# A140 ショベル運搬人 実装設計レビュー

## 概要

A140「ショベル運搬人」（Shovel Bearer）は、2016年改訂版アグリコラの職業カード（A帯）で、**3〜4人戦のみ使用可能**なカードです。効果は「自分が粘土採取場（Clay Pit）または窪地（Hollow）を使用したとき、**もう一方の粘土蓄積スペースに残っている粘土の数だけ食料を獲得する**」というものです。

## 公式ルール確認事項（要人間判断）

| 項目 | 内容 | 要確認 |
|------|------|--------|
| カード種別 | 職業 | ✓ |
| 使用可能人数 | 3〜4人（2人不可） | ✓ |
| コスト | なし | ✓ |
| 点数 | 0点 | ✓ |
| 発動タイミング | ワーカー配置後（AfterWorker フェイズ） | ✓ |
| 発動条件 | 自分が Clay Pit または Hollow を使用したとき | ✓ |
| 効果 | 使用しなかった方の蓄積スペースの粘土数＝食料獲得 | **要確認** |
| 「残っている粘土」の定義 | コア アクションの粘土取得**後**の残量（= 公開蓄積マップの値） | **要確認** |
| 任意/強制 | 強制（"gain" 表記） | ✓ |
| 対象 | カード所有者のみ | ✓ |

> **公式典拠**: agricola.cloudfree.jp ?p=3285（BGA wiki 第三者起こし）。公式 PDF / カード原文照合は Issue #91 等で別途実施。

## 既存実装の現状（未コミット差分）

### 実装済み項目
1. **CardEffect variant**: `AfterWorkerGainOtherAccumulationFood { space: Space, good: Good }`
2. **ハンドラ**: `handle_after_worker_gain_other_accumulation_food`（`card_events.rs`）
   - 使用スペースと反対側を決定（Clay Pit ↔ Hollow）
   - `DispatchContext.accumulations` から反対側の粘土数を読取
   - 0 なら何もしない、欠損キーも 0 扱い
   - `ctx.player.supply` に食料を加算
3. **カード定義**: `verified_band_a_cards()` に A140 追加
   - `MinimumPlayers(3)` 前提条件
   - Clay Pit 用・Hollow 用の 2 エントリを同一カードに同梱
4. **ディスパッチャ連携**: `dispatch_after_worker` が `accumulations` を `Some(&map)` でスレッド
5. **ポリシー推定値**: 中盤典型蓄積 2 粘土として `card_good_value(Food) * 2 * horizon / 2`
6. **テストファイル**: `tests/a140_shovel_bearer.rs` 作成済み

### 未完了・要修正項目
| # | 項目 | 状態 | 対応 |
|---|------|------|------|
| 1 | テスト `a140_counts_only_the_other_accumulations_clay` | **未完了**（タイポ `set_with_aiego_dummy_placeholder`） | `set_with_a140()` へ修正＋アサーション追加 |
| 2 | ChooseTriggerOrder pause/resume + save/load + replay テスト | 未実装 | M8 受入条件「再演一致」用に必須 |
| 3 | エンジン経路統合テスト（`Game::apply` 経由） | 未実装 | `tests/a140_shovel_bearer.rs` に追加 |
| 4 | 2人戦での不適格確認テスト | 未実装 | `CardSet::projected_for_players(2)` で A140 が除外されること確認 |
| 5 | 公式ルール照合記録 | 未完了 | Issue #91 へ移管、PR 本文に典拠 URL 記載 |

## 設計上の論点と決定案

### 1. 蓄積マップのスレッド方法（現状維持で可）
- `Game::apply` 内の resume パス、通常パス双方で `&next.public.accumulations` / `&self.public.accumulations` を渡している
- **理由**: ポーズ後の再開時も「コア アクションの取得後」状態が `public.accumulations` に反映済みのため、同一マップを参照すれば整合

### 2. 反対側蓄積の語彙（現状維持で可）
- `Space::ClayPit` と `Space::Hollow` のみを対象とし、他スペースは早期リターン
- **理由**: カード文面が「粘土蓄積スペース」を Clay Pit と Hollow のみ指すため（BGA spec `spaces: ["clay_pit", "hollow"]` と一致）

### 3. 動的量のバリデーション除外（現状維持で可）
- `CardSet::validate` の amount チェックで `AfterWorkerGainOtherAccumulationFood` を除外（値 1 でダミー合格）
- **理由**: 実行時まで量が確定しないため、構造検証では「実行可能」のみ確認

### 4. ポリシー推定値（暫定 2 粘土で可、要調整余地）
- 現状: `card_good_value(Food) * 2 * horizon / 2`（条件付き割引 1/2 適用）
- **懸念**: 実測分布（2人戦 OFF 時 Clay Pit 平均約 1.8、Hollow 平均約 0.9）と乖離する可能性
- **方針**: M12 ベンチマークで実測平均を取得し、ヒューリスティックを校正する Issue を切る

### 5. BGA spec との整合（概ね合致）
| spec キー | 実装 | 備考 |
|-----------|------|------|
| `trigger: "after_worker_space"` | `EventKind::WorkerPlaced` + After フェイズ | ✓ |
| `conditions: worker_space_used { spaces: ["clay_pit", "hollow"] }` | `effect_condition_holds` で space 一致判定 | ✓ |
| `steps: gain_resource { good: "food", amount: { op: "count", args: ["clay_on_other_accumulation"] } }` | ハンドラで反対側蓄積を count | ✓ |
| `optional: false` | 強制実行 | ✓ |
| `frequency: "every_use"` | 使用ごとに発動 | ✓ |
| `duration: "permanent"` | カードが場にある限り | ✓ |

## 受け入れ基準（M8/M10 観点）

| ゲート | 基準 | 現状 |
|--------|------|------|
| **ルール正確性** | 公式期待結果テスト・不変条件・複合効果テスト全緑 | ハンドラ単体テストのみ、統合テスト不足 |
| **詳細行動 API** | `SimpleDetails` 外の合法手も受理・検証可能 | A140 は自動適用（選択なし）なので非該当 |
| **再演/秘匿** | ポーズ・再開・保存・復帰で状態ハッシュ一致 | 未検証（要テスト追加） |
| **初期評価基盤** | 1,000試合完走・異常終了ゼロ・速度計測 | カード単体では非該当（M8-3 で網羅） |
| **M10 カードプール** | 3〜4人適法職業 40 枚以上の一部 | 実装完了後にカウント対象 |

## 推奨次のアクション（実装担当へ）

1. **即時修正**
   - `tests/a140_shovel_bearer.rs:178` のタイポ修正＋テスト完成
   - ChooseTriggerOrder pause/resume + save/load + replay テスト追加
   - `Game::apply` 経由の統合テスト追加（3人戦・追加タイル ON/OFF 両方）

2. **公式照合完了まで配布不可扱い**
   - `CardSet::validate` は通るが、`CardPool` への登録は Issue #91 解決まで保留
   - `verified_band_a_cards()` には残すが、PR 本文に「公式照合未了」を明記

3. **ポリシー校正 Issue 切り出し**
   - M12 ベンチマーク後に実測平均でヒューリスティック更新

4. **ドキュメント同期**
   - `docs/design.md` のカード実装状況表（あれば）更新
   - `DEEPAI_PROGRESS.md` の A140 行を「実装済み・テスト未完・照合未了」へ更新

## 判定

**条件付き承認（Conditional Approve）**

- 実装の骨子は設計原則（ルールエンジン分離、動的量の実行時読取、公開情報のみ参照）に準拠
- テスト未完・公式照合未了のため、**テスト完成＋公式照合 Issue 記録**をマージ前提条件とする
- 実装担当は上記「推奨次のアクション」を同一ブランチ内で完了させ、自己レビュー・CI 緑を確認してから PR 作成

---

**レビュー日**: 2026-11-23  
**レビューア**: 設計レビュー担当（初回独立レビュー）  
**対象ブランチ**: `m10/hermes-ch-a140-clay-mirror`  
**関連 Issue**: #91（公式ルール照合）、#105（4人カードプール）、#108（M8-1 ルール監査）
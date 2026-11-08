# 合法詳細行動APIの現行契約（M8-2 / #109）

更新: 2026-11-09。対象: `main` の現行Rust実装。**これは実装済みAPIの棚卸しであり、未実装APIを完成と見なす仕様変更ではない。** 詳細行動の受け入れは2016年改訂版ルールに基づく共通エンジンが判定する。

## 1. 責務と呼び出しの流れ

```text
Game の完全な状態
   |
   +-- available_spaces()  構造上の利用可能スペース（人間UI向け）
   |         |
   |         +-- detail_hints(space) / fence_hints() / hints::*()
   |         |       追加情報・候補・制約の提示（合法手集合ではない）
   |         |
   |         +-- 人間が Action{space, detail} を構築
   |                         |
   |                         +-- validate_action(&action) / preview_harvest(...)
   |                         |       状態を変更しない事前検査・収穫プレビュー
   |                         |
   |                         +-- apply(action) / apply_with_harvest_policy(...)
   |                                 共通Rustエンジンが確定・記録
   |
   +-- legal_spaces()    AI候補へ絞ったスペース（全合法スペースではない）
               |
               +-- AI詳細方針の提案 → 共通Rustエンジンが合法性検査
```

`SimpleDetails` はCPU向けの既定詳細生成ポリシーであり、合法詳細行動全体の定義ではない。AI向け候補に含まれない手を、候補外という理由だけで不正としない。

## 2. 現在存在するRust APIと境界

| API | ファイル | 現時点の責務・限界 |
|---|---|---|
| `Game::available_spaces()` | `src/engine.rs` | ラウンド公開・未使用・追加タイルの排他制約による構造的候補。詳細パラメータが合法かの証明ではない |
| `Game::legal_spaces()` | `src/engine.rs` | AIなどの候補生成に使う絞り込み済みスペース。全合法詳細行動の列挙を保証しない |
| `Game::detail_hints(space)` / `Game::fence_hints()` | `src/engine.rs` | 農場・詳細入力のヒント。候補集合をエンジンの合法行動集合に昇格させない |
| `hints::feeding_hints(player)`, `breeding_hints(player)`, `free_action_hints(player)` | `src/hints.rs` | 給餌・繁殖・変換などの入力補助。入力提案は最終合法性検査の代用ではない |
| `Game::validate_action(&Action)` | `src/engine.rs` | `self`を複製し、`apply_with_plans`を試す読み取り専用の事前検査。実行後の`self`は不変 |
| `Game::preview_harvest(&Action, Option<&[FeedingPlan]>)` | `src/engine.rs` | 収穫直前の配置・畑収穫・指定給餌から`PublicState`を試算。最終的なゲーム確定・全繁殖選択を代替しない |
| `Game::apply(action)` / `apply_with_harvest_policy` | `src/engine.rs` | 共通の状態遷移、必要な収穫処理。コミットされた行動はイベントに記録され、再演経路で検証される |
| `Game::apply_free(player, FreeAction)` | `src/engine.rs` | フリーアクションをコピー上で実行・`validate()`した後に原子的に確定。失敗時は`self`を置換しない |

**重要**：`validate_action`は読み取り専用だが、別のスナップショットで確認した詳細が、その後の状態で必ず合法とは限らない。確定時のエンジン検証を省かない。

## 3. ブラウザ・CPU・観測

`src/web.rs` はローカル単一人間UIとして`POST /api/play`、`POST /api/free`、`POST /api/harvest`を提供し、Rust側に処理を委ねる。UIの入力ヒントやCPUの詳細候補が妥当性判定を確定するわけではない。

観測用の`Observation`は観測者本人の手札だけを含む。相手の非公開手札、未公開ラウンド順、内部seedを、ヒント・AI方針の入力から取得する設計にしない。拒否された行動ではゲーム状態・ログの整合性を維持する。保存復元やイベント再演は`Game::load_json`と`Game::replay`等の既存検証経路で照合する。

## 4. 代表テストと未達条件

既存の代表テスト:

- `tests/m8_legal_detail_api.rs`: `SimpleDetails`外の合法詳細を受け入れ、不正な柵・畑・種まき・家畜配置を状態不変で拒否する例
- `tests/m8_human_hints.rs`、`tests/renovation_details.rs`、`tests/farm_animals.rs`、`tests/farm_crops.rs`、`tests/farm_buildings.rs`、`tests/harvest_feeding.rs`: 個別の合法性・UI支援・収穫を検証する既存ファイル
- `docs/m8-evidence-requirements.json` / `docs/m8-evidence.md`: G0の証拠対象とテスト実走結果を対応付ける。形式上の証拠があっても全合法手の形式的証明にはならない

**引き続き未実装または未完了**（#109のフォローアップとして別PRで扱う）:

1. `Game::validate_free(&self, player, &FreeAction)`相当の公開・読み取り専用フリーアクション事前検査。現在は`apply_free`が原子的に拒否するが、実行前プレビューAPIが存在するとは主張しない。
2. 畑・柵・種まき・部屋/厩・改築・家畜・フリーアクションの**横断的な合法/不正マトリクステスト**（給餌・繁殖は別プレビュー経路）。個別テストと網羅テストを混同しない。
3. 家畜収容先、改善費用、給餌順、繁殖前配置換えなどの段階的ヒント拡充。ヒント強化は人間が入力できる合法詳細の集合を制限しない。
4. 公式ルールの全状態・全詳細の形式的証明。各モードで1,000試合完走してもこれに代わらない。

本書はコードを変更せず、現行実装の入力・検証・確定の責務境界のみを固定する。受け入れ条件と引き続き残る課題は[#109](https://github.com/cueikusuta/agricola-re-ai/issues/109)、本書の分割元は[#203](https://github.com/cueikusuta/agricola-re-ai/issues/203)。

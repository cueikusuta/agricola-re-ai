# M1受け入れチェック表（2016年リバイズド・2人戦）

この文書は2026-11-04時点の実装候補に対する検証状況をまとめる。**チェックの「コード・CI確認済」は公式ルールの全状態空間の形式的証明ではない。**

| Issue | 仕様・対象 | 実装/代表的な検証 |
|---|---|---|
| #1 | 5×3農場、初期2部屋/家族2人、用途別乱数、公開観測、保存/復元 | `src/model.rs`, `tests/m1.rs` |
| #2 | 合法手→詳細→検証→適用、フリーアクション、不正操作の原子性 | `src/engine.rs`, `tests/legal_invariants.rs` |
| #3 | ステージ別14ラウンド、開始プレイヤー、追加4択ON/OFF | `src/model.rs`, `tests/round_additional.rs` |
| #4 | 部屋・改築・厩・家族増員 | `src/engine.rs`, `tests/farm_buildings.rs`; 厩建設位置の追加テスト（#45）|
| #5 | 畑の連結・種まき・収穫・複合耕作 | `src/engine.rs`, `tests/farm_crops.rs` |
| #6 | 牧場の柵・分割/複雑分割、家畜収容/移動 | `src/engine.rs`, `tests/farm_animals.rs`; #23/#31/#41 |
| #7 | 主要改善10枚、資源払い/交換、食料化、井戸・工房・窯 | `src/engine.rs`, `tests/official_improvements.rs`; #27 |
| #8 | 収穫6回、給餌・工房加工・繁殖・物乞い | `src/harvest.rs`, `src/breeding.rs`; #25/#29/#37/#39 |
| #9 | 得点表・マイナス点・同点の建築資源比較 | `src/score.rs`, `tests/scoring_categories.rs` |
| #10 | Random/Greedy・公開情報だけで判断・詳細方針交換 | `src/policy.rs`, `tests/policy_api.rs` |
| #11 | 固定シード・イベントログ/再生・保存復元・性能分布 | `tests/integration_acceptance.rs`, GitHub Actions benchmark; #33/#43 |

## 公式ルールの確認点

- ルールブックp.8: 牧場は柵で囲み、厩はタイルのない場所に建築。先に建てた厩は後から囲える。囲われた厩は容量を倍増。
- ルールブックp.12: 有限のコマとして厳密に制約するのは家族5人・厩4個・柵15本で、共通資源は物理コマ不足で上限にしない。
- 付録p.2: 主要改善10枚、調理場の資源購入/かまど交換、パン窯、工房、井戸。
- 付録p.7–8: 各アクションの任意/必須、追加アクションタイル4択と条件。
- 付録p.11–12: 得点カテゴリ、残資源ボーナスと物乞い。

公式PDF（収録せず参照のみ）:
- https://lookout-spiele.de/upload/en_agricolare.html_Rules_Agricola-RE_EN.pdf
- https://lookout-spiele.de/upload/en_agricolare.html_Appendix_Agricola-RE_EN.pdf

## 自動受け入れゲート

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all-targets`
4. 追加アクションスペースOFF/ON各100試合のリリースビルド計測
5. 固定シードのRandom/Greedy完走、ログ再現、保存復元、観測情報遮断

## 残る非自動条件

- 全アクション・全配置・全状態の形式的証明は実施していない。
- 外部独立レビュアーの検証結果は未取得。重要なルール解釈が新たに判明した場合はこの表を更新する。
- **mainへのマージとIssueのクローズは人間レビュー後**。このPRをマージせずとも統合ブランチでRustプログラムとして実行できる。

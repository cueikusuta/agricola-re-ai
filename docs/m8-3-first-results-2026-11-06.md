# M8-3: GitHub Actions大量対戦の初回実走記録（2026-11-06）

本ファイルは **GitHub Actionsで実行済み** の試験結果を永続化するための記録であり、ルール正確性・全モード・既存M7 AI比の強さの検証完了を意味しない。

## 1. カードなし4人戦・1,000試合の完走スモーク

- GitHub Actions Run: [#42105282438](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42105282438)（全ジョブ成功）
- 実行専用branch: `verify/mass-1000-20261106`; 実行SHA `ed3337618cc5543d2099899229ad6bf195df1851`
- 環境：GitHub-hosted `ubuntu-24.04` 標準CPU、4シャード×250試合
- 対象：カードなし、4人、追加タイルOFF、全席`random`、seed 420000〜420999
- 結果：**1,000試合**が各シャードと集約で成功。失敗seed・集約不一致なし。
- Rustゲーム部分の平均時間：**0.01073325秒/試合**（約93.17試合/秒/ランナー相当、ビルド/キュー待ち・集約を含まない）。CPU種類・負荷で変動するため性能保証ではない。
- Artifact：`cardless-1000-aggregate` および `mass-smoke-shard-0..3`。GitHub Actionsの保持期間後はArtifactの取得ができなくなるため、ここに再現条件を残す。
- **非検証項目**：全試合の個別再演、首位率差/95%CI、4人カードあり、ドラフト、公式ルール整合性。

## 2. カードなし4人戦・座席交代によるAIの比較 2,000試合

- GitHub Actions Run: [#42106593936](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42106593936)（全4シャード・集約成功）
- 検証branch: `verify/paired-eval-4p-1000-20261106`; 実行SHA `6933266dc06f008fc45f8865e895c026efaab91d`
- 実装PR：[#124](https://github.com/cueikusuta/agricola-re-ai/pull/124)、mainへのsquash SHA `dc2d37a94c0636132443211998515e20f6cd4210`
- 対象：カードなし4人、追加タイルOFF、全席ローテーション、seed 600000〜600249 の**250独立seedブロック**。
- 同一seed×席で candidate `eval` と baseline `greedy` を比較。他の3席は `greedy` 固定。各arm 1,000試合、合計 **2,000試合**。
- **全行に対して**ゲーム完走・`Game::replay` 後の状態ハッシュ一致を検証。集約器がseed/block/arm/seatの重複・欠落、得点/順位と首位クレジット・再演確認を検査する。
- 主指標は同率首位の`1/k`クレジットによる首位率。独立した250seedブロック単位の対応差を、固定seed `20261106` の**10,000回percentile bootstrap**で95%区間推定。

| 指標 | candidate: eval | baseline: greedy |
|---|---:|---:|
| 首位クレジット率 | **0.749（74.9%）** | **0.250（25.0%）** |
| 平均順位 | 1.344 | 2.499 |
| 対戦相手平均との差 | +10.7247点 | 約0点 |

- **首位クレジット率差：+0.499（+49.9ポイント）**
- **対応差の95%CI：[+0.4715, +0.5265]**（+47.15〜+52.65ポイント）
- +5ポイント差、80%検出力を想定した観測分散からの事後必要数目安：151seedブロック（実際250実施）。この値は今後の別モデル・モードの必要試合数を保証しない。
- 観測した最大`SpacePolicy::select`時間：**0.182523ミリ秒**。アクション詳細入力や収穫を含む1手全体の10秒上限ではない。
- Artifact：4シャードの`shard-0..3`試合別JSONLと `paired-eval-4p-1000-report`。データはGitHub Actionsの短期保存対象。

### 結論の解釈

この結果は、**4人カードなし・固定greedy相手という限定条件で、`eval` の方が `greedy` より良い**という実測を裏付ける。一方、`greedy` は長期開発計画で比較対象にしている**M7カード対応MCTSではない**。したがって「既存M7比+5pp改善」「実戦用強AI完成」とは判定しない。

本試験で**未検証・未達**の事項：

1. 公式リバイズド2016の期待結果・ルール正確性ゲート（#108）。
2. `SimpleDetails`外の合法な細部行動API（#109）。
3. 人間対戦で**1手全体を10秒以内に必ず打ち切り、合法手へフォールバック**する機構。
4. M7観測ベースMCTSとの同時間予算での比較と統計検出力（#106/#110）。
5. カードあり4人・7-7/10-7ドラフト・2人/3人/追加タイルONのモード別実測。

継続する正式工程：[M8-3 #110](https://github.com/cueikusuta/agricola-re-ai/issues/110)、[M8-3A #123（今回の小タスク完了）](https://github.com/cueikusuta/agricola-re-ai/issues/123)。追加実験はデータの再利用範囲・ルール/デッキ版を先に固定し、未実施の結果を記載しない。

## 4. M13評価レポートの診断項目追加（2026-11-07、#106）

前節までの集約器は首位クレジット平均・95%区間・期待順位・得点差・思考時間を出していたが、`docs/roadmap-m7-onward.md` 3.1 が求める**参考値**（同率首位を含む首位到達率、純粋な単独1位率）と 6.6 が求める**試合/秒・意思決定/秒**を出力していなかった。これを追加した。

| 追加フィールド | 定義 |
|---|---|
| `candidate_solo_first_rate` / `baseline_solo_first_rate` | 対象席が**単独1位**を取った試合の割合（同率首位は数えない） |
| `candidate_first_reach_rate` / `baseline_first_reach_rate` | `rank == 1` の試合の割合。同率首位を含む到達率 |
| `candidate_tied_first_game_rate` / `baseline_tied_first_game_rate` | 1位が複数いた試合の割合 |
| `candidate_first_credit_by_seat` / `baseline_first_credit_by_seat` | 席ごとの首位クレジット平均。座席ローテーションの偏りを検出する |
| `candidate_seat_credit_spread` / `baseline_seat_credit_spread` | 上記の最大値−最小値。集約値が隠す席依存を可視化する |
| `total_cpu_seconds` | 全試合の `total_wall_ms` 合計。1プロセスの連続実行時間 |
| `games_per_second` / `decisions_per_second` | `total_cpu_seconds` を分母にしたスループット |

- すべて**検証済みフィールドからの導出**であり、対戦を追加実行しない。首位クレジットの `1/k` 定義・順位の整合検査は既存のまま。
- `total_wall_ms <= 0` は測定値ではなくプレースホルダなので**拒否**する（スループットの0除算も防ぐ）。
- スループットは1プロセスの逐次実行を前提とした**ベンチマークの目安**で、並列実行の速度向上や人間対戦時のレイテンシを意味しない。`throughput_note` に同じ注意を機械可読で残す。

### ローカル小規模スモーク（強さの主張ではない）

- 条件：カードなし2人戦、candidate `eval` / baseline `greedy`、相手席は `greedy`。seed 901〜902の2ブロック、座席ローテーション込み8試合、release ビルド、ローカルWindows。
- 観測値：`total_cpu_seconds ≈ 0.076`、`games_per_second ≈ 105.7`、`decisions_per_second ≈ 7362`、`candidate_solo_first_rate = 1.0`、`candidate_first_credit_by_seat = [1.0, 1.0]`（席偏りなし）。
- 8試合は**統計的判断に使えない**。首位率の差・+5pp判定はこの結果から主張しない。正式評価は `Paired AI Evaluation` ワークフローの手動実行で行う。

# M12〜M14 AI強化の独立実験計画（Issue #106）

更新: 2026-11-06。**計画・事前判定規則のみ。強さの実測結果、1000試合完了、学習済みモデルは存在しない。**
依存: #108（M8-1 正確性）、#109（M8-2 合法性API）、#110（M8-3 Actions基盤）、#105/#115（4人適法カード）、#104（ドラフト）。

## 1. 比較方針・既存M7の扱い

- 目的：同じルール/相手/seed/座席/思考時間のもと、**首位率（同率1位なら1/kクレジット）をM7既存AIから絶対値+5ポイント**改善。相対+5%ではない。
- 主要指標：首位クレジットの平均と差。補助指標の順序：期待順位（低い方が良い）→平均/分布得点差→計算費用（CPU秒・意思決定秒）。単独首位率・同着率・平均得点・不正手/タイムアウトも記録。
- 人間との1手：**ワーカー/カード/詳細パラメータ、ドラフト1ピックを含め10秒以内**。測定は `release` 実行、固定基準CPU、p50/p95/p99/最大とタイムアウト/合法フォールバック率。オフライン学習時間はこの10秒に含めない。
- M7の `src/mcts.rs`、`src/evaluation.rs`、`src/benchmark.rs` は比較対象として**実装SHAと設定を固定**し、歴史的な実装を勝手に置き換えない。従来の再抽選型MCTSと厳密なISMCTSの違いを明示。
- 最初の対照群：既存M7の評価関数/評価関数＋詳細決定/再抽選型MCTS（既存実装の機能範囲に限る）。新候補：評価関数改善、純MCTS、ISMCTS、マクロアクションMCTS、軽量方策/価値モデル、探索＋モデル、ExIt型教師反復。モデル単独と探索単独は必ず残す。

## 2. 実験条件の固定

`ExperimentManifest` に `experiment_id, code_sha, engine_sha, ruleset, player_count, additional_tile, deck_hash, draft_mode, opponents, policy_id+weights, details_policy, compute_budget, seed_blocks, seat_rotation, data_split_version, model_hash, replay_format, evaluation_method_version` を記録。秘密のカード/seedをAI入力には含めない。各試合を監査できるJSONL/CSVは #110 に定義。

### データ分割
- **train**：自己対戦生成/評価関数調整/探索の事前探索。
- **validation**：ハイパーパラメータ・報酬の重み・候補選抜。事前宣言した有限の比較回数と探索空間を記録する。
- **test**：未知seed・未知デッキ組合せ（可能なモードのみ）を**最終判定専用**にロック。学習・調整・教師ラベル作成に使用禁止。train/testはseed値の数値範囲だけでなく、生成器バージョンとデッキ版でも区別し、同一初期状態の漏れを検査。
- 評価対象が2人/4人、カードなし/あり、7-7/10-7の場合は**モード別に別試験**。M10/M11未完成時にカードあり4人/ドラフトの成績を報告しない。

### 試合数と統計
1. 初期バッチ **1000試合/AI/条件**。4人なら seed 250ブロック×4席、既存M7と新AIの**同じblock×seat**を揃え、4人の残り3席AIは同じ版に固定。新旧比較で計2000試合。2人なら500ブロック×2席/arm。
2. 各 `seed` をブロックとして `delta_block = mean_seats(candidate first_credit - baseline first_credit)`。95%差区間は固定bootstrap seed・例10,000反復の**seedブロック・ブートストラップ**。独立試合の二項分散だけで多座席相関を無視しない。
3. +5ポイントの**点推定**と、差の95%区間**下限>0**が暫定採用条件。下限も+5ポイント以上という強い主張と区別。区間が0を跨げば保留、負方向なら不採用。期待順位・得点差が優位でも首位率未達を達成済みにしない。
4. 80%検出力・両側α=.05の独立二群近似で `p0=.25, p1=.30` は各arm約1250試合、`p0=.50, p1=.55` は各arm約1560試合が参考。実際の **必要seedブロック数** は対応差の観測標準偏差 `s_d` から `ceil(((1.96+0.84)*s_d/0.05)^2)` を**最初の1,000試合後**再推定。相関・複数候補探索・同着クレジットで大幅に変化する。
5. 途中解析の予定（例：1000/arm→事前上限を決めた一回の追試→最終判定）と最大追加ブロック数は本番実行前にmanifestへ固定。testを繰り返し見ながら重み調整したらそのtestは探索用に降格し、新しい未見seedで最終判定する。
6. **複数比較**：M13で何種類も試した最大値を無補正で採用しない。validationで一次候補を1つ選び、locked testでM7と**事前登録した一次比較**のみを主要検定とする。追加候補の探索的比較は探索的と表示。

## 3. M12：評価/自己対戦データを再現可能にする

**目的**：正しいエンジンに基づくデータ生成器・比較実験のバージョン管理・速度/ボトルネック測定。

- インターフェース（具体名は #109との整合後）：`reset(config)`, `observe(player)`, `legal_action_types(player)`, `validate_detail(action)`, `step(validated_action)`, `clone/restore`, `replay(log)`。**SimpleDetailsが選ばない合法詳細手も受理**し、探索の候補集合と合法手集合を混同しない。
- 高速Rust releaseバッチはサーバの内部完全状態を持ち、各AIへ **Observation（公開情報+自分だけの手札/観測履歴）**だけ渡す。MCTSが隠れ情報を再標本化するとき、真の内部カード/未来ラウンド/seedを参照しない。
- 特徴ベクトルのスキーマを固定：ラウンド、手持ち資源、農場/家族/部屋、公開スペース、観測済みカード、ドラフトで自分が見た履歴、合法候補のマスク等。**他人の非公開手札を含めない**。可変長のカードプールはID埋込＋マスク/集合統計に分離し、カード未検証なら特徴から除外。
- 速度：games/sec、decisions/sec、clone/sec、eval/sec、p50/p95/p99/max latency、メモリ使用量、CPUモデルとコンパイラ版。低速化要因（コピー/合法手/探索/カード効果）のプロファイルを記録。
- データ：`state_observation_version`、`legal_action_mask`、行動・教師分布、勝敗クレジット・得点差・順位、seed block、policy version、公開カード版。再現ログは破損/欠落の検知が必要。
- **M12受入**：#108の検証済み対象モードについて1000試合/armで14R完走・再演ハッシュ一致・不正手0・非公開情報の非干渉、比較JSON/CSV/95%区間/必要数再計算/性能測定がGitHub Actions run artifactとして残る。合格しないモードは対象外と明記。

## 4. M13：探索方式と合法候補生成の同条件比較

| 実験ID | 手法 | 主な変更箇所 | 対照/アブレーション |
|---|---|---|---|
| E0 | M7既存AI | 既存評価関数 + 既存再抽選MCTS | 固定参照 |
| E1 | 評価関数のみ | 食料安全、家族の早期増員、残手番、カード長期価値、将来の行動数 | 既存Eval、M7 |
| E2 | MCTS | UCT/PUCT、プレイアウト深さ、rollout policy | E0と同じ時間 |
| E3 | ISMCTS | 公開観測と自分の記憶に**整合する情報集合**の再標本化、情報集合ノードの統合 | E2と同じ時間、再標本化回数 |
| E4 | マクロアクションMCTS | 柵/耕作/部屋/家畜/職業の合法な詳細候補を少数列挙、ロールアウト内で組合せ | E2/E3、マクロなし |
| E5 | 軽量方策/価値 | 合法候補の順位付け + 局面価値、CPU推論優先（線形/小MLPから） | モデル単独、MCTS単独 |
| E6 | 方策誘導探索 | E5のprior/valueをE2/E3/E4へ投入 | 同時間の探索単独・モデル単独 |

- `N simulations` 固定ではなく、**最大10秒/意思決定の同一時間予算**で比較。副実験として同一CPU秒/試合・同一シミュレーション回数を測る。CPU/スレッド数固定、warm-up・キャッシュ・順序効果を記録する。
- E3での「真の内部状態を一度読み出して再抽選」は禁止。観測整合サンプラーの粒度、相手行動モデル、カード履歴、ラウンド非公開をテスト。**strategy fusion** や **non-locality** の制約を検証し、PIMC/M7再抽選と区別。
- E4は`legal_detail_validator`でマクロを実際に検証。候補にない人間の合法手もエンジンは受け付ける。探索候補を増やしすぎるほど1手上限を越えるため候補生成CPU秒も計測。
- E5はモデルにカード原文や秘密真値を入力させず、学習サンプルにも見えない情報を含めない。推論だけで10秒を使い切るモデルは不採用。
- **M13受入**：候補一覧・パラメータ探索を登録し、validationで一次候補を事前選抜。未知testでM7との首位率差/95%区間、期待順位、得点差、CPU秒と人間対戦最大10秒を報告。+5pp未達は未達と表示し、モデル学習へ無条件に進まない。

## 5. 報酬は最終目的とは別の実験因子

| 報酬ID | 学習・探索の報酬定義（候補） | 留意点 |
|---|---|---|
| R-A | 首位報酬：`1/k` if shared first else 0 | 最終目標と一致。学習初期は疎 |
| R-B | 得点差：`self_final_score - mean(opponents_final_scores)` | 連続で学習しやすいが「大差の2位」を選び得る |
| R-C | 複合：`w1*R-A + w2*normalized_rank + w3*clipped_score_margin` | 重み・正規化・clipを**train/validationだけ**で調整 |

- 同じ教師探索、学習サンプル数、モデル容量、学習/CPU予算で R-A/B/C を比較。終局の首位率で最終評価し、損失/得点差の改善だけで勝利宣言しない。
- ランクの符号・引分・タイブレーク、4人の `1/k` はコードテストで固定。中間報酬を追加するなら公式ルールの結果を変えない形で別IDにする。

## 6. M14：ExIt等の小規模学習（4ゲート）

ExIt候補：教師として情報集合を守るE2/E3/E4の探索を用い、合法アクションごとの訪問回数分布 `π_teacher` と公開観測 `obs` を収集。学生モデルは `πθ(a|obs)` と `Vθ(obs)` を推定（policy CE/KL・価値Huberなど）、探索のprior/leaf evaluationに再投入。教師/学生が同じseedや秘密を共有しない。PPO等は**G3以降の候補**であり最初から大規模GPU学習しない。

### G0：実験基盤成立（満たすまで学習開始禁止）

> 判定の実装: `src/dataset_split.rs` の `evaluate_g0` と `dataset_split g0`。データ側の検査だけでは `pass` にならず、evidence JSON（`games_per_arm >= 1000`・異常0・再演一致・不正手0・観測非干渉・CI再現・Artifact追跡）が揃って初めて `pass` を返す。未提出は `hold`、矛盾は `fail`。詳細と証拠スキーマは `docs/m12-dataset-split-and-g0.md`。
- **合格**：検証済みモードごとに14R完走1000試合/arm、異常0、保存/再演ハッシュ一致、合法操作チェック0違反、AI観測の非干渉テスト合格、版固定JSONL/CSV・座席ブロック95%区間が再現。train/test seed漏洩0、Artifactが追跡可能。
- **不合格**：実装待ち・テスト欠落・未検証カードを本番に混ぜた・再演不一致・必要統計なし。M8-1とM8-2の合格を前提。
### G1：小規模教師信号として有効
- **試験**：train上の探索ラベル作成、validation上で合法アクションマスク一致と教師/学生の一致度、政策負対数尤度、価値誤差を固定seedで再現。ランダム/一様priorと同容量モデルの対照を置く。
- **暫定合格**：0不正ラベル/非公開漏洩、教師ラベル生成100%再演、学生がvalidationの合法行動選択の負対数尤度を一様prior対照より5%以上改善し、価値MAEを定数予測対照より改善。失敗したら教師・特徴・ラベルの誤りを修正しG1再試験。**これだけで首位率の改善を主張しない。**
### G2：実戦価値が確認できる
- **試験**：未知testで学生単独、教師探索単独、学生誘導探索を、M7ベースラインと同時間・同seed×全席で比較。R-A/B/Cもvalidationで一次報酬を選択。相手は固定M7だけでなく、異種AIや過去版も**探索的**に評価。
- **暫定合格**：一次候補の首位率がM7比 **点推定+5pp以上かつ差95%CI下限>0**、全手実測10秒以内・合法フォールバック成功、未見seed上の再現性。初期1000/armでCIが0を跨ぐ場合は**保留**、事前登録した追加試合で再判定。モデル単独/探索単独/組合せの各効果を区別。
- **不合格**：首位率未達、ルール破綻、情報漏洩、10秒超過。代理報酬の改善では代替不可。大規模学習へ進まない。
### G3：拡張の判断
- **試験**：G2で採用した方式を、2→4人、カードなし→公式照合済みカードあり、通常配布→7-7→10-7へ**各々別の条件で**展開。相手混合リーグ、過去AI、カード/座席偏り、得点差と計算コストを追跡。すべて同じ未見seedブロック・席順交代の方式を維持。
- **合格（拡張判断）**：各新規対象モードの正確性・秘匿・再演ゲート合格、モード別初期1000/arm・差の95%区間と追加数計画を報告、10秒上限のフォールバック成立、M7比の首位率+5pp目標の達否を**モードごと**に明示。G2条件の強さに統計的に確かな退行を発見した場合は展開停止。全モードでの+5ppを**未達なのに達成と扱わない**。
- **不合格/保留**：一部モード未整備・未計測ならそのモードの拡張を留保。ExItを強行せずE3/E4・データ・教師・モデルに戻す。PPOの導入はG2/G3で追加複雑性に見合う改善が出た場合のみ別Issueで検討。

## 7. 具体的な実装分割と優先順位

- **M12-Eval**：#110基盤を再利用し、実験manifest/データsplit管理/結果統計とベンチマークの再現性を追加（新規独立Issue）。分割Manifestと G0 判定は `docs/m12-dataset-split-and-g0.md` で実装（#167 C）。
- **M13-Search**：E1〜E4の探索+マクロ合法候補・観測整合サンプラーと同時間比較（アルゴリズム単位にPRをさらに分割）。
- **M14-Learn**：G0→G1教師→G2小規模学生/ExIt→G3拡張。**ゲートを超えての一括実装を禁じる**。モデル版・学習ログ・学習/検証の分割は必須。
- #104/#105が未完ならカードあり/ドラフトの学習実験は未着手のまま保留。規模拡大ではなく正確な観測・再演と候補生成の改善を先にする。

## 8. 自己レビュー対象

- 毎関数：合法候補/情報集合境界、決定性、乱数使用、実行予算。
- 毎処理単位：同一seed×席の比較、全4席ブロック、評価データの不正混入、報酬の定義一致。
- 毎コミット：fmt/clippy/test、seed固定回帰、差分レビュー、ブラウザ動作との境界、CIと専用Actions run URL。未測定の結果は書かない。

**進捗更新（2026-11-09）**：G0評価基盤だけは [GitHub Actions Run #42511558662](https://github.com/cueikusuta/agricola-re-ai/actions/runs/42511558662) で正式に`pass`を確認した。G1〜G3、カードあり4人戦、AIの首位率+5pp改善、可変思考予算の厳密な10秒実測は未達のまま。G0の`pass`をAI強化達成と誤解しない。

## 9. 実装状況：M12データ層（2026-11-07、Task C / #106）

- `src/episode_data.rs` が独立した変換・検証機構。`EpisodeRecord` は1試合分の観測・行動・報酬・終局結果・seed・ルールモード・AI版・`code_sha`・エンジン版・カード版を保持し、`schema_version = 1` で版管理する。
- JSONL の書き出し・読み込み（`to_jsonl` / `from_jsonl`）と、件数・スキーマ版・人数・重複seed・再演破損を検出する `inspect_dataset` / `DatasetReport`。
- `verify()` はコミット済みイベント列を `Game::replay_with_cards` で再演し、最終ハッシュと順位表の一致を確認する。
- `training_samples(viewer)` は行為者が `viewer` のターンだけを返し、`TrainingSample` には seed もイベント列も含めない。`assert_public()` で秘匿フィールドの混入を拒否する。ドラフト履歴は `DraftEpisode.views[seat]`（各席が正当に観測した `DraftObservation`）からのみ渡す。
- ローカル小規模生成は `src/bin/episode_export.rs`。大量試合は既存のGitHub Actions専用ワークフロー（`docs/github-actions-mass-simulation-m8-3.md`）で実行し、通常CIには組み込まない。
- **エピソードAPIとの統合**（2026-11-07）: `record_self_play` は `record_episode` / `record_from_episode` を通じて `src/api.rs` の `Episode` を駆動する。コーディネータが `Episode` を保持し、方策には `PolicyInput`（観測＋候補手）だけを渡し、各行動はエンジンの `validate_action`/`step` を通す。`Episode::game()` は信頼されたコーディネータ用の記録アクセサ（`events()`/`config()` と同じ境界）。API経由の記録が直接Gameドライバと完全一致することは `tests/m12_episode_api_bridge.rs` が固定する。実測の強さ・学習結果は未評価。
- 本統合はマージ済みの PR #148 と PR #158 の再演検証強化を基盤とする。

## 10. 実装状況：M13評価条件の分離（カード条件、2026-11-07 / #106）

- 3節「モード別に別試験」のうち、**カードなし／実装済みカードあり**の分離を評価基盤へ実装した。`src/bin/paired_eval.rs --cards none|verified` と `scripts/paired_eval_stats.py --cards none|verified_basic_2p` が対になり、片方だけの条件混在を拒否する。詳細は `docs/github-actions-mass-simulation-m8-3.md` 10節。
- `verified` が使えるのは2人戦のみ（`verified_basic_card_set` の制約）。4人戦カードありは M10 の G1 未達のため、**評価条件として未提供**と明記する。
- デッキ同一性は `CardSet::content_hash()`（`id`/`kind`/`cost`/`points`/`effects` の秩序非依存ハッシュ）で検証する。seed・配布手札は含めないため、非公開情報の漏洩経路にならない。
- ドラフト条件（`draft_7_7` / `draft_10_7_simultaneous` / `choose_10_7`）は未提供のまま。#155 の慣行決定と M10 のプール整備が前提。
- **首位率の比較結果はまだ無い。** 本項は条件分離の実装報告であり、+5pp の達成・未達を判定したものではない。


## 11. #212：4人カードなし PUCT 対 MCTS の直接比較（事前登録）

**目的**：M13の`PUCT`と`MCTS`を同じシード・4席ローテーションで直接比較する。カード実装の完了を待たない。今回のPRは比較プロトコルと小規模CI実走まで。結果の大小で事前登録条件を後から変更しない。

### 小規模pilot（PRで実走・採用判断禁止）

- `players=4`、`cards=none`、追加タイルOFF、`candidate=puct`、`baseline=mcts`、`opponent=mcts`。PUCTが1席、残り3席がMCTSの直接対戦。baseline armは全席MCTSで**mirror control**。一般的な「1対1」ではない。
- `seed_start=64000`、4seedブロック×4席×2arm＝**計32試合**。試験の統計は見ても強さの結論を出さない。
- `search_mode=simulation-capped`、`decision_budget_ms=0`、MCTS/PUCT各4シミュレーション・depth1、PUCT leaf=terminal。**同シミュレーション数であって同CPU時間ではない**。再現性を確認する配管テスト。
- `paired_eval`自身が全14ラウンド・イベント再演ハッシュを検証し、`paired_eval_stats.py`が各block×arm×seatの件数・モード・seed・同一性、首位クレジットと95%ブロックbootstrapを検証する。Artifactに行データと集約JSONを保存。

### 初期正式比較（別の手動/エージェント明示起動・CIと分離）

- 事前登録した**1つの一次比較**：`players=4`、`cards=none`、追加タイルOFF、`candidate=puct`、`baseline=mcts`、`opponent=mcts`、同一シード席順交代（MCTS mirror対照）。`seed_start=965000`、`blocks=250`、4席＝**1,000試合/arm、合計2,000試合**。pilot seed 64000台/G0 seed 920000台とは分ける。
- 正式比較は各16シミュレーション・depth2、PUCT leaf terminal（ほかのパラメータはworkflow既定）、`simulation-capped`、`decision_budget_ms=0`。これで探索方式の違いを見る。**同時間・最大10秒の判定は#167の別実験**とし、混ぜない。
- 実行用コマンド（権限のあるエージェントからGitHub標準CPU上で明示起動）：

```sh
gh workflow run paired-eval.yml --ref main \
  -f players=4 -f blocks=250 -f seed_start=965000 \
  -f candidate=puct -f baseline=mcts -f opponent=mcts \
  -f cards=none -f extra=false \
  -f search_mode=simulation-capped -f decision_budget_ms=0 \
  -f mcts_simulations=16 -f mcts_tree_depth=2 \
  -f puct_simulations=16 -f puct_tree_depth=2 \
  -f puct_leaf=terminal
```

- 必須成果：Run URL、実行SHA、入力パラメータ、`paired-eval-report` Artifact、`comparison_design.head_to_head=true`と`mirror_baseline=true`、1/k首位クレジット差・同着・期待順位・得点差・全4席内訳・95%CI・必要ブロック数・CPU費用・ルール/再演失敗0。
- PUCTの差が**点推定+5pp以上、かつ95%CI下限>0**ならこの限定条件で暫定採用候補。区間が0を含めば保留、負方向なら不採用。pilotの数字を正式実験の意思決定に用いない。正式test結果に合わせて参数を再選択した場合は**新しい未知seed**で別の一次評価を行う。
- この条件は4人戦カードなし・固定シミュレーション数であり、BGA対戦時間設定、ドラフト、カードあり4人戦、G1/G2、10秒上限、全プレイ条件での+5pp達成を意味しない。


## 12. #214：AIエージェントによる正式直接対戦の明示起動

`.github/workflows/m13-direct-puct-mcts.yml`は§11で事前登録した正式比較**だけ**を行う。通常PRやmain pushで2,000試合を自動実行しない。

- 本人のGitHub上のIssue #106に、**本文全体が次の1行だけ**のコメントを投稿すると起動する：`/run-m13-4p-puct-vs-mcts-20261109`。GitHub Actions側で`issue_comment.created`、`github.ref=refs/heads/main`、`github.repository`、Issue番号106、GitHub投稿者/actor `cueikusuta` をすべて検査する。別人のコメント/PR/無関係なIssueはジョブを開始できない。
- GitHub CLIでの明示起動も可能：`gh workflow run m13-direct-puct-mcts.yml --ref main`。両経路ともユーザー入力はシェルに埋め込まず、**固定**の`candidate=puct, baseline=mcts, opponent=mcts`、4人カードなし、250ブロック×4席×2arm、seed 965000、各16 simulations/depth2を実行する。
- 8シャードをGitHub-hosted `ubuntu-24.04`で分散し、全実行runとコミットSHAのmeta情報・シャード固有block範囲を確認。Rust側で全試合再演ハッシュを照合し、`scripts/paired_eval_stats.py`が2,000行・全seat/seedの完全性・95%区間/期待順位・score margin・速度を検証する。失敗シャードがあれば集約はFAILし、不完全結果を強さの証拠にしない。
- PR #213での**32試合pilot**はGitHub-hostedのスモークとしてSUCCESS、PUCT 93.75%・MCTS mirror25.00%、しかし独立seedが4ブロックしかなく4 simulations/depth1の探索的観測である。これを正式な改善主張には用いず、今回の250ブロックの結果を別に評価する。
- 本正式比較でも両探索の**シミュレーション回数を一致**させたのであり、同時間の10秒条件にはなっていない。時間上限の可変化と対局時計は#167/#92、カード・ドラフト追加は#105/#104で別に扱う。

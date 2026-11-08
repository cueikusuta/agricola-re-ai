# M9 ブラウザ14ラウンド受入試験（Issue #92）

更新: 2026-11-07（2026-11-08 に保存・復元の節を追記）。対象: `tests/e2e/full-game.cjs`、`tests/e2e/save-restore.test.cjs`、`.github/workflows/web-full-game.yml`。

**本ドキュメントは受入試験の契約と、ローカル実行が環境要因で成立しなかった事実の記録。14ラウンド完走の実ブラウザ確認は CI（`Browser UI Full Game`）で実施する。**

## 1. 位置付け

`docs/roadmap-m7-onward.md` M9 の受入条件は「Playwright で2人戦の14ラウンド完走を必須とし、4人戦もサポート対象モードで実行。フリーアクション、不正入力、保存復帰、相手手札の秘匿を確認」。通常CIの軽量スモーク（`tests/e2e/smoke.test.cjs`）とは別ジョブで、`web-full-game.yml` が `tests/e2e/full-game.cjs` の変更時に自動実行される。

## 2. 既存からの拡張点

| 項目 | 拡張前 | 本PR後 |
|---|---|---|
| 14ラウンド完走 | 3シナリオ（2人カードあり／2人カードあり＋追加タイル／4人カードなし） | 同左を維持 |
| 保存復帰 | ラウンド7で `page.reload()` し、手札のみ比較 | **リロード前後の可視状態（手番・サマリ・資源・農場15マス・自分の手札）を一式比較**。差異があれば失敗 |
| 保存・復元（サーバ再起動） | 未実装 | **`/api/save` と `/api/load` を追加**し、`tests/e2e/save-restore.test.cjs` がブラウザ保存 → プロセス終了 → 別プロセス起動 → 読込 → 可視状態一致 → 残りラウンド完走を検証 |
| API代替の禁止 | 「APIを直接叩かない」運用ルールのみ | **機械的検査**：`POST /api/play|/api/free|/api/harvest` の実リクエスト数と、`#confirm` を押した回数を突合し、不一致なら失敗（隠れた `fetch()` での着手を検出） |
| 失敗時の証跡 | スクリーンショット＋スタックのテキスト | **`artifacts/full-game-seed-<seed>-failure.json`**：seed・人数・カード条件・追加タイル・エラー・cause・画面状態（手番/サマリ/資源/通知/エディタ）・ブラウザ例外・コンソールエラー・UI操作ジャーナル（直近25件）・送信回数・再現コマンド・スクリーンショットパス・時刻 |
| 成功時の証跡 | 標準出力の coverage JSON | **`artifacts/full-game-seed-<seed>-coverage.json`**（同内容＋coverage）を保存 |
| 受入項目の明示断言 | 2人カードありでのみ手動操作6種を断言 | 全モードで「畑をUIで耕した」「フリーアクションをUIで送信」「不正値が状態を進めず拒否された」「給餌が実ブラウザで発生」「セッション復帰で可視状態が一致」を断言 |

### 2.1 「保存復帰」と「保存・復元」の区別

2つは別の失敗モードであり、別の試験で検証する。

| 経路 | 何を保証するか | 試験 |
|---|---|---|
| **リロード復帰** | ブラウザをリロードしても、動作中のサーバセッションへ再接続し同じ手番・資源・農場・自分の手札が表示される | `tests/e2e/full-game.cjs`（ラウンド7）。成果物に `reloadOnlyResume: true` と `saveRestoreCoveredBy` を記録 |
| **保存・復元** | ブラウザで保存したファイルから、**サーバプロセスを終了して別プロセスを起動した後**に同じ状態へ復元でき、残りラウンドを完走できる | `tests/e2e/save-restore.test.cjs` |

保存・復元の実装は `src/web.rs` の `Session::export_save` / `Session::import_save` と `POST /api/save` / `POST /api/load`。

- 保存形式は `format: "agricola-web-save"` / `schema_version: 1` の封筒で、中身はエンジン自身の `Game` 保存（`Game::save_json` と同じ形）。復元は `Game::load_json` を通すので、**ルール/エンジンの版・config・初期状態スナップショット・イベント列のハッシュ連鎖**がすべて再検証される。改ざん・破損・版不一致は読み込み時に拒否され、実行中の対戦は変化しない（トランザクション）。
- **収穫の給餌・繁殖を選択している間、およびドラフト由来の対戦（ドラフト完了後も含む）**は保存を拒否する。M11の観測履歴・選択記録・rulesetは現行保存封筒に含まれず、保存後に復元すると失われるため、履歴を認証付きで保存復元できるまでfail closedとする（#184 P1）。通常対戦の保存には影響しない。
- `POST /api/load` の受信上限は8MiB、その他のPOSTは従来どおり64KiB。上限超過は413で拒否し、メモリを無制限に確保しない。4人戦・終盤のセーブ容量と回帰テストを設ける（#184 P2）。
- **64bit seed の扱い**：`round_seed` / `action_seed` は 2^53 を超えるため、ブラウザの `JSON.parse`/`JSON.stringify` を通すと丸められる。`/api/save` は保存内容を**文字列**で返し、`/api/load` はアップロードされたファイルを**そのまま**送る。この契約は `a_save_posted_verbatim_keeps_the_64_bit_seeds` で固定している。
- **非公開情報**：保存ファイルには相手の手札・未公開ラウンド順・内部乱数が含まれる（本人のローカルファイルであるため）。`/api/state` は従来どおり公開情報と自分の手札だけを返し、保存は明示操作でのみ行う。UIにも共有しない旨を表示する。

### 2.2 API代替を行わないことの検査

```js
assert.deepEqual(
  traffic.submits,                                       // #confirm を押した回数
  traffic.api.play + traffic.api.free + traffic.api.harvest,
  "every game-mutating request came from a confirmed UI dialog"
);
```

状態を進める操作は必ず描画されたダイアログの確定ボタンから行う。テスト内で `fetch()` や `page.evaluate` による直接POSTを追加すれば、この等式が崩れて失敗する。`page.waitForResponse` は**結果の観測のみ**に使い、送信には使わない。

## 3. 実行方法

CI（推奨）:

```sh
gh workflow run web-full-game.yml
```

ローカル:

```sh
cargo build --locked
AGRICOLA_WEB_PORT=8791 ./target/debug/agricola-re-ai --web &   # ポートは任意（PR #149 と競合させない）
AGRICOLA_WEB_URL=http://127.0.0.1:8791/ node --test tests/e2e/full-game.cjs
```

- 既定URLは `http://127.0.0.1:8787/`。`AGRICOLA_WEB_URL` で上書きでき、サーバ起動・ポートの扱いは**本PRで変更していない**（PR #149 の起動/ポート変更と競合しない）。
- ページ読込・操作はすべて実際の Chromium（`channel: "chrome"`）で行う。

## 4. ローカル実行の結果（2026-11-07）

**完走確認は取れていない。** この作業環境では次の理由でローカル実行が成立しなかった。

1. 既定ポート `8787` は既存の別プロセスが使用しており、本ブランチのビルドは `os error 10048` でバインドに失敗した（`web.log`）。その既存プロセスは他セッションのものと考えられ、停止していない。
2. 別ポート（8790）でサーバを起動する操作は、この環境の権限で**拒否**された（「persistent local web server」）。拒否を回避せず、ローカル完走は未実施のままとした。

代わりに確認したこと:

- `node --check tests/e2e/full-game.cjs`：構文OK。
- `node --test tests/e2e/full-game.cjs` を既存の 8787 サーバに対して実行し、**失敗時の証跡が正しく生成されること**を確認。`artifacts/full-game-seed-42-failure.json` に seed・人数・カード条件・画面状態・エラー・再現コマンドが記録され、PNG も保存された（既存サーバは本ブランチより古いUIで `#turnWorkers` を持たないため、全シナリオがその地点で失敗した）。
- `artifacts/` は `.gitignore` 済みで、PR差分には含まれない。

ローカル環境では完走を確認できていない。**実ブラウザ完走の判定は CI で行う。**

## 4.1 CI での実行結果（2026-11-07、PR #171）

`Browser UI Full Game` が成功した（`browser-full-game` ジョブ、48.5 秒、pass 4 / fail 0）。

| シナリオ | ラウンド | 給餌 | 繁殖 | UI送信 | POST（play/free/harvest） |
|---|---:|---:|---:|---:|---|
| 2人・カードあり・追加タイルOFF（seed 42） | 1–14 全観測 | 6 | 6 | 48 | 35/1/12 = 48 |
| 2人・カードあり・追加タイルON（seed 43） | 1–14 全観測 | 6 | 6 | 56 | 43/1/12 = 56 |
| 4人・カードなし・追加タイルON（seed 44） | 1–14 全観測 | 6 | 6 | 42 | 29/1/12 = 42 |

- 全シナリオで `manualPlow`・`freeAction`・`invalidInputRejected`・`sessionResumeStatePreserved: true`。
- 2人カードありの2シナリオは、柵・部屋・改築・種まき・家畜・改善を各1回以上UIから実行し、職業カードも使用した。
- **UI送信回数とPOST回数が一致**（48/48、56/56、42/42）。テストがAPIを直接叩いていないことが機械的に確認された。
- `reloadOnlyResume: true` と `saveRestoreCoveredBy` を全シナリオで記録し、リロード復帰と保存・復元を混同しない。
- 成果物: Actions Artifact `browser-full-game-evidence`（`artifacts/full-game-seed-<seed>-coverage.json` と実行ログ）。

## 5. 未解決・範囲外

1. **実ブラウザ14ラウンドの完走は CI で確認済み**（4.1節）。ローカルでは環境制約（ポート使用中・サーバ起動の権限拒否）により未確認。
2. **保存・復元は実装済み**（2.1節、`tests/e2e/save-restore.test.cjs`）。残る制約は、収穫選択中とドラフト進行中は保存できないこと、保存ファイルが非公開情報を含むこと。
3. 4人戦はカードなしのみ。4人カードありは M10 のプール整備（40+40）待ちで、`web-full-game.yml` の対象外。
4. ドラフト込みの14ラウンド完走は `tests/e2e/draft.test.cjs` の範囲で、本ファイルの対象外。
5. ポート上書き（`AGRICOLA_WEB_PORT`）は既に実装済みだが、PR #149 が起動手順・既定ポートを変更する可能性があるため、本PRはワークフローと起動手順に触れていない。

## 6. 保存・復元試験の内容（`tests/e2e/save-restore.test.cjs`）

サーバプロセスを試験自身が起動・終了する（ポートは `AGRICOLA_SAVE_TEST_PORT`、既定 8799）。

1. 2人戦・カードなし・seed 42 で開始し、ラウンド7までUIで進める。
2. 「対戦を保存」を押し、ダウンロードされたファイルを検証（`format` と `schema_version`、`game` の存在）。
3. サーバプロセスを終了し、ポートが閉じたことを確認する。
4. 新しいプロセスを起動する。新しいセッションはラウンド1なので、**復元されたラウンド7はファイル由来であることが示される**。
5. ファイルを読み込み、手番・サマリ・資源・農場15マス・自分の手札・履歴が保存前と一致することを比較する。
6. 破損ファイル（途中で切れたJSON）と改ざんファイル（食料を+5）を読み込み、どちらも拒否され、対戦が変化しないことを確認する。
7. 残りラウンドをUIで完走し、「対戦終了」と最終順位表示を確認する。

成果物は `artifacts/save-restore-coverage.json` と保存ファイル。


**残課題**：ドラフト由来ゲームの保存・復元を有効にするには、M11の観測履歴・ルールセット・選択記録を検証付きで保存し、ゲーム手札と突き合わせてから復元する実装と実ブラウザE2Eが必要。現行の拒否はデータ損失防止であり、対応完了ではない。

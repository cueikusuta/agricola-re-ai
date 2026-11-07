# M9 ブラウザ14ラウンド受入試験（Issue #92）

更新: 2026-11-07。対象: `tests/e2e/full-game.cjs`、`.github/workflows/web-full-game.yml`（変更なし）。

**本ドキュメントは受入試験の契約と、ローカル実行が環境要因で成立しなかった事実の記録。14ラウンド完走の実ブラウザ確認は CI（`Browser UI Full Game`）で実施する。**

## 1. 位置付け

`docs/roadmap-m7-onward.md` M9 の受入条件は「Playwright で2人戦の14ラウンド完走を必須とし、4人戦もサポート対象モードで実行。フリーアクション、不正入力、保存復帰、相手手札の秘匿を確認」。通常CIの軽量スモーク（`tests/e2e/smoke.test.cjs`）とは別ジョブで、`web-full-game.yml` が `tests/e2e/full-game.cjs` の変更時に自動実行される。

## 2. 既存からの拡張点

| 項目 | 拡張前 | 本PR後 |
|---|---|---|
| 14ラウンド完走 | 3シナリオ（2人カードあり／2人カードあり＋追加タイル／4人カードなし） | 同左を維持 |
| 保存復帰 | ラウンド7で `page.reload()` し、手札のみ比較 | **リロード前後の可視状態（手番・サマリ・資源・農場15マス・自分の手札）を一式比較**。差異があれば失敗 |
| 保存復帰の限界 | コメントのみ | 成果物に `serializedSaveRestoreAvailable: false` を明示記録（このサーバに永続保存/復元APIは無い） |
| API代替の禁止 | 「APIを直接叩かない」運用ルールのみ | **機械的検査**：`POST /api/play|/api/free|/api/harvest` の実リクエスト数と、`#confirm` を押した回数を突合し、不一致なら失敗（隠れた `fetch()` での着手を検出） |
| 失敗時の証跡 | スクリーンショット＋スタックのテキスト | **`artifacts/full-game-seed-<seed>-failure.json`**：seed・人数・カード条件・追加タイル・エラー・cause・画面状態（手番/サマリ/資源/通知/エディタ）・ブラウザ例外・コンソールエラー・UI操作ジャーナル（直近25件）・送信回数・再現コマンド・スクリーンショットパス・時刻 |
| 成功時の証跡 | 標準出力の coverage JSON | **`artifacts/full-game-seed-<seed>-coverage.json`**（同内容＋coverage）を保存 |
| 受入項目の明示断言 | 2人カードありでのみ手動操作6種を断言 | 全モードで「畑をUIで耕した」「フリーアクションをUIで送信」「不正値が状態を進めず拒否された」「給餌が実ブラウザで発生」「セッション復帰で可視状態が一致」を断言 |

### 2.1 何を「保存復帰」と呼んでいるか

このアプリケーションには**永続化された保存/復元APIが無い**（`src/web.rs` のルートは `/api/state`・`/api/new`・`/api/play`・`/api/free`・`/api/harvest`・`/api/draft/*` のみ）。したがって本試験が検証できるのは「**ブラウザをリロードしても、動作中のサーバセッションへ再接続し、同じ手番・資源・農場・自分の手札が表示される**」ことまでである。

- サーバ再起動を跨ぐシリアライズ保存/復元は**未実装**であり、本試験の合格に含めない（成果物に `serializedSaveRestoreAvailable: false` として残す）。
- エンジン側のログ再演（`Game::replay_with_cards` と状態ハッシュ一致）は `tests/replay_integrity.rs` 等が担当し、本試験とは別の担保である。

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

**したがって「14ラウンド完走を実ブラウザで確認した」とは主張しない。** 実ブラウザ完走の判定は `web-full-game.yml` の実行結果（Actions URL）で行う。

## 5. 未解決・範囲外

1. **実ブラウザ14ラウンドの完走はCI実行待ち。** 上記の環境制約によりローカルでは未確認。
2. **永続保存/復元は未実装。** 本試験はセッション復帰のみを検証し、サーバ再起動を跨ぐ保存復帰は対象外（成果物に明示）。
3. 4人戦はカードなしのみ。4人カードありは M10 のプール整備（40+40）待ちで、`web-full-game.yml` の対象外。
4. ドラフト込みの14ラウンド完走は `tests/e2e/draft.test.cjs` の範囲で、本ファイルの対象外。
5. ポート上書き（`AGRICOLA_WEB_PORT`）は既に実装済みだが、PR #149 が起動手順・既定ポートを変更する可能性があるため、本PRはワークフローと起動手順に触れていない。

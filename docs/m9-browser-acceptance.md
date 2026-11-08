# M9 実ブラウザ受入：4人戦14ラウンド（Issue #92）

更新: 2026-11-08。**APIテストではなく、Chromium上の描画済みコントロールだけ**で操作した結果である。
`tests/e2e/full-game.cjs` を `.github/workflows/web-full-game.yml`（手動実行・PRの当該ファイル変更時）で走らせる。
通常CIの60秒ゲートとは分離したままにする。

## 1. 対象モードと前提

| 人数 | カード | 追加タイル | 状態 |
|---|---|---|---|
| 2 | A/B実装済み（`verified_basic`） | OFF / ON | 検証済み |
| 4 | カードなし | OFF / ON | 検証済み（本docの追加分） |
| 4 | A/B実装済み | — | **未対応**。`verified_basic_card_set(4)` が「2人戦のみ」として拒否する |

4人戦でカードを使う検証は、M10の適法40+40（Issue #105/#115）が揃うまで行わない。カードプール不足のモードを
「検証済み」にしないため、E2Eは**拒否されること**を確認する（`four player verified card pool is refused`）。
拒否時は `/api/new` が非2xxを返し、`#summary`・`#turn` が変化しないことまで確認する。

## 2. 実ブラウザで実行している操作（4人戦）

すべてボタン・フォーム・`<dialog>` 経由で送信する。`fetch` を直接呼ぶ補助は使っていない。
`watchTraffic` が `POST /api/play|free|harvest` の回数を数え、UIで確定した回数と一致することを
終局時に検証するため、裏口のAPI操作があれば失敗する。

- 開始：人数・カード・追加タイル・seed・AIを画面から選択して「開始」。
- 自由操作：`#free` モーダルで初期食料1の廃棄。
- 畑：不正値 `-1` を送って拒否（ターン・履歴・状態が動かないこと）を確認した後、正しいマスで耕作。
- 柵：`#fields` の合法候補ボタンから1件選択して送信（候補は `fence_hints`、送信は通常の詳細フォーム）。
- 部屋・厩、改築、種まき・パン、家畜（羊/猪/牛）、主要改善：`detail_hints` と候補から値を入れ、実際に送信。
  主要改善は市場に残っている改善を順に試し、資源不足なら採石場・粘土坑などで実資源を集めてから再試行する。
- 収穫：`収穫の詳細を指定` → 給餌（初回は給餌順を実際に並べ替え）→ 繁殖 をそれぞれ送信。
- 表示確認：`#turnWorkers`（残りワーカー）、`#turnActions`（使用可能アクション件数）、`#summary`、`#stats`、
  各席タブの得点、`#notice` の最終順位（4席すべて）。
- 手札秘匿：1P以外の全席タブで `手札: 非公開` を確認（2人戦の2Pだけを見る旧検証を全席へ拡張）。
- 再読み込み復帰：ラウンド7で `page.reload()` し、**全席の**農場・資源・手札表示・ターン・使用可能件数・履歴が
  完全一致することを確認する。

## 3. 保存・復帰の切り分け

`src/web.rs` の現行セッションには**永続化された保存/復元APIがない**。したがってブラウザE2Eで確認できるのは
「実行中サーバーセッションへの再接続」であり、成果物には `serializedSaveRestoreAvailable: false` を記録して
保存復帰の合格とは主張しない。直列化された保存・復帰は Rust 側の
`Game::save_json` / `load_json` / `Game::replay` で検証する（`tests/m4_multiplayer.rs` の2〜4人、
`tests/m8_detail_api_audit.rs` の非既定詳細）。ブラウザ用の保存APIは独立Issueで扱う。

## 4. 成果物（GitHub Actions Artifact）

`web-full-game.yml` が `artifacts/` を保存する。1ゲームごとに次を残す。

- `full-game-seed-<seed>-coverage.json`: 種別 `browser-14-round-coverage`、到達ラウンド、実行した手動操作の回数
  （`manualFence`/`manualRoom`/`manualRenovation`/`manualSow`/`manualAnimal`/`manualImprovement`）、
  給餌・繁殖回数、`pageReloadResumed`、`serializedSaveRestoreAvailable`、最終順位、
  UI確定回数とPOST回数、`reproduce` コマンド。
- `full-game-seed-<seed>-failure.json` とスクリーンショット: 失敗時のターン・要約・資源・エラー・
  最後の25操作（`last_ui_actions`）・再現用 `--test-name-pattern`。
- 実行ログ（`full-game-run.log`）とサーバーログ。

正常系（14ラウンド完走・表示・順位）と異常系（不正入力の拒否、未対応モードの拒否）は上記のとおり別テストに分ける。

## 5. 実測（ローカル、2026-11-08）

`bash scripts/run-browser-e2e-local.sh tests/e2e/full-game.cjs`（Windows 11、Chrome headless、debug build、
専用ポート8799）。

- 6テストすべて成功、合計約42秒（2人×2・4人×2・未対応モード拒否・親テスト）。
- 4人戦2件はいずれも14ラウンド到達、最終順位が4席表示、手動詳細6種すべて1回以上送信、
  ラウンド7の再読み込みで全席一致。
- 未対応モード拒否テストは `POST /api/new` が非2xx、画面状態が不変であることを確認。

CIの所要時間・ランナー差は別途GitHub Actionsの実行結果で確認する（本docはローカル実測のみを主張する）。

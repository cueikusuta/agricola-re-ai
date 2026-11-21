# 作業の現在地

## Freebuff — D帯仕様データの文献照合（ブランチ `research/freebuff-dband-spec-check`）

- 基点: `origin/main` = `be4ee45`。成果物: `docs/dband-spec-check.md`。仕様データ・コード・テストは変更しない。
- 対象: PR #396（17枚）・#398（14枚）・#401（15枚）= **46枚**（D070–D077, D080, D082, D085–D120 のうち追加分）。
- 現在地: 全章完了。PR #402 を作成。**マージしない。** 独立レビュー待ち（`AGENTS.md` §14）。
- 判定: 一致23・表現不足3・食い違い20。費用・前提・印刷点は D080 のOR費用以外すべて一致。
- 主な指摘: D070–D077・D080 の `left_neighbor_hand`（traveling の根拠なし、9枚）、D097 の物乞いを食料に計上、
  D092 の食料2（文献に無い）、D096 の小進歩限定、D110 のしきい値、D113・D115 の回数。
- 転記ミスとして確定できるもの7件、文献間の割れ6件、未確定9件、人間への質問9件（Q-1〜Q-9）。
- 再開はこの節の「現在地」から。

## Freebuff — A帯・B帯仕様データの文献照合（ブランチ `research/freebuff-ab-spec-check`）

- 基点: `origin/main` = `be4ee45`。既照合の除外: #387（A29・B21）、#394（A/B 0）、#402（D帯46・B008は校正用例）。
  対象はA帯139枚・B帯147枚。
- 仕様JSONは `scripts/build_bga_card_specs.py` の生成物で、`tests/test_bga_card_specs_builder.py` が
  再生成一致を要求する（Actions の `bga-spec-contract` で確認）。また各カードに `data/bga-card-tests/` の契約がある。
  このため修正はビルダーの表＋再生成で行い、テスト契約が旧読みのカードは「保留」として文献の読みを残す。
- A帯: 一致86・食い違い33・表現不足11・未確定9。**修正19枚**（前提13・語彙名3・効果3）、**保留25枚**、質問Q-1〜Q-9。
  成果物 `docs/aband-spec-check.md`、ログ `result-freebuff-aband-spec-check.md`。PR #409。
- B帯: 一致50・表現不足70・食い違い17・未確定10。**データは変更しない**（食い違い17はすべてテスト契約の再設計が前提）。
  修正案・語彙不足・質問Q-1〜Q-5を `docs/bband-spec-check.md` に記録。ログ `result-freebuff-bband-spec-check.md`。PR #410。
- 検証: ビルダー再生成一致・unittest 3 OK・`validate_bga_card_specs.py` exit 0。`src/`・テスト・台帳は無変更。
- 現在地: A帯PR・B帯PRを作成して停止。**マージしない。** 独立レビュー待ち（`AGENTS.md` §14）。
- 再開はこの節の「現在地」から。

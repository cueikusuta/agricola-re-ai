# 作業の現在地

## Freebuff — E帯仕様JSONの未作成分（ブランチ `research/freebuff-eband-spec-create`）

- 基点: `origin/main` = `c4a4ba6`。作業ブランチ `research/freebuff-eband-spec-create` を基点に、章ごとに
  `research/freebuff-eband-spec-create-ch01` / `-ch02` を分ける。**PR は作成まで、マージしない。**
- E帯は台帳（`data/bga-card-base-ledger.csv`）に E001–E168 の **168枚**。

### 第0章 E帯の棚卸し（main との突合）

| 区分 | 枚数 | 内訳 |
|---|---|---|
| 台帳のE帯 | 168 | E001–E168 |
| main に仕様JSONあり | 131 | E001–E083（83）＋ E121–E168（48） |
| main に仕様JSONなし | 37 | E084–E120 |
| 他担当の作業中ブランチにあり（対象外） | 15 | E084–E098（`origin/m10/bga-spec-E084-098-c01`、commit `1c69f67`） |
| **今回の対象** | **22** | **E099–E120** |

- E084–E098 は `origin/m10/bga-spec-E084-098-c01` に仕様JSON 15枚とテスト契約15枚が存在する。
  ただし **PR が未作成**（open/closed とも 0 件、`gh pr list --state all --head` で確認）で、
  他担当が進行中の作業と判断した。重複を避けるため対象から外す（`AGENTS.md` 運用条件「他担当の進行中範囲に触れない」）。
  このブランチのPRが作られないまま残る場合、E084–E098 は別章で追加が必要。
- E070–E083 は PR #403 で main に統合済み。E121–E168 は既存分を変更しない。

### 章割り

| 章 | 範囲 | 枚数 | ブランチ |
|---|---|---|---|
| 第1章 | E099–E109 | 11 | `research/freebuff-eband-spec-create-ch01` |
| 第2章 | E110–E120 | 11 | `research/freebuff-eband-spec-create-ch02` |

### 第1章 E099–E109（完了）

- ブランチ `research/freebuff-eband-spec-create-ch01`。仕様JSON 11枚＋テスト契約11枚（33ケース）を追加。
- 内訳: `bga_checked` 9枚、`needs_resolution` 2枚（E101 ステージ境界、E109 交換回数）。テスト契約は既存バッチと同じ
  `effect_only` の3ケース／枚。
- 追加語彙は `docs/bga-card-effect-vocabulary.md` の「2026-11-21追補：E帯 099–120」。
- 記録: `docs/eband-spec-create-ch01.md`（カード一覧・質問Q-1/Q-2・検証）。
- 検証: `validate_bga_card_specs.py` exit 0（errors 空）、ビルダー unittest 3 OK。`src/`・台帳・既存仕様は未変更。

### 現在地

- 第1章の仕様・テスト・文書を作成済み。PRを作成して停止する（マージしない）。次は第2章 E110–E120。

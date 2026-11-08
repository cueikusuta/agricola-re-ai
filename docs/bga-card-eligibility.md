# カードの人数適格（Issue #202）

## 目的

BGA向けWikiにはカードごとの人数別適格が記載されていないため、`data/bga-card-base-ledger.csv`
の `eligible_2/3/4` は長らく `unknown` だった。人数条件は**カード番号規則**から確定できるので、
その規則・出典・適用範囲をここで管理する。

**これはカード効果の公式照合ではない。** 効果内容のLookout公式照合は
`docs/card-catalog-ae-audit.csv`（#115）側で別管理し、本作業では更新しない。

## 出典

Lookout Games『Consul Dirigens Rules』（2016改訂版のカード番号規則）4ページ。

- https://shop.asmodee.ca/img/product/description/EN-AGRICOLA-CONSUL_DIRIGENS-RULES.pdf

同資料は各デッキ（A〜E）の168枚を、番号帯ごとに種別と使用可能人数で定義している。

## 番号規則

| 番号帯 | 種別 | 2人 | 3人 | 4人 |
|---|---|---:|---:|---:|
| 001–084 | 小進歩 | 可 | 可 | 可 |
| 085–126 | 職業 | 可 | 可 | 可 |
| 127–147 | 職業 | 不可 | 可 | 可 |
| 148–168 | 職業 | 不可 | 不可 | 可 |

### 台帳データによる規則の裏づけ

`data/bga-card-base-ledger.csv` の `kind` は、5デッキすべてでこの番号帯と一致する
（001–084 が小進歩、085–168 が職業。例外は `multi_type` の4枚 A060 / D025 / D059 / D060 で、
いずれも 001–084 帯に属する）。番号と種別の食い違いは0件。

## 適用範囲

| デッキ | 適用 | `eligibility_basis` |
|---|---|---|
| A / B / C / D | 規則を適用 | `consul_dirigens_rule` |
| E | **適用しない**（別途確認） | `unknown` |

Eセットは同規則を無条件に拡張せず、`unknown` のまま残す。Eの適格を確定するときは、
Eセット固有の根拠資料を別途記録し、この文書に追記する。

## 台帳の列

`data/bga-card-base-ledger.csv` に次の2列を追加した。

- `eligibility_basis`: `consul_dirigens_rule` または `unknown`
- `eligibility_source_url`: 規則を適用したカードは上記PDFのURL、未適用は空

`eligible_2/3/4` は `yes` / `no` / `unknown` の三状態を維持する。`unknown` は
「未確認」であり「使用不可」と混同しない。

## 再適用

```bash
python3 scripts/apply_card_eligibility.py --check   # 差分の確認のみ
python3 scripts/apply_card_eligibility.py           # 台帳と per-card スナップショットを更新
```

スクリプトは A〜D に規則を適用し、E は `unknown` のままにする。`--check` は書き込まない。
`data/bga-card-specs/*.json` の `bga.eligibility` も同じ値へ揃える。

## 結果（適用後）

- `eligible_4 = yes`: **672枚**（A〜D の 001–168 全枚）
- `eligible_4 = no`: **0枚**（A〜D に4人不可の番号帯はない）
- `eligible_4 = unknown`: **168枚**（Eセット全枚）
- `eligible_2 = no`: 84枚（A〜D の 127–147 帯）
- `eligible_3 = no`: 0枚

## 注意

- この規則は**人数適格のみ**を与える。カード効果・コスト・禁止区分の正しさは保証しない。
- BGA上での実際の使用可否（`bga_playable`）と禁止区分（`ban`）は別列で管理する。
- 4人戦プールへカードを入れる判定は、人数適格だけでなく効果の公式照合と実装状態も必要。

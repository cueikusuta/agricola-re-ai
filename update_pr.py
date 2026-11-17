import urllib.request, json, subprocess
token = subprocess.check_output(['gh', 'auth', 'token']).decode().strip()
req = urllib.request.Request('https://api.github.com/repos/cueikusuta/agricola-re-ai/pulls/327', headers={'Authorization': f'Bearer {token}', 'Accept': 'application/vnd.github.v3+json'})
with urllib.request.urlopen(req) as response:
    data = json.load(response)
    body = data.get('body', '')
    # Read the new section to append
    new_section = '''

---

## 修正コミット追加 (HEAD `308ce83`)

### P1: 増分検証のボーナス台帳検査漏れを修正

**問題**: `validate_incremental` と `validate_free_action` が `bonus_points` の変更を検査しておらず、未プレイカードへの不正な加点を含む `post` 状態でも `Pass` を返していた。

**修正**:
- `validate_incremental`: 台帳が変わる遷移では `Unsure` を返し（フォールバックで全件検証へ）、soundness 契約を維持。台帳変更はカード効果で合法的に起こり得るが、安価に正当性を証明できないため。
- `validate_free_action`: フリーアクションでは台帳が変わらないことが確実（カード効果ディスパッチなし）なので、変更は確定違反として `Fail`。
- 契約コメントの列挙に `post.bonus_points` を追加。

**再現テスト（修正前 RED → 修正後 GREEN）**:
- `validation::tests::tampered_bonus_points_on_unplayed_card_declines_incremental`
- `validation::tests::tampered_bonus_points_on_unplayed_card_declines_free_action`

### P2: レガシーセーブでの非空ボーナス台帳受理を修正

**問題**: `integrity_hash` の無いセーブ（レガシー形式）で `bonus_points` が非空のまま受理されていた。改竄済みのスタンプ付きセーブから `integrity_hash` を削除すると、レガシーとして通ってしまう。

**修正**: `load_json` で `integrity_hash` が無いかつ `bonus_points` が非空の場合は拒否（`\"legacy save must not carry a non-empty bonus point store\"`）。空台帳の正規レガシーセーブ（v5 フィクスチャ）と v5 セーブ往復は引き続き受理。

**再現テスト（修正前 RED → 修正後 GREEN）**:
- `pending::tests::tampered_bonus_points_without_integrity_hash_is_rejected`
- `pending::tests::legacy_save_with_empty_bonus_points_still_loads`（回帰テスト）

### ゲート結果

- `cargo fmt --check` 合格
- `cargo clippy --locked -- -D warnings` 合格
- `cargo test --locked`: **全テスト合格**（lib 331件 + bonus_point_store 8件 + pending 23件 + 他統合テスト）

### 維持・修正した既存テスト
- `tests/bonus_point_store.rs` 8件：全件維持
- `src/validation.rs` tests 29件：全件維持 + 新規2件
- `src/pending.rs` tests 23件：`a_legacy_save_without_a_stamp_still_loads` 維持 + 新規2件
- v5 フィクスチャ往復テスト：`legacy_v5_fixture_round_trip_keeps_an_empty_store` 維持

'''
    updated_body = body + new_section
    # Update PR
    import base64
    payload = json.dumps({'body': updated_body}).encode()
    update_req = urllib.request.Request('https://api.github.com/repos/cueikusuta/agricola-re-ai/pulls/327', data=payload, headers={'Authorization': f'Bearer {token}', 'Accept': 'application/vnd.github.v3+json', 'Content-Type': 'application/json'}, method='PATCH')
    with urllib.request.urlopen(update_req) as response:
        print(f"PR updated: {response.status}")
        print(response.read().decode()[:500])
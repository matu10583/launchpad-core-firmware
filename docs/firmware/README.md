# 機種別CoreFW説明書

ここにあるのは**このリポジトリの現在のソース**を対象にした説明です。純正Novation FWのガイド、実機の全機能保証、ファームウェア書込み手順ではありません。表のビルド別名を間違えないでください。

| 機種 | ビルド別名 | 説明書 |
| --- | --- | --- |
| Launchpad X | `lpx` | [開く](lpx.md) |
| Launchpad Pro Mk3 | `lppmk3` | [開く](lppmk3.md) |
| Launchpad Mini Mk3 | `mini` | [開く](mini.md) |
| Launchpad Pro（旧モデル） | `lpp` | [開く](lpp.md) |
| Launchpad Mk2 | `mk2` | [開く](mk2.md) |
| Launchpad Mini Mk1 | `minimk1` | [開く](minimk1.md) |
| Launchpad S | `lps` | [開く](lps.md) |

## 正しさを保つ仕組み

`tools/generate_fw_manuals.py` はCargo feature・`AppId`・Setupの入力番号などをソースから取り出し、機種別説明書を作ります。操作文は出典と未検証事項を添えたレビュー対象です。`source-review.json` に各機種で参照するソースのSHA-256を保存し、コードが変わると `--check` とCIが失敗します。**コード変更だけで説明文を自動で正しいものへ書き換えることはできません。**

```sh
python3 tools/generate_fw_manuals.py --check
# 関連ソースを読んで説明文を直した後に、明示的にレビュー済みにする
python3 tools/generate_fw_manuals.py --accept-source-changes
# ソース未変更で生成テンプレートだけ変えた場合
python3 tools/generate_fw_manuals.py --write
```

CIは説明書の差分とソース変更の両方を検査します。ハッシュは「このソースについて確認した」という記録であり、実機動作の証拠ではありません。実機で確認した事実は機種別の文面で明示し、それ以外は「ソース上の挙動」または「未検証」とします。

## WebFlasherから開く場合

機種ごとの `.md` は独立したファイルです。[`catalog.json`](catalog.json) の `updater_product` はFWパッケージの `manifest.json` にある `updater.product` と同じ値で、対応する `manual` が説明書のリポジトリ内パスです。WebFlasher側では選択中のパッケージのこの値を照合し、**同じ `source_commit` の説明書**を取得するのが安全です。Markdown中の相対リンクも同じコミットのリポジトリパスを基準に解決してください。`source_commit` がない／ソースが未コミット状態（`source_dirty: true`）の場合は説明書との同一性を主張しないでください。現リポジトリにはWebFlasher本体は含まれず、WebFlasherへのボタン追加はまだ行っていません。

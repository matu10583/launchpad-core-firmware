# `code` のファームウェアCI

Launchpad X／Pro Mk3の通常ビルドは [GitHub Actions](../.github/workflows/x-pro-mk3-check.yml) が担当します。`code` へのpush、`code` 宛てPR、手動の `workflow_dispatch` で起動し、Rust 1.94.1 で各機種の `xtask` テストとリリース用パッケージ作成を別々に行います。[説明書照合](../.github/workflows/firmware-manuals.yml)は別のワークフローです。

各ジョブは `build/core-launchpad-{x|pro-mk3}.syx`、対応する `.manifest.json`、`SHA256SUMS.txt` を検証し、GitHub Actions のArtifactとして14日間保存します。Artifact名にはソースSHAと `UNVERIFIED` を含めます。ビルドは実機へ書き込まず、GitHub Releaseへも自動公開しません。WebFlasherへの自動配信も行いません。ダウンロードはリポジトリのActions画面で対象コミットの実行結果から行ってください。

従来のTeamCity X／Pro Mk3構成は、GitHub Actions側の成功を確認した後にVCSトリガーを停止してください。両方が有効な間は重複ビルドになります。既存の[手動Releaseワークフロー](../.github/workflows/release.yml)と過去の機種別試作ブランチのワークフローはこの移行の対象外です。

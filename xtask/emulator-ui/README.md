# ローカル・ブラウザエミュレータ（AppHost接続）

このツールはFWの実際の`AppHost`とNote/Custom/Setup等のAppソースをホスト上で実行する。ブラウザは入力と出力の表示だけを担当し、ルーティングをJavaScriptで再実装しない。

```sh
cargo run --offline --locked -p xtask --bin emulator-web -- x
# Pro Mk3 は必ず機種別 feature で別ビルド
cargo run --offline --locked -p xtask --bin emulator-web --no-default-features --features launchpad-pro-mk3 -- pro-mk3
```

ブラウザで `http://127.0.0.1:7878/` を開く。ポートはローカルループバック専用。ホスト側でRustがない場合、Dockerホストで利用可能なビルドイメージから起動できる（`/path/to/repo`は実在するホスト上のパスに変更する）。

```sh
docker run --rm --network host \
  --mount type=bind,src=/path/to/repo,dst=/work,readonly \
  --mount type=volume,src=corefw-cargo-registry,dst=/usr/local/cargo/registry,readonly \
  -e RUSTUP_TOOLCHAIN=1.94.1-x86_64-unknown-linux-gnu -e CARGO_TARGET_DIR=/tmp/target \
  -w /work corefw-build:1.94.1 \
  cargo run --offline --locked -p xtask --bin emulator-web -- x
```

ブラウザが別の端末ならSSHポート転送を使う。インターネット向けにポートを公開しない。実機MIDI機器には接続せず、FWの書き込みも行わない。`cargo test --offline --locked -p xtask`でUIのHTTP入出力も含めテストする。

## 操作

- 8×8と外周のパッドを押下／解放。グリッドの番号は初期状態で非表示、「パッド番号を表示」で切り替える。上段のSession・Note・Customなどの物理ラベルは常時表示（Pro Mk3は別のラベル）。番号はツールチップと読み上げ用ラベルでも確認できる。`velocity`は押す瞬間の値。LEDのRGBとNovationパレット番号はサーバーで別々に保持し、パレット色の画面表示のみ近似値。
- スマホでは盤面を画面幅と高さに収めて表示し、説明文は「使い方と再現範囲」に折りたたむ。操作欄は盤面の下へ続くため、縦スクロールで利用する。
- Note／Custom／Setupボタンは診断用の直接切替。実際のモード遷移を検証する場合はパッドを使う。Xは97でCustom、96でNote、95を押したまま仮想時間を500ms進めるとSetup。Pro Mk3は96でCustom、94でNote、0でSetup。Setup画面の12／11／13（Proは14／24も）でモード選択後、Xは95、Proは0で退出。
- ガイド: BEGIN→SET_PAD→END。QUERY／GET_LAYOUT／KEEPALIVE、ポート付きの任意MIDI 3バイトとSysEx `F0…F7`、仮想時間を送信できる。テスト用HTTPコマンド`aftertouch <pad> <value>`は実`AppHost`の圧力イベント経路を通す（センサー測定値の再現ではない）。出力欄にはFWが送信したMIDI/SysEx（ポート・16進値）を表示する。
- `GET_LAYOUT`等は生の16進出力。コマンドを送るだけではUSB RX/TXのパケット化やキュー制限は再現しない。

## 次の検証境界

次はUSB-MIDI codecとキュー、圧力フィルタとLED走査の検証を段階的に検討する。`AppHost`の実ルーティングとSetup／回転は接続済みだが、**USB・ADC/FSR・Pro M0（専用Roadrunner SysExを含む）・表示の実色・設定の永続化・実機起動**は再現していない。実機確認と同等とは扱わない。開発用のローカル一時プロセスであり、配備・外部公開・観測専用サーバー追加はしない。データはプロセス内のみで、SysEx本文はログに保存しない。

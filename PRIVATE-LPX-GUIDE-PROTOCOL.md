# PRIVATE / NOT FOR PUBLIC RELEASE — Launchpad X / Pro MK3 練習ガイド試作

**この文書・実装・`.syx`をPUBLICなfork、GitHub Actions/Artifacts、Release、配布先へ送らないこと。** ローカル専用ブランチ `private-lpx-guide-prototype` にのみ置く個人・非商用・非公開の試作。MIDI Associationの特別ID `0x7D` は公開製品では使用できない。将来公開する場合は、利用権のあるSysEx IDを取得し、ソース・仕様・アプリを再検証する。Novation ID・`0x7C`・Universal SysExを代用しない。**実機検証は未実施。**

## 受信先とバイト配置（**この私用試作内のみ**）

USB `LPX (MIDI)`ではX Note Mode、USB `PRO MK3 (MIDI)`ではPro MK3のNoteまたは固定Custom Modeでのみ動く。DAWポートと他モードは対象外。すべての数字は十六進、値は7bit。フレームは必ず次の完全一致形式とする。

```text
F0 7D 43 46 57 model 01 cmd [payload] F7
   │  C  F  W  機種  v1
   └ 個人・非公開試作に限定（公開用IDではない）
```

`model=0C`はX、`0E`はPro MK3。機種の不一致は拒否。`src/app/guide.rs`で共通のパーサ・期限・左右別状態を管理し、各モードは自分の配列と移調を用いて描画する。Webは機種名判定やQUERYを前提にせず、時間再生では両モデルへコマンドを送信する。他機種宛のフレームはFWが無視し、待機モードのみGET_LAYOUT応答を使う（Webの実装は今回変更しない）。

**私用ローカル既定:** このブランチでは`src/app/guide.rs`に`0x7D`を固定する。`CORE_GUIDE_SYSEX_ID`の指定は不要で、指定しても値の切替には使用しない。`GITHUB_ACTIONS`下でのビルドは拒否するが、公開pushそのものは防げないため**この履歴を公開forkへpushしない**。公開でソースを管理するなら公開`code`を起点に、私用ID・文書・生成物を含めない新しい履歴で行う。

| `cmd` | 送信payload | 動作 / 返答 |
| --- | --- | --- |
| `01` QUERY | なし | 読み取り専用。`41 03 mode active`（`01`左右ガイド、`02`レイアウト；旧方式のセッション中抑制bit`04`は廃止）。`mode=01` Note / `02` Pro MK3固定Custom；`active=00/01`。QUERYだけでは状態変更せず、Webの時間再生には必須ではない。 |
| `02` GET_LAYOUT | なし | 読み取り専用。X: `42 preset transpose+64 valid[10] pitches[64]`（全85バイト）。Pro MK3: `42 layout root scale overlap offset_low offset_high valid[10] pitches[64]`（全89バイト）。`layout=00`Chromatic / `01`Scale / `02`固定Custom。Proの符号付きoffsetは`(offset+8192)`を下位・上位7bitに分けた値。8行×8列を下行左から上行右へ、slot0=11、slot7=18、slot8=21、slot63=88。`valid[slot/7]`のbit`slot%7`が1なら音程`pitches[slot]`有効、0ならpitch欄は0で無効。`valid[9]`のbit1～6は常に0。64バイトの音程は0～127（127も有効）。レイアウト照会は押下状態を変更しない。 |
| `10` BEGIN | なし | ガイド状態を消し、5秒間のセッション開始。`50 10 01`を応答。 |
| `11` KEEPALIVE | なし | セッション中のみ5秒へ更新し`50 11 01`を応答。アプリは2秒ごとに送る。 |
| `12` END | なし | セッションを終了しガイド表示を消去、`50 12 00`を応答。 |
| `20` SET | `layer side pitch color` | セッション中のみ受付。`layer=00`予告、`01`現在；`side=00`左4列、`01`右4列、`02`両側；`pitch=00..7F`；`color=01..7F`は固定Novationパレット番号、`00`は指定対象だけを解除。SET成功時は5秒へ更新。 |

返答も同じヘッダと終端を使用し、**各機種のUSB演奏用MIDIポートにのみ送信**。不正なID・モデル・版・長さ・データ（`80`以上）・未知コマンドは無視。`SET`は音や演奏MIDI出力を変更せず、押下パッドIDは送信しない。問い合わせのレスポンスは試作FW特有で、通常のDAW MIDIは変更しない。

**アプリ移行上の注意:** 旧X専用仕様書の`valid[8]`はSysExデータの7-bit制約に反し、最上位ビットが立つ通常の配列でGET_LAYOUT応答が失敗するため、XとPro双方を`valid[10]`に修正した。Xの`model=0C`と送信コマンドは不変だが、**GET_LAYOUT応答の長さ・pitch開始位置は互換ではない**。アプリは85/89バイトを機種ごとに確認してからデコードし、旧応答を誤解釈しないこと。プロトコルは未配布の私用試作なので、この修正をv1の確定仕様とする。

## 表示・ライフサイクル

左右×予告/現在×pitchを独立に保持する。同じpitchが同じ側の複数パッドにあるときは全て表示する。描画時に現在の配列と移調から64パッドの音程を再計算する。希望側にpitchがあれば希望側だけへ表示し、希望側に存在せず反対側にあれば反対側へフォールバックする（左右対称）。両側になければ表示しない。左右の同じパッドで衝突したら**現在 > 予告、同じレイヤーならそのパッド側の手 > 反対側からのフォールバック**。本体で押しているパッドの予告は隠し、現在は押下表示より優先。同じパッドを連続打鍵する際はアプリ側で現在を解除または予告色へ更新する（1個のLEDに2色は同時表示不可）。本体の配列・移調変更後もガイドのpitch状態は保持して再判定する。

**X Note ModeとPro MK3 Note／固定CustomはSysExガイド専用。** 旧ch1～5のNote On/OffとProの選択チャンネル入力によるLEDフィードバックは廃止。BEGIN前・セッション中・END後・期限切れ後とも受信Note On/Offで点灯しない。演奏パッドのMIDI送信とチャンネル選択UIは維持。モード退出ではガイドを消去。USB切断通知に依存せず最後のSET/KEEPALIVEから5秒で自動終了・消灯する。Webは旧方式へのフォールバックを使わない。

## 検証の限界

ユーザー報告による検証：NASにキャッシュ済みの`rust:slim`コンテナへFWソースを**読み取り専用**でマウントし、**Rust 1.96.1**で`cargo test --locked -p xtask`を実行して**13件すべて成功**。`cargo check --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`も成功。コンテナは終了し、公開forkへのpush・公開CI/Artifact・実機書き込みは行っていない。こちらのセッションでは実行ログを直接照合していない。

上記のユーザー報告は**X専用の旧コミット`96568a1`時点**の結果。共通化後のローカル検証では`rust:1.94.1-slim`を基に、指定と同じ**Rust 1.94.1**・`thumbv7em-none-eabihf`・`rust-src`・`llvm-tools`を入れたDockerイメージ`corefw-build:1.94.1`を作成した。Cargo依存のキャッシュはローカルDocker volume `corefw-cargo-registry`。初回だけ依存を取得し、以降はソースの**読み取り専用**マウント、キャッシュ参照、ネットワーク無効のコンテナで次を実行して成功した。

- `cargo test --locked -p xtask`: **15件成功**（共通ガイド5件を含む）。
- `cargo check --offline --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`と`--features launchpad-pro-mk3`: ID未指定・個人用ID指定の**両方で成功**。
- `cargo build --offline --locked --release --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`と`--features launchpad-pro-mk3`: 個人用ID指定で**両方成功**。生成物は使い捨てコンテナの`/tmp/target`にのみ置き、終了時に破棄した。公開CI/Artifactへの掲載・実機書き込みはしていない。

再検証時はこのローカルイメージとvolumeを再利用できる。Dockerがホスト側で動く環境ではマウント元はNASホストの`/home/matu10583/docker-apps/opencode/workspace/projects/corefw-x-pro-mk3-integration`（コンテナ内の`/workspace/projects/...`ではない）。ツールチェーンの自動同期を避けるため`RUSTUP_TOOLCHAIN=1.94.1-x86_64-unknown-linux-gnu`を指定する。**以下は過去の実装時点の記録**であり、現行版では私用IDを常に有効にしている。いずれも**実機USB・LED・演奏動作の確認ではない**。

**旧コミット`7d3b3a4`時点のX旧LED入力廃止の検証:** `xtask`にホスト用の軽量ドライバを追加し、FW本体の`test=false`でもX NoteAppの実コードのテストを実行可能にした。Rust 1.94.1で`cargo test --offline --locked -p xtask`はID未指定/個人用ID指定の双方で**25件成功**。両機種の個人用ID指定`cargo check --offline --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x` / `launchpad-pro-mk3`も成功。現行版は下記の検証結果を参照する。

**現行の私用変更:** Rust 1.94.1のローカルDocker（ネットワークなし・FWソース読み取り専用）で、`CORE_GUIDE_SYSEX_ID`未指定の`cargo test --offline --locked -p xtask` **37件成功**。X Note／Pro Note／Pro固定Customの実コードで旧LED入力の無視、選択チャンネルでの演奏MIDI維持、左右フォールバック・移調／配列追従・heldと色優先、機種model／版／長さの拒否を確認。両機種とも環境変数未指定で`cargo check --offline --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`および`--features launchpad-pro-mk3`成功。`GITHUB_ACTIONS=true`のホストテストコンパイルは私用ビルド拒否の期待どおり失敗。これらは実機のLED・MIDI動作の検証ではない。既存の`build/`内の生成物は**この変更を含まない旧FW**なので新仕様として扱わない。

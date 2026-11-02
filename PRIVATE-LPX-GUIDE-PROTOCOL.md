# PRIVATE / NOT FOR PUBLIC RELEASE — Launchpad X / Pro MK3 練習ガイド試作

**この文書・実装・`.syx`をPUBLICなfork、GitHub Actions/Artifacts、Release、配布先へ送らないこと。** ローカル専用ブランチ `private-lpx-guide-prototype` にのみ置く個人・非商用・非公開の試作。MIDI Associationの特別ID `0x7D` は公開製品では使用できない。将来公開する場合は、利用権のあるSysEx IDを取得し、`src/app/guide.rs` のビルド時ID指定を適切に変更して仕様/版/アプリを再検証する。Novation ID・`0x7C`・Universal SysExを代用しない。**共通化後の両機種についてホストテスト・コンパイル・ローカルreleaseビルド成功（下記参照）。実機検証は未実施。**

## 受信先とバイト配置（**この私用試作内のみ**）

USB `LPX (MIDI)`ではX Note Mode、USB `PRO MK3 (MIDI)`ではPro MK3のNoteまたは固定Custom Modeでのみ動く。DAWポートと他モードは対象外。すべての数字は十六進、値は7bit。フレームは必ず次の完全一致形式とする。

```text
F0 7D 43 46 57 model 01 cmd [payload] F7
   │  C  F  W  機種  v1
   └ 個人・非公開試作に限定（公開用IDではない）
```

`model=0C`はX、`0E`はPro MK3。機種の不一致は拒否。`src/app/guide.rs`で共通のパーサ・期限・左右別状態を管理し、各モードは自分の配列と移調を用いて描画する。**X用に接続済みのアプリは`0C`のままで変更不要**。Pro MK3へ接続する場合のみ`model=0E`を使う。

**ビルド時ID設定:** ソースの`src/app/guide.rs`には私用IDを固定しない。`CORE_GUIDE_SYSEX_ID`を指定しないビルドでは私用SysExの受信・応答を無効にする。個人用FWのローカルビルドでのみ`CORE_GUIDE_SYSEX_ID=7D`を渡す（2桁の16進数、1バイトIDに対応）。IDを含む設定ファイルやビルド済みFWを公開リモート・Actions・Artifactに送らない。現状の**ローカルブランチの過去のコミットとこの私用文書にはIDが残る**ため、そのブランチをそのまま公開forkへpushしない。公開でソースを管理するなら公開`code`を起点に、IDの値とこの文書を含めない**新しい履歴**で行い、CIがID未指定でビルドすることを確認する。

| `cmd` | 送信payload | 動作 / 返答 |
| --- | --- | --- |
| `01` QUERY | なし | 読み取り専用。`41 capabilities mode active`。Xの`capabilities=03`（`01`左右ガイド、`02`レイアウト）。Pro MK3の`capabilities=07`は今回変更しない既存仕様（bit`04`はProのみセッション中の旧MIDI抑制）。`mode=01` Note / `02` Pro MK3固定Custom；`active=00/01`。QUERYだけでは状態変更しない。 |
| `02` GET_LAYOUT | なし | 読み取り専用。X: `42 preset transpose+64 valid[10] pitches[64]`（全85バイト）。Pro MK3: `42 layout root scale overlap offset_low offset_high valid[10] pitches[64]`（全89バイト）。`layout=00`Chromatic / `01`Scale / `02`固定Custom。Proの符号付きoffsetは`(offset+8192)`を下位・上位7bitに分けた値。8行×8列を下行左から上行右へ、slot0=11、slot7=18、slot8=21、slot63=88。`valid[slot/7]`のbit`slot%7`が1なら音程`pitches[slot]`有効、0ならpitch欄は0で無効。`valid[9]`のbit1～6は常に0。64バイトの音程は0～127（127も有効）。レイアウト照会は押下状態を変更しない。 |
| `10` BEGIN | なし | ガイド状態を消し、5秒間のセッション開始。`50 10 01`を応答。 |
| `11` KEEPALIVE | なし | セッション中のみ5秒へ更新し`50 11 01`を応答。アプリは2秒ごとに送る。 |
| `12` END | なし | セッションを終了しガイド表示を消去、`50 12 00`を応答。 |
| `20` SET | `layer side pitch color` | セッション中のみ受付。`layer=00`予告、`01`現在；`side=00`左4列、`01`右4列、`02`両側；`pitch=00..7F`；`color=01..7F`は固定Novationパレット番号、`00`は指定対象だけを解除。SET成功時は5秒へ更新。 |

返答も同じヘッダと終端を使用し、**各機種のUSB演奏用MIDIポートにのみ送信**。不正なID・モデル・版・長さ・データ（`80`以上）・未知コマンドは無視。`SET`は音や演奏MIDI出力を変更せず、押下パッドIDは送信しない。問い合わせのレスポンスは試作FW特有で、通常のDAW MIDIは変更しない。

**アプリ移行上の注意:** 旧X専用仕様書の`valid[8]`はSysExデータの7-bit制約に反し、最上位ビットが立つ通常の配列でGET_LAYOUT応答が失敗するため、XとPro双方を`valid[10]`に修正した。Xの`model=0C`と送信コマンドは不変だが、**GET_LAYOUT応答の長さ・pitch開始位置は互換ではない**。アプリは85/89バイトを機種ごとに確認してからデコードし、旧応答を誤解釈しないこと。プロトコルは未配布の私用試作なので、この修正をv1の確定仕様とする。

## 表示・ライフサイクル

左右×予告/現在×pitchを独立に保持する。同じpitchが同じ側の複数パッドにあるときは全てを表示する。描画時に現在の配列と移調から64パッドの音程を再計算し、グリッド範囲外の音程は表示しない（保持はする）。**X Note Modeの優先順位は新ガイド現在 > 未押下パッドの新ガイド予告 > 本体押下 > 基底色**。本体で押しているパッドの予告は隠し、現在は押下表示より優先。同じパッドを連続打鍵する際はアプリ側で現在を解除または予告色へ更新する（1個のLEDに2色は同時表示不可）。Pro MK3の表示処理・旧フィードバックは今回変更しない。

**X Note ModeはSysExガイド専用で、旧ch1～5 Note On/OffによるLEDフィードバックを一切受け付けない。** BEGIN前・セッション中・END後・期限切れ後とも受信した通常Note On/Offで点灯しない。本体押下・解放による`LPX (MIDI)`への演奏ノート送信は従来どおり。移調・配列変更ではガイドを**保持して再描画**する。Note Mode退出では消去。USB切断通知に依存せず、最後のSET/KEEPALIVEから5秒で自動終了・ガイド消灯する（再接続時はQUERYから）。アプリも旧ガイドUI/送信・旧方式へのフォールバックを削除し、Xには本方式を必須とする。Pro MK3の旧方式は今回の変更対象外で、今後の別作業として扱う。

## 検証の限界

ユーザー報告による検証：NASにキャッシュ済みの`rust:slim`コンテナへFWソースを**読み取り専用**でマウントし、**Rust 1.96.1**で`cargo test --locked -p xtask`を実行して**13件すべて成功**。`cargo check --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`も成功。コンテナは終了し、公開forkへのpush・公開CI/Artifact・実機書き込みは行っていない。こちらのセッションでは実行ログを直接照合していない。

上記のユーザー報告は**X専用の旧コミット`96568a1`時点**の結果。共通化後のローカル検証では`rust:1.94.1-slim`を基に、指定と同じ**Rust 1.94.1**・`thumbv7em-none-eabihf`・`rust-src`・`llvm-tools`を入れたDockerイメージ`corefw-build:1.94.1`を作成した。Cargo依存のキャッシュはローカルDocker volume `corefw-cargo-registry`。初回だけ依存を取得し、以降はソースの**読み取り専用**マウント、キャッシュ参照、ネットワーク無効のコンテナで次を実行して成功した。

- `cargo test --locked -p xtask`: **15件成功**（共通ガイド5件を含む）。
- `cargo check --offline --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`と`--features launchpad-pro-mk3`: ID未指定・個人用ID指定の**両方で成功**。
- `cargo build --offline --locked --release --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x`と`--features launchpad-pro-mk3`: 個人用ID指定で**両方成功**。生成物は使い捨てコンテナの`/tmp/target`にのみ置き、終了時に破棄した。公開CI/Artifactへの掲載・実機書き込みはしていない。

再検証時はこのローカルイメージとvolumeを再利用できる。Dockerがホスト側で動く環境ではマウント元はNASホストの`/home/matu10583/docker-apps/opencode/workspace/projects/corefw-x-pro-mk3-integration`（コンテナ内の`/workspace/projects/...`ではない）。ツールチェーンの自動同期を避けるため`RUSTUP_TOOLCHAIN=1.94.1-x86_64-unknown-linux-gnu`を指定する。個人用ビルド以外は`CORE_GUIDE_SYSEX_ID`を指定しない。いずれも**実機USB・LED・演奏動作の確認ではない**。

**Xの旧LED入力廃止後の追加検証:** `xtask`にホスト用の軽量ドライバを追加し、`Cargo.toml`でFW本体の`test=false`でもX NoteAppの実コードのテストを実行可能にした。Rust 1.94.1で`cargo test --offline --locked -p xtask`はID未指定/個人用ID指定の双方で**25件成功**（旧ch1～5のNote On/OffをBEGIN前・中・END後に無視、演奏パッド押下/解放時のMIDI送信維持、QUERYの能力値を含む）。両機種の個人用ID指定`cargo check --offline --locked --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-x` / `launchpad-pro-mk3`も成功。Pro MK3のソースには今回差分なし。公開リモートへのpush/CI/Artifact・実機書き込みは行っていない。

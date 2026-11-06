# Pro Mk3 DIN In → USB / CC64 調査用（通常Proビルドに同梱、実験機能）

**DIN MIDI機能はまだ未実装。** 現行CoreFWではPro Mk3のUSBはDAW/MIDIの2ポートだけで、DIN InからのCC64を受信しない。公式FWでは届いたが、現行FWでは届かないことを実機で確認済み。内部UART5はM0ボタン・LED用のためDINとして再設定しない。

`pro-din-probe`はPro Mk3専用の一時診断機能。**通常のPro Mk3 TeamCityビルドに含むが、専用SysExを受け取るまでは実行しない。X用ビルドには含まれない。** MIDI用USBポートで `F0 7D 43 46 57 0E 01 30 F7` を受けたときだけ、GPIO A～F の**現在の入力レジスタ**を40ms読み、各ピンの変化回数をUSB SysExで返す。GPIOのモード・代替機能・プルアップ・出力レベルやUART/M0を変更しない。ただしGPIOポートのクロックはONになり、40msだけ通常の入力処理が遅れる。診断中は演奏しないこと。返信形式:

```
F0 7D 43 46 57 0E 01 70 01 28 [GPIOA pin0..15 (16 bytes)] ... [GPIOF pin0..15] F7
```

`28`は16進で40ms、変化回数は0～127（飽和）。**基準（無入力）と、DIN InへCC64を約10ms間隔で繰り返した状態**の両方を記録し、GPIOポート・ピン番号ごとの差を比べる。USB MIDIモニターで返信の全バイトを保存する。外部鍵盤→DIN InとPC→USBで経路を分け、ソフトウェアのMIDIループバックを無効にする。返答がない、またはパッド・LEDが不安定なら直ちに診断を中止する。

**重要な制限:** 入力がアナログモードで無効化されているピン、M0側の端子、GPIO A～F以外のピンは観測できない。変化の検出だけではMIDI入力ピンとの証明にならず、特定したピンで後から31250bps・8N1の受信データとCC64を検証する必要がある。全てゼロでもDIN入力がないとは結論できない。外部のMidiPort::DinやThruを有効化したビルドではない。

## 実機での確認手順

1. **Pro Mk3構成の新しいTeamCityビルド**が成功し、コミットSHAがこの診断変更を含むことを確認する。ZIP内のPro Mk3用`.syx`だけを使う。従来の正常に動くPro Mk3版と純正復旧手段を手元に残す。実機への書込みはユーザーのみが行う。公式のブートローダー手順は[NovationのSetupガイド](https://userguides.novationmusic.com/hc/en-gb/articles/25494545308306-Using-the-Launchpad-Pro-MK3-s-Setup-page#bootloader-menu)を参照。**USBの通常の演奏用MIDIポートへFWを書き込まない**。本体のSetupを押しながらUSB接続して、表示されたブートローダーポートを選ぶ。誤機種ファイルを使わない。
2. Pro Mk3を通常起動し、PCのUSB MIDI入力/出力から**PRO MK3 (MIDI)**（DAWではない）に相当するポートを確認。外部USB→DINインターフェースの**DIN Out → Pro Mk3 DIN In**を接続。対応するTRS変換ケーブルを使い、DIN OutをProのDIN Outへ接続しない。DAW等の自動MIDIループバックを停止。音源は接続しなくてよい。
3. PCに追加ツールをインストールせず、[診断ページ `tools/pro_mk3_din_probe.html`](tools/pro_mk3_din_probe.html)をダウンロードしてChrome/Edgeで**ローカルファイルとして開く**。この単一HTMLは外部スクリプト・ネット接続・保存をしない。クリックしてWeb MIDIの**SysExアクセスを許可**し、Pro Mk3 USB **MIDI**入力・出力と、外部USB→DINインターフェースの出力をそれぞれ**ポート名を見て**選択。3つ目にPro Mk3の出力を選ばない。`CORE_GUIDE_SYSEX_ID`が通常の`7D`以外のビルドでは、ページのID欄を変更する。
4. `無入力とCC64送信中を比較`を押す。ページは基準時に1回診断を行い、**外部DINアダプタ出力にch1 CC64=127/0を約10ms間隔で送信**しながら3回診断し、GPIOの差分を表示する。ブラウザを前面に保ち、途中で別のMIDIソフトから送らない。`結果をコピー`で4件の `reply: F0 ... F7` と `GPIO...` 差分を共有する。PCのブラウザはChrome/EdgeのWeb MIDI SysEx対応と安全なコンテキストが必要で、環境によっては`file://`から許可できない。その場合は、信頼できるローカルHTTPS配信/`localhost`で同じHTMLを開くか、下記のPython代替を使う。FWのZIPとHTMLは別々に配布し、**ページを開くだけではFWを書き換えない**。
5. 無返信ならUSBポート・FWリビジョン・`CORE_GUIDE_SYSEX_ID`を確認。出力がすべて0でも非接続とは断定しない。検査後、通常演奏・LED・ボタン操作に異常がないか確認し、不要なら保存済みの通常FWへ戻す。電源再接続で直らない異常があれば診断を中止し、以前のFWへ復旧する。

外部USB→DINアダプタがない場合は、外部鍵盤側からCC64を連打しながらMIDIモニターで手動SysEx要求を送れるが、40msの観測に偶然入らないことがある。**返信がゼロだけの結果では原因を確定しない。**

Pythonを使える場合は従来の`tools/pro_mk3_din_probe.py`も残す。`python -m pip install mido python-rtmidi`、`python tools/pro_mk3_din_probe.py --list`でポートを確認後、`python tools/pro_mk3_din_probe.py --pro-in "<Pro USB MIDI input>" --pro-out "<Pro USB MIDI output>" --din-out "<外部DINインターフェースのoutput>"`を実行。ブラウザと同じプロトコル・結果を使う。どちらも実機でまだ未検証。

## ビルドと制約

通常のPro Mk3ビルド（追加feature不要）:

```
cargo build --offline --locked --release --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-pro-mk3
```

`CORE_GUIDE_SYSEX_ID`を指定しているビルドは上記の`7D`をそのIDに置き換える必要がある。通常Proビルド#8（`4f9f7f8`）にはまだ含まれず、新しいコミットのTeamCity成果物に含まれる。XとProで共通のガイド応答には新コマンドを追加しない。実験用`0x30`はProだけで通常コマンドに優先して処理する。

以前の試作用`build/pro-din-probe.syx`はgit管理外のローカルファイルであり、**今回のTeamCity成果物ではない**。必ず新しいTeamCityビルドIDとリビジョンを確認する。

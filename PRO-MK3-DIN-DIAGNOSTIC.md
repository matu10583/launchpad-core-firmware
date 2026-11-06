# Pro Mk3 DIN In → USB / CC64 調査用（通常Proビルドに同梱、実験機能）

**DIN MIDI機能はまだ未実装。** 現行CoreFWではPro Mk3のUSBはDAW/MIDIの2ポートだけで、DIN InからのCC64を受信しない。公式FWでは届いたが、現行FWでは届かないことを実機で確認済み。内部UART5はM0ボタン・LED用のためDINとして再設定しない。

`pro-din-probe`はPro Mk3専用の一時診断機能。**通常のPro Mk3 TeamCityビルドに含むが、専用SysExを受け取るまでは実行しない。X用ビルドには含まれない。** MIDI用USBポートで `F0 7D 43 46 57 0E 01 30 F7` を受けたときだけ、GPIO A～F の**現在の入力レジスタ**を40ms読み、各ピンの変化回数をUSB SysExで返す。GPIOのモード・代替機能・プルアップ・出力レベルやUART/M0を変更しない。ただしGPIOポートのクロックはONになり、40msだけ通常の入力処理が遅れる。診断中は演奏しないこと。返信形式:

```
F0 7D 43 46 57 0E 01 70 01 28 [GPIOA pin0..15 (16 bytes)] ... [GPIOF pin0..15] F7
```

`28`は16進で40ms、変化回数は0～127（飽和）。**基準（サステイン操作なし）と、DIN Inの機器からCC64を手動で踏む・離す操作を繰り返す状態**の両方を記録し、GPIOポート・ピン番号ごとの差を比べる。ブラウザは基準3回と操作中最大40回の診断要求をUSBで送り、応答の全バイトを表示する。外部機器→DIN InとPC→USBで経路を分け、ソフトウェアのMIDIループバックを無効にする。返答がない、またはパッド・LEDが不安定なら直ちに診断を中止する。

**重要な制限:** 入力がアナログモードで無効化されているピン、M0側の端子、GPIO A～F以外のピンは観測できない。変化の検出だけではMIDI入力ピンとの証明にならず、特定したピンで後から31250bps・8N1の受信データとCC64を検証する必要がある。全てゼロでもDIN入力がないとは結論できない。外部のMidiPort::DinやThruを有効化したビルドではない。

## 実機での確認手順

1. **Pro Mk3構成の新しいTeamCityビルド**が成功し、コミットSHAがこの診断変更を含むことを確認する。ZIP内のPro Mk3用`.syx`だけを使う。従来の正常に動くPro Mk3版と純正復旧手段を手元に残す。実機への書込みはユーザーのみが行う。公式のブートローダー手順は[NovationのSetupガイド](https://userguides.novationmusic.com/hc/en-gb/articles/25494545308306-Using-the-Launchpad-Pro-MK3-s-Setup-page#bootloader-menu)を参照。**USBの通常の演奏用MIDIポートへFWを書き込まない**。本体のSetupを押しながらUSB接続して、表示されたブートローダーポートを選ぶ。誤機種ファイルを使わない。
2. Pro Mk3を通常起動し、**手元のサステイン送信機器のMIDI Out → Pro Mk3 MIDI In、Pro Mk3 USB → PC**だけを接続。別のUSB→DINアダプタは不要。PCのUSB MIDI入力/出力から**PRO MK3 (MIDI)**（DAWではない）に相当するポートを確認。DAW等の自動MIDIループバックを停止。音源は接続しなくてよい。
3. PCに追加ツールをインストールせず、[診断ページ `tools/pro_mk3_din_probe.html`](tools/pro_mk3_din_probe.html)をダウンロードしてChrome/Edgeで**ローカルファイルとして開く**。この単一HTMLは外部スクリプト・ネット接続・保存をしない。クリックしてWeb MIDIの**SysExアクセスを許可**し、Pro Mk3 USB **MIDI**入力・出力の2つだけを選ぶ。`CORE_GUIDE_SYSEX_ID`が通常の`7D`以外のビルドでは、ページのID欄を変更する。
4. サステインを触らずに`診断開始`を押し、基準3回が終了するのを待つ。3秒のカウントダウン後、**8秒間、サステインを繰り返し踏む・離す（1秒に2～4回を目安）**。ページが間隔を空けて診断要求を繰り返し、最大40個の40ms窓でGPIOの変化を観測する。ブラウザを前面に保つ。`結果をコピー`で `baseline` と `active` の全 `F0 ... F7` 応答および `GPIO...` 差分を共有する。1回だけのペダル操作は40msの窓に入らない可能性がある。PCのブラウザはChrome/EdgeのWeb MIDI SysEx対応と安全なコンテキストが必要で、環境によっては`file://`から許可できない。その場合は、信頼できるローカルHTTPS配信/`localhost`で同じHTMLを開く。FWのZIPとHTMLは別々に配布し、**ページを開くだけではFWを書き換えない**。
5. 無返信ならUSBポート・FWリビジョン・`CORE_GUIDE_SYSEX_ID`を確認。**旧診断FW（TeamCity Pro Mk3 #9 / コミット`51c1051`）は応答107バイトに対してUSB送信上限が32パケット（96バイト）だったため、必ずタイムアウトする。送信上限を修正した新しいビルドが必要。** 出力がすべて0でも非接続とは断定しない。検査後、通常演奏・LED・ボタン操作に異常がないか確認し、不要なら保存済みの通常FWへ戻す。電源再接続で直らない異常があれば診断を中止し、以前のFWへ復旧する。

### 実機で得られた結果と次の診断

送信上限修正後、実機から基準3回・サステイン操作中40回の診断応答を受信した。両方ともGPIOA11/A12が127（飽和）、ほかは0で変化増分なし。A11/A12はサステインに相関していない。GPIOのモードを変更せずにIDRだけ読んだ結果なので、未設定・アナログピンやM0側の端子にDINがある可能性は残る。この結果でDINの接続ピンは確定できない。

次の診断は同じ通常Pro Mk3ビルドに、読み取り専用コマンド `F0 7D 43 46 57 0E 01 31 [port=00..05] F7` を追加する。応答は `F0 7D 43 46 57 0E 01 71 [port] [clock enabled 00/01] [pin0 mode,AF,pull,level] ... [pin15 mode,AF,pull,level] F7`。modeは0入力/1出力/2代替/3アナログ、pullは0なし/1上/2下。診断ページが基準値の直後にA～Fを1回ずつ読み、元の40ms診断も続行する。**レジスタ値は候補絞り込みのためで、配線や31250bps受信の証明にはならない。** このコマンドではGPIOモード・UART5・M0・外部フラッシュを書き換えない。新しいHTMLを使う場合は`0x31`を含む新しいFWが必要で、古いFWは無応答となる。ビルドIDとリビジョンを照合してからユーザーが書き込む。

実機の`0x31`応答ではGPIO A～Fの観測対象にanalogモードはなく、主なUART候補はPA9/PB7 (AF7, USART1 TX/RX)、PE7/PE8 (AF8, UART7 RX/TX)、PE0/PE1 (AF8, UART8 RX/TX)だった。PC12/PD2 (AF8, UART5 TX/RX)は既知の内部M0通信であり、DIN用には変更しない。これらのGPIO設定はUART周辺回路が実際に有効か、DINの配線先かを証明しない。GPIOレベルの1回の読み取りや差分ゼロも同様。

そのため追加の読み取り専用コマンド `F0 7D 43 46 57 0E 01 32 [index] F7` で、USART1=0、UART7=1、UART8=2、UART5(M0)=3のクロック有効フラグとCR1/CR3/BRR/ISRを読む。応答は `F0 7D 43 46 57 0E 01 72 [index] [clock] [CR1 5 bytes] [CR3 5 bytes] [BRR 5 bytes] [ISR 5 bytes] F7`。32bitレジスタ値を各7bitのリトルエンディアン5バイトで表す。クロック無効ならレジスタは読まず0で埋める。`RDR`を読まず、UART設定・M0通信・受信データを変更しない。ページは設定読取の直後に4件の状態を表示する。**UARTが有効でもDIN配線やCC64の受信は確定しない。** 新ページには`0x32`を含む新FWが必要。

実機の`0x32`応答ではUSART1/UART7/UART8のクロック・UE・REがいずれも1、RXNE=1、ORE=0、FE=1だった。UART5(M0)もクロック・UE・RE=1だがRXNE=0。USART1のBRRは`0x3AA`、UART7/8は`0x67E`。現在の起動時クロック設定からAPB2=108MHz、APB1=54MHzと仮定するとそれぞれ約115.1kbps、約32.49kbpsとなる（通常の31250bpsとの差がある）。**この算出値は実機クロックの実測ではない。** RXNEやFEは以前から残った状態かもしれず、1回の状態だけではDINからデータが来た証拠にならない。次は**FWを変更せず**、同じ`0x32`をサステイン操作の前後に追加で送り、OREの0→1等の変化を比較する。UART5(M0)を含めRDRは読まない。OREが変化しても内容がCC64かどうかは分からず、変化しなくてもDINがないとは断定しない。

この比較には`0x32`対応FWをそのまま使い、**ページHTMLだけ更新**する。最初に「対照試験」にチェックを入れてペダルに触れず1回、**本体のUSBを切り離して再接続**してからチェックを外して8秒間繰り返し踏む・離す操作を1回実行し、両方の`uart ...`と`uart after ...`行・状態差分を記録する。再接続後にWeb MIDIポートを選び直す。OREは状態フラグであって受信したMIDIバイトではなく、一度立つと残るため各試行の前に電源を入れ直し、ペダルとの相関を見るため対照試験が必要。

40ms単位の窓の間には観測されない時間があるため、**返信がゼロだけの結果では原因を確定しない。** ペダルの送信が押下・解放の瞬間だけなら繰り返し操作が必要。

追加の外部USB→DINアダプタを持つ場合だけ、従来の`tools/pro_mk3_din_probe.py`も選べる。**今回の実機確認には不要。** ブラウザもPythonも実機ではまだ未検証。

### 公開純正FWによるオフデバイス調査（実機操作なし）

- [Novation Componentsの公開FW一覧](https://components.novationmusic.com/api/v2/firmwares)には`product: launchpad-pro-mk3`の純正v1.2.2（build 483、ID 153）が掲載され、[配布先](https://components.novationmusic.com/api/v2/firmwares/153/file)から`.syx`を取得できる（配布先はリダイレクトされる）。`python3 tools/syxtool.py --to-bin <取得したsyx> <出力bin>`で展開でき、展開後458752バイトの先頭はSTM32向けベクタテーブル（初期SP=`0x20000400`、リセットベクタ=`0x0807B539`）だった。M0専用FWや別製品M-VAVE FM-1のFWではなく、Pro Mk3メインMCU用イメージを調べられる。**これは書込みを推奨するものではない。**
- 展開イメージのオフセット`0x67338`付近に`USART1`ベースアドレス`0x40011000`と値`0x1C442`（115778）が、`0x67388`付近に`UART4`ベースアドレス`0x40004C00`と値`0x7AAF`（31407）が、`0x673D8`付近に`UART7`ベースアドレス`0x40007800`と値`0x7AAF`（31407）が、`0x67428`付近に`UART5`ベースアドレス`0x40005000`と値`0x5B3EF5`（既存コードのM0通信速度定数と一致）が並ぶ。これらは**UART初期化構造体のボーレート指定と思われるが、逆アセンブルによる使用経路の確認は未実施**。`UART8`のベースアドレスもイメージに現れるが、この構造体群では対応する速度値を確認できない。UART4は現行FWの`0x32`診断対象外で、公開イメージだけではDIN Inに接続しているUARTを判定できない。
- 現行FWの設定はHSE=24MHz、SYSCLK=216MHz、APB1=54MHz、APB2=108MHz。通常の16倍オーバーサンプリング、PCLKを直接使うという**仮定**なら、31250bps用のBRRはUART7/8/4（APB1）で`0x6C0`（1728）、USART1（APB2）で`0xD80`（3456）。実機のUART7/8=`0x67E`（1662）、USART1=`0x3AA`（938）はこれと一致しない。ただし実際のクロック・オーバーサンプリング・UARTの用途は未確認で、この不一致だけで配線も故障原因も断定しない。
- 前項までの調査ではGPIO名とUART4の初期化経路を追えていなかった。次項の追加資料により静的解析上のDIN候補はUART4/PC11まで絞れた。ただし実機で受信したMIDIバイトはまだ確認していない。

### 純正FWの名前付きGPIO設定とDIN UART4（追加調査）

提供された`launchpad_pro_mk3_midi_pinout.txt`は[Launchpad Firmware Utilityの純正FWコピー385](https://github.com/mat1jaczyyy/LP-Firmware-Utility/blob/master/firmware/old/LPProMK3-385.bin)および[465](https://github.com/mat1jaczyyy/LP-Firmware-Utility/blob/a387645e8725b93e698730cc8f8d10153c1ddb3b/firmware/stock/LPProMK3-465.bin)の静的解析を記録している。メモによると、FW中の`LPP3_BSP_MIDIDIN.c`にある名前付きGPIO設定と初期化経路は次の通り（端子への実配線は未実測）。

| FWの名前 | GPIO / 設定 | 役割 |
| --- | --- | --- |
| `MIDI0Rx` | PC11 / AF8 | UART4 RX、DIN In候補 |
| `MIDI0Tx` | PA0 / AF8 | UART4 TX、Out 1相当と推定 |
| `MIDI1Tx` | PE8 / AF8 | UART7 TX、Out 2相当と推定 |
| `MIDI1ThruMode` | PA7 / GPIO出力 | Out 2 / Thru切替候補。Low/Highの意味は未確定 |

手元でComponentsから取得済みの**別バージョン483**（前項）の展開イメージでも、この4件の28バイトGPIO設定が各1か所、ファイル内オフセット`0x63208`、`0x63224`、`0x63240`、`0x6325C`でメモの記載値と一致した。`LPP3_BSP_MIDIDIN.c`、`MIDI0Tx`、`MIDI0Rx`、`MIDI1Tx`、`MIDI1ThruMode`という文字列も確認した。ただし483で関数の呼出経路やCR2書込みまで独立に逆アセンブルしたわけではない。385/465についてのGPIO初期化の呼出経路、UART4 RDR受信処理、UART4/UART7の`CR2.TXINV=1`書込み確認は提供メモによる解析結果。実装時にはTX反転を考慮し、純正FWの31,407というボーレート指定値を現行FWへ丸写ししない。PA7の切替方向も決め打ちしない。

現行CoreFWのGPIO設定読取でPC11は`mode=0`（入力）、PA0とPA7も`mode=0`であり、現行FWは純正のDIN初期化を再現していない。従来のUART診断`0x32`は**UART4を対象にしていなかった**ため、UART7/8/USART1の状態がペダル操作で変わらなかったことはUART4/PC11の否定材料にならない。次はUART4/PC11のみに絞った受信確認を設計し、CC64バイトの取得を実機で検証してからDIN In→USB転送を実装する。UART5（PC12/PD2）は内部M0用のまま維持する。Out 1/Out 2とPA7の意味は別途実機確認する。

#### UART4 / PC11の限定受信診断（新しいFWでのみ利用可能）

通常のPro Mk3ビルドに追加した専用SysEx `F0 7D 43 46 57 0E 01 33 F7`は、**要求を受けたときだけ**PC11をAF8・プルアップにし、UART4をAPB1=54MHzから計算した31250bps・8N1・RXのみで40msポーリングする。既にUART4のクロックがONなら周辺回路に触れず`status=1`で拒否。正常終了時はUART4を無効にし、PC11の元のモード・AF・プル設定とUART4/GPIOCクロックの有効状態を戻す。UART4 TX（PA0）、UART7（PE8）、PA7、UART5/M0、USB設定、フラッシュには触れない。40msの間は既存のGPIO診断と同様にメインループの通常処理が遅れる。実機ではまだ未試験であり、**DIN→USB転送機能ではない**。

応答は`F0 7D 43 46 57 0E 01 73 [status] [count] [FE] [ORE] [hi,lo]×count F7`。`status=0`は収集完了、`1`はUART4使用中、`2`は32バイトの収集上限超過。`FE`と`ORE`は各最大127、`hi/lo`は受信バイトの上位／下位4bitで、MIDIステータスの最上位ビットも失わない。FE/OREがある測定のバイトは誤りを含む可能性がある。専用ページ[`tools/pro_mk3_uart4_rx.html`](tools/pro_mk3_uart4_rx.html)はUSB MIDIポートだけに診断要求を出し、外部サステイン機器がMIDI Inへ送るバイトを表示する。**旧診断FWは`0x33`に応答しない。** 旧`0x30`～`0x32`は変更していない。実機へのFW書込みはユーザーのみ。ビルドのリビジョンとPro Mk3用ZIPを確認し、復旧用FWを残してから実施する。

## ビルドと制約

通常のPro Mk3ビルド（追加feature不要）:

```
cargo build --offline --locked --release --bin core --target thumbv7em-none-eabihf --no-default-features --features launchpad-pro-mk3
```

`CORE_GUIDE_SYSEX_ID`を指定しているビルドは上記の`7D`をそのIDに置き換える必要がある。通常Proビルド#8（`4f9f7f8`）にはまだ含まれず、新しいコミットのTeamCity成果物に含まれる。XとProで共通のガイド応答には新コマンドを追加しない。実験用`0x30`はProだけで通常コマンドに優先して処理する。

以前の試作用`build/pro-din-probe.syx`はgit管理外のローカルファイルであり、**今回のTeamCity成果物ではない**。必ず新しいTeamCityビルドIDとリビジョンを確認する。

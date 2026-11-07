# Launchpad X / Pro Mk3 ヘッドレスエミュレータ計画

**更新:** `xtask`のX／Pro Mk3機種別featureでFW実`AppHost`を接続。パッド・MIDI・SysEx・1ms tick、Setup／DAWを含むモード遷移・回転とポート判定を実コードへ送る。以下の「第1段階」の記述は実装当時の記録。USBパケット／キュー、ADC、M0専用Roadrunner SysEx、設定永続化は対象外。直接Switchは診断用ショートカットで、物理ボタン検証時はSurface入力を使う。

## 目的・立ち位置

OpenCodeが実機の代わりに**FWの実コード**へパッド操作、MIDI、SysEx、経過時間を入力し、USB MIDI出力とLED状態を機械的に照合できるようにする。画面の見た目も8×8＋外周ボタンのスナップショットで確認する。最初からSTM32の全周辺機器や実機USBを模倣しない。デバイス依存の電気的特性は再現しないと明記し、最後の実機確認を少数残す。

対象はX（STM32F401RE）とPro Mk3（STM32F745VE）のNote／Custom／Setupおよびガイド。既存の`xtask`ホストテストが実際の`src/app/note/mod.rs`、`pro_note/mod.rs`、`custom_note/mod.rs`、`x_custom.rs`を読み込み、MIDIとLEDをスタブへ記録する仕組みを拡張する。アプリを模写した「別実装」を正本にしない。

## 現状の接続点（調査済み）

- `src/app/host.rs`: `AppHost`がモード切替・パッド・MIDI・SysEx・tickを一元的にルーティングする。XはNote/`XCustom`、ProはNote/`CustomNote`。SysExは演奏用`MidiPort::Midi`でのみガイドへ届く。
- `src/sys/driver/launchpad-x/mod.rs`: 1msループでMIDI RX→tick→パッド／Aftertouch→SysEx RX。Proの`src/sys/driver/launchpad-pro-mk3/mod.rs`はMIDI RX→tick→M0入力→SysEx RX（LED更新を間に挟む）。順序差はシナリオ結果に影響するので、機種別ランナーで維持する。
- `src/sys/driver/common/usb/midi.rs`: USB-MIDI 4バイトパケット→MIDI／SysExの純粋な解析部分がある。SysEx受信の最大長600、TX上限32パケット。キュー満杯や断片化はアプリ単体テストでは見落とす。
- `xtask/src/app/mod.rs`は実アプリと`AppHost`をホストに取り込み、`xtask/src/driver.rs`と`xtask/src/sys.rs`は送信MIDIとLED更新を記録する。USBパケットとCLIのシナリオ駆動はまだない。
- Xの圧力閾値・ヒステリシスは`src/sys/driver/launchpad-x/inputs.rs`、ProのM0フレーム／解放判定は`src/sys/driver/launchpad-pro-mk3/input_filter.rs`。通常の`SurfaceEvent`注入では物理センサーの再現までは保証しない。

## 着手済みの最小プロトタイプ

`xtask/src/app/emulator.rs`にテスト専用の`Emulator`を追加。FW本体と同じX／Pro Mk3のNote／Custom Appへ物理パッド番号・ポート別SysEx・仮想tick・モード切替を入力し、`xtask/src/sys.rs`のLED最終色（RGB/パレットを区別）と変更履歴、`xtask/src/driver.rs`の送信ポート付きMIDIを比較する。実際のAppモジュールを`#[path]`で読み込んでいるので、App内部の挙動のコピーではない。既存ホストテストと合わせて`cargo test --offline --locked -p xtask --quiet`で**70件成功**（2026-11-06、FW側で並行変更がある作業ツリー）。同音重複パッドのNote Off、X/Pro双方のGET_LAYOUTの長さ、DAWポート／他モデルの拒否、5秒lease、モード退出をシナリオテストで確認した。

**当時の限界（AppHostは接続済み）:** USBパケット／RXキュー・LED走査・実際のセンサーは走らせていない。直接Switchは診断ショートカット、実ボタン操作は`AppHost`で処理。CLI/JSONシナリオはまだ未実装。

## 段階的実装

### 1. AppHostを使う機種別ヘッドレスランナー（最優先）

FWの`AppHost`と`App`を同じソースからホストでコンパイルできるよう、ハードウェア参照を小さなdriver/settings/rotation/LED境界へ隔離する。XとProは`--features`等で**別々にビルド**し、`#[cfg]`差も試験する。ダミーの実装を増やして重要なルーティングを迂回しない。`xtask`の既存App単体テストは残す。

入力: 仮想時刻（tickをN回進める）、物理パッドのpress/release/aftertouch、MIDIポート別のNote/CC、SysExフレーム、切断／再接続に相当するモード退出。出力: 送信先ポート付きMIDI/SysExバイト列、最終LED RGB/パレットと変更履歴、現在モード・選択Custom・演奏中ノート。押下イベントは物理番号→回転後の論理番号変換を`AppHost`に任せる。

使い捨てCLI例（**予定、現時点では未実装**）:

```text
cargo run -p xtask -- emulate --device x --scenario tests/scenarios/x-guide-pad.json --json
cargo run -p xtask -- emulate --device pro-mk3 --scenario tests/scenarios/pro-guide-pad.json --json
```

固定入力に対し順序の安定したJSONを返し、失敗時に期待値と差分・直前イベントを表示する。CLIのメタデータに論理ID・用途・版を載せ、イベントは必要ならCloudEvents 1.0形式で構造化して出す。観測のための常駐HTTPサーバーは作らず、認証情報やセッション本文は記録しない。IOPの詳細は配備先に正本があるときだけ参照する。

### 2. USB-MIDI／実時間／圧力の境界テスト

USB 4バイトパケットの断片化、複数ケーブル（DAWと演奏用）、QUERY/GET_LAYOUTの85/89バイト応答、5秒lease、SET連打時のRX/TXキュー限界を実際の共通USB codec/queueロジックで検証する。機種別1msループの入力順序を再現し、時計はsleepしない仮想時間とする。

Xの圧力値→press/release/velocity、ProのM0フレーム→press/release/velocityは、ハードウェアレジスタ操作を含まない計算部分だけを抽出して単体検証する。ADC DMA・M0通信の実機タイミングを推測値で「検証済み」とは扱わない。

### 3. 表示・回帰テストとAI向け可視化

最終LEDを8×8＋外周のテキスト表とJSONで表示し、任意で静的HTML/SVGに描画。CIの成果物や公開サービスは不要。色の値とパレット番号を別フィールドに保持し、`clear`後の更新・ちらつき（全画面更新回数）もアサートできるようにする。golden画像だけで合否を決めず、MIDIとSysExのバイト列・ポート・状態も検証する。

最初のシナリオ:

1. X/ProのNote・Custom切替、押下→同音重複押下→解放、Note On/Offの対応、Proの選択チャンネル。
2. 正しい機種IDと間違った機種ID、DAWと演奏用ポート、BEGIN/QUERY/GET_LAYOUT/SET/KEEPALIVE/END、期限切れ、無効7bit/長さ。
3. `GET_LAYOUT`の64スロットと物理パッド番号の対応、Xの4 Custom配列・移調、Pro Note/Customの無効パッド。
4. 音程指定とパッド指定のガイド競合、held表示、重複ピッチから単一パッド選択、配列変更時の旧色消去、モード退出時のリセット。
5. Velocity編集（タップ／130ms内のドラッグ／削除／最低値変更）後に設定保存・再起動し、Note On velocityが単調で1または選択下限～127に収まること。

## 合格基準・実機確認の線引き

- 同じシナリオを毎回再実行でき、ホストテスト＋X/Pro releaseリンクが通る。GET_LAYOUT、MIDI出力、LEDの**最終状態と変更履歴**が期待値に一致する。演奏用ポート以外へガイド応答しない。
- パッド押下からLED更新までのtick数、モード切替でのNote Off漏れ、不要な全画面clear回数を数値で比較できる。
- ファームウェア実コードへの参照が切れた場合にテストが失敗する。実機のFWバイナリをホストで直接実行したものと誤認させない。
- 実機でなお必要: USBデバイスの列挙・ホストOSのポート名、XのFSRの個体差と圧力感、ProのM0/LED通信と表示色・残像、書込み／ブート／Flash容量。変更の種類ごとにX・Proを最低1回だけ目視／MIDI監視する。エミュレータはこれらの完全な代替ではない。

## 作業上の分離

まずホスト側の新規テストランナーとスタブの拡張だけで着手する。並行作業中の`src/app/guide.rs`や各Note/CustomのFW実装は上書きしない。アプリ境界の抽出が必要な場合は差分を小さくし、担当するFW変更が落ち着いてから統合する。実機書込み・外部公開・pushは別の依頼で行う。

## 2026-11-06 ブラウザ操作の第1段階

`xtask/src/bin/emulator-web.rs`と`xtask/emulator-ui/`を追加。テスト専用だった同じ`Emulator`を通常の`xtask`バイナリから呼び出し、ループバック専用HTTPでFW Appの実出力（100個のLED状態、ポート別MIDI/SysEx、LED更新回数）を表示する。8×8と外周パッドのpress/release、Note/Customの簡易切替、MIDIとSysExの送信、ガイド操作、最大10秒の仮想tickを操作できる。起動方法と未再現範囲は`xtask/emulator-ui/README.md`。`cargo test --offline --locked -p xtask`で既存70件＋ブラウザ用バイナリ61件成功、通常ビルドも成功。これでブラウザから操作できるが、上記第1段階の**AppHost接続は未着手**。画面遷移・Setup・物理入力とUSB経路は次に接続する必要がある。

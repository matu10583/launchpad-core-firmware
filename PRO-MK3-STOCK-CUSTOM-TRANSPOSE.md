# Launchpad Pro MK3: 固定Custom配列＋Note Mode 実装リスト

**対象はPro MK3実機。このブランチには固定CustomモードのUSB MIDI試作があるが、純正互換ファームウェアやDIN対応版ではない。実機書き込み・動作確認は未実施。** 上流 `code` の `3f5e3538973df842fb53fca83c8caec177467be7` を基点とし、Launchpad X用の感度試作やNote Modeは取り込まない。

### 実装済みの範囲（試作）

- 固定Customノート配列・ド＝ピンク／音階内＝青／音階外＝暗色、USB MIDIのベロシティとAftertouch、同音ノートの重複押下、押下時音程を保持するNote Off、受信USBノートの同音パッド点灯。チャンネルは初期値1、Shift＋Customから下2行で1～16を選択可能（保存は未実装）。
- Custom物理ボタン（96）または既存Setupの14番パッドから選択可能。Custom中はSessionボタン（93）で既存DAWモードへ戻る。移調は上（80）下（70）・左（91）右（92）、各ペア同時押しでその方向だけリセット。モード退出時は演奏中のノートを停止する。
- Noteボタン（94）で独立したNote Modeに入り、Shift＋Noteで設定画面を開く。Chromatic／Scale、16スケール、ルート、重複配置（sequential／2～5 finger）、MIDIチャンネル1～16、USBノート／Aftertouch、モード退出時のNote Offを実装。既存Setupの24番パッドからも選択できる。**これは純正の全挙動を再現したものではない**（スケールの個別音程や配置の実機照合、設定保存、ドラムレイアウト・Ableton自動切替は未実装／未検証）。
- **未実装**：DIN入出力、その他の純正モード・設定。現在の起動時動作も純正Liveとは異なる。現行のUSB送信ドライバをDIN送信として扱わないこと。`.github/workflows/pro-mk3-check.yml` は該当ブランチのGitHub-hostedランナーでPro MK3だけをビルドし、成功した場合のみ検証前の実験用`.syx`をActions artifactとして保存する。Release公開・自動書き込みはしない。

## 参照資料

- [Novation公式ユーザーガイド（Web版 Ver. 6.0）](https://userguides.novationmusic.com/hc/en-gb/sections/25494488182418-Launchpad-Pro-MK3-User-Guide)：[各モード、Note/Custom](https://userguides.novationmusic.com/hc/en-gb/articles/25494530115346-Launchpad-Pro-MK3-interface)、[Setup](https://userguides.novationmusic.com/hc/en-gb/articles/25494545308306-Using-the-Launchpad-Pro-MK3-s-Setup-page)、[Sequencer](https://userguides.novationmusic.com/hc/en-gb/articles/25494505907346-Using-Launchpad-Pro-MK3-s-Sequencer)、[MIDI配置付録](https://userguides.novationmusic.com/hc/en-gb/articles/25494488560914-Launchpad-Pro-MK3-appendix)。
- [公式Programmer’s Reference](https://fael-downloads-prod.focusrite.com/customer/prod/s3fs-public/downloads/LPP3_prog_ref_guide_200415.pdf)：公開MIDI/SysEx、DAW、LEDの仕様。ユーザーガイドにはない詳細を別途照合する必要がある。
- [公式ダウンロード一覧](https://downloads.novationmusic.com/novation/launchpad-mk3/launchpad-pro-mk3-0)：日本語ガイド、1.2アップデート追加ガイド。対応する純正版との差は実機で確認する。

## 今回の実装対象（優先順）

1. **共通基盤**：Pro MK3の全パッド／矢印／モードボタン／Shift／Setupの物理マップ、演奏中のモード変更時のNote Off、安全な起動・USB MIDIチャンネルとLED更新。CoreFWの既存Performance・Programmer・DAW機能との共存。純正は起動時Live、現在のCoreFWはPerformanceなので起動動作の変更も必要（未対応）。
2. **固定Custom Modeを1つ**：提示画像のLED配色と、既存X試作版 `launchpad-x-note-mode-v3` のChromatic配列を採用することでユーザー確認済み。左下MIDI 48、左4列は横1半音・上の行＋4半音、右4列は左の同じ行＋24半音（左上2行＝右下2行）。ド＝ピンク・音階内＝青系・音階外＝暗色。まずMIDIチャンネル1を既定とし、純正同様のチャンネル設定を別途確認する。Components転送、編集UI、8スロットやCC/Program Change/フェーダーは**今回不要**。押下中は同音程の全パッドを点灯し、重複ノートの最後の指を離したときにNote Off。移調はCustomのNote型パッドにだけ適用し、上下＝オクターブ、左右＝半音、各ペア同時押し＝その方向の移調だけリセット。Note On時に送った音程を保持してNote Offを出す。MIDI 0–127の範囲外は発音させない。
3. **純正に近いNote Mode**：Chromatic（純正既定）、Scale、ドラムのノート配置とLED、上下オクターブ・左右半音、16スケールとルート選択、行の重複設定、チャンネル1–16、Shift＋Noteの設定画面、ベロシティ/Aftertouch。Chromaticの純正初期表示はC minorで、ルートは紫、スケール内は青。**Customとは移調状態を共有せず**、Note側の操作は純正に近づける。Chromatic／Scaleの基本実装は追加済み。ドラムとAbleton Drum Rackの自動判定はDAW連携が必要なので後回し。
4. **DIN MIDI（必須）**：物理MIDI In、Out 1、Out 2/Thruの切替。純正のUSB MIDIポートとの送受信ルーティング、MIDIチャンネル・クロックRx/Tx、Thruの転送、ループ防止を確認する。通常演奏とCustom移調は送信先ごとに同じ音程と対応するNote Offが届く必要がある。DIN Inからの音もNote/Customの状態と競合しないよう扱う。
5. **共通設定と検証**：Setup内のLED明るさ、Velocity on/offと曲線・押下閾値、Aftertouch、USB MIDI/LED、DINの基本動作、保存・再起動時の状態。ビルドとホストテストはGitHub-hosted Actionsのみ。実機書き込みは自動化しない。

### MIDI DIN端子: **現状未対応・今回の必須条件**

純正Pro MK3にはUSBのDAW/MIDIインターフェースと**物理DIN In、Out 1、Out 2/Thru**がある。公式Programmer’s Referenceは純正USBのMIDI、DIN、DAWの3インターフェースを区別するが、現行 `src/sys/driver/launchpad-pro-mk3/usb.rs` はDAW/MIDIの2ポートしか宣言していない。書き換えても端子がそのまま機能するとは限らない。現行CoreFWの `src/sys/driver/launchpad-pro-mk3/runtime.rs` の `send_midi` はUSB送信キューにのみ渡す。`mod.rs` はUSBイベントを読む一方、UART5は内部M0のボタン・LED用であり、DINの送受信・Thruの初期化は見つからない。`MidiPort::Din` というenumはあるが、物理DIN経路の実装を意味しない。**現行コードからはDIN動作を保証できない。配線/内部M0の役割の確認、DIN In/Out 1/Out 2/Thru実装、USBとDINの重複送信防止、および実機ループバック試験を必須とする。** 実装前の試作をDIN対応版として配布しない。

## 後回し（純正同等を名乗るまでに残る差分）

| 領域 | 必要な機能 / 現状 |
| --- | --- |
| Ableton Session | クリップ/シーン表示と起動、ミキサー、録音、Clip操作、Print to Clip等。既存DAWアプリはあるが完全一致未検証。 |
| Chord | Triads、Note Area、14スロットのChord Bank、Lock、Sustain、Noteとスケール共有。未実装。 |
| Sequencer / Projects | 4トラック×32ステップ、各8パターン、シーン、Gate/Probability/Mutation/Micro Steps、クロック、録音、64プロジェクト永続化。未実装。 |
| 完全な純正Setup/Legacy/Components | Live/Programmer/Legacy切替、すべてのSetup設定、Componentsによる編集/バックアップ。今回の固定Custom実装にComponents互換は含めない。 |

**未確定事項**：純正Pro MK3のMIDI入出力ハードウェア配線と内部M0側のDIN担当範囲、USB/DINの正確な純正ルーティング、CustomのMIDIチャンネル設定をどの純正画面に合わせるか。配列の音程はX版と同一で確定。

**配布条件**：Pro MK3向けActionsでのビルド成功はUSB・DIN・純正UIの実機動作の保証ではない。USB音源のPanic/全ノート停止、純正復旧手順を確保し、Pro MK3の実機で基本演奏・Note Off・モード変更・USBとDINを別々に確認するまでは「純正と同じ」と表現しない。

## 実験用ファームウェアを入れる前に

- この作業ツリーの変更は未pushで、GitHub Actionsはまだ動いていない。現時点でこの変更を含む書き込み用ファイルは存在しない。上流のReleaseや公式CoreFW Webインストーラーのファイルはこの試作とは別物。
- 上流サイトはCoreFW 1.2.1への更新とPro MK3のシリアル番号確認・修復を強く推奨している。このブランチは1.2.0基点なので、書き込み前に[上流の注意](https://fw.anthonyhfm.dev/)と[修復ページ](https://fw.anthonyhfm.dev/repair)を確認し、復旧経路を確保する。
- 自分のforkのこのブランチへ変更をpushして `.github/workflows/pro-mk3-check.yml` の成功を確認したら、実行結果の `pro-mk3-experimental-UNVERIFIED` artifactから `core-launchpad-pro-mk3.syx` を取得し、同梱の `SHA256SUMS.txt` と照合する。Actions成功は実機安全性の保証ではない。上流の `Release Firmware` は全機種Release公開・タグ作成を行うため本件では使わない。
- 書き込む場合はWindows PCでUSB接続を外し、**Pro MK3本体のSetupボタンを押しながら**接続してブートローダーメニューに入る（[Novation公式Setupガイド](https://userguides.novationmusic.com/hc/en-gb/articles/25494545308306-Using-the-Launchpad-Pro-MK3-s-Setup-page#bootloader-menu)）。上流 `readme.md` のCapture MIDI長押しは機種一般向けの記述であり、Pro MK3では採用しない。ブートローダーのMIDI出力ポートを確認してから、ダウンロードしたこの `.syx` を `tools/flash.py` などのSysEx送信ツールで送る。DAW/MIDI演奏ポートや他機種用ファイルを選択しない。WindowsでPythonツールを使う場合は `python -m pip install mido python-rtmidi`、`python tools/flash.py <ダウンロードしたsyxのパス>`。ローカルRustビルドは不要。
- 公式ファームウェアへの復旧手順・シリアル番号・DIN端子の動作は未検証。**それらを確認できない場合はまだ書き込まない。**

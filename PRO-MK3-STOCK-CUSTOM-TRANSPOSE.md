# Launchpad Pro MK3: 固定Custom配列＋Note Mode 実装リスト

**対象はPro MK3実機。このブランチは仕様・差分整理段階で、書き込み用ファームウェアは未作成。** 上流 `code` の `3f5e3538973df842fb53fca83c8caec177467be7` を基点とし、Launchpad X用の感度試作やNote Modeは取り込まない。

## 参照資料

- [Novation公式ユーザーガイド（Web版 Ver. 6.0）](https://userguides.novationmusic.com/hc/en-gb/sections/25494488182418-Launchpad-Pro-MK3-User-Guide)：[各モード、Note/Custom](https://userguides.novationmusic.com/hc/en-gb/articles/25494530115346-Launchpad-Pro-MK3-interface)、[Setup](https://userguides.novationmusic.com/hc/en-gb/articles/25494545308306-Using-the-Launchpad-Pro-MK3-s-Setup-page)、[Sequencer](https://userguides.novationmusic.com/hc/en-gb/articles/25494505907346-Using-Launchpad-Pro-MK3-s-Sequencer)、[MIDI配置付録](https://userguides.novationmusic.com/hc/en-gb/articles/25494488560914-Launchpad-Pro-MK3-appendix)。
- [公式Programmer’s Reference](https://fael-downloads-prod.focusrite.com/customer/prod/s3fs-public/downloads/LPP3_prog_ref_guide_200415.pdf)：公開MIDI/SysEx、DAW、LEDの仕様。ユーザーガイドにはない詳細を別途照合する必要がある。
- [公式ダウンロード一覧](https://downloads.novationmusic.com/novation/launchpad-mk3/launchpad-pro-mk3-0)：日本語ガイド、1.2アップデート追加ガイド。対応する純正版との差は実機で確認する。

## 今回の実装対象（優先順）

1. **共通基盤**：Pro MK3の全パッド／矢印／モードボタン／Shift／Setupの物理マップ、演奏中のモード変更時のNote Off、安全な起動・USB MIDIチャンネルとLED更新。CoreFWの既存Performance・Programmer・DAW機能との共存。純正は起動時Live、現在のCoreFWはPerformanceなので起動動作の変更も必要。
2. **固定Custom Modeを1つ**：提示画像のLED配色と、既存X試作版 `launchpad-x-note-mode-v3` のChromatic配列を採用することでユーザー確認済み。左下MIDI 48、左4列は横1半音・上の行＋4半音、右4列は左の同じ行＋24半音（左上2行＝右下2行）。ド＝ピンク・音階内＝青系・音階外＝暗色。まずMIDIチャンネル1を既定とし、純正同様のチャンネル設定を別途確認する。Components転送、編集UI、8スロットやCC/Program Change/フェーダーは**今回不要**。押下中は同音程の全パッドを点灯し、重複ノートの最後の指を離したときにNote Off。移調はCustomのNote型パッドにだけ適用し、上下＝オクターブ、左右＝半音、各ペア同時押し＝その方向の移調だけリセット。Note On時に送った音程を保持してNote Offを出す。MIDI 0–127の範囲外は発音させない。
3. **純正に近いNote Mode**：Chromatic（純正既定）、Scale、ドラムのノート配置とLED、上下オクターブ・左右半音、16スケールとルート選択、行の重複設定、チャンネル1–16、Shift＋Noteの設定画面、ベロシティ/Aftertouch。Chromaticの純正初期表示はC minorで、ルートは紫、スケール内は青。**Customとは移調状態を共有せず**、Note側の操作は純正に近づける。Ableton Drum Rackの自動判定はDAW連携が必要なので後回し。
4. **DIN MIDI（必須）**：物理MIDI In、Out 1、Out 2/Thruの切替。純正のUSB MIDIポートとの送受信ルーティング、MIDIチャンネル・クロックRx/Tx、Thruの転送、ループ防止を確認する。通常演奏とCustom移調は送信先ごとに同じ音程と対応するNote Offが届く必要がある。DIN Inからの音もNote/Customの状態と競合しないよう扱う。
5. **共通設定と検証**：Setup内のLED明るさ、Velocity on/offと曲線・押下閾値、Aftertouch、USB MIDI/LED、DINの基本動作、保存・再起動時の状態。ビルドとホストテストはGitHub-hosted Actionsのみ。実機書き込みは自動化しない。

### MIDI DIN端子: **現状未対応・今回の必須条件**

純正Pro MK3にはUSBのDAW/MIDIインターフェースと**物理DIN In、Out 1、Out 2/Thru**がある。書き換えても端子がそのまま機能するとは限らない。現行CoreFWの `src/sys/driver/launchpad-pro-mk3/runtime.rs` の `send_midi` はUSB送信キューにのみ渡す。`mod.rs` はUSBイベントを読む一方、UART5は内部M0のボタン・LED用であり、DINの送受信・Thruの初期化は見つからない。`MidiPort::Din` というenumはあるが、物理DIN経路の実装を意味しない。**現行コードからはDIN動作を保証できない。配線/内部M0の役割の確認、DIN In/Out 1/Out 2/Thru実装、USBとDINの重複送信防止、および実機ループバック試験を必須とする。** 実装前の試作をDIN対応版として配布しない。

## 後回し（純正同等を名乗るまでに残る差分）

| 領域 | 必要な機能 / 現状 |
| --- | --- |
| Ableton Session | クリップ/シーン表示と起動、ミキサー、録音、Clip操作、Print to Clip等。既存DAWアプリはあるが完全一致未検証。 |
| Chord | Triads、Note Area、14スロットのChord Bank、Lock、Sustain、Noteとスケール共有。未実装。 |
| Sequencer / Projects | 4トラック×32ステップ、各8パターン、シーン、Gate/Probability/Mutation/Micro Steps、クロック、録音、64プロジェクト永続化。未実装。 |
| 完全な純正Setup/Legacy/Components | Live/Programmer/Legacy切替、すべてのSetup設定、Componentsによる編集/バックアップ。今回の固定Custom実装にComponents互換は含めない。 |

**未確定事項**：純正Pro MK3のMIDI入出力ハードウェア配線と内部M0側のDIN担当範囲、USB/DINの正確な純正ルーティング、CustomのMIDIチャンネル設定をどの純正画面に合わせるか。配列の音程はX版と同一で確定。

**配布条件**：Pro MK3向けActionsでのビルド成功はUSB・DIN・純正UIの実機動作の保証ではない。USB音源のPanic/全ノート停止、純正復旧手順を確保し、Pro MK3の実機で基本演奏・Note Off・モード変更・USBとDINを別々に確認するまでは「純正と同じ」と表現しない。

# Launchpad Pro MK3 純正互換＋Custom Note移調: 仕様と実装状況

**対象はPro MK3実機。これは実装計画であり、完成したファームウェアではない。** このブランチは上流 `code` の `3f5e3538973df842fb53fca83c8caec177467be7` を起点にしており、Launchpad X向け試作の感度値・Note Mode・操作は取り込まない。Pro MK3への書き込みは、純正互換性・復旧方法が確認できるまで行わない。

## 参照する公式仕様

- [Pro MK3公式ユーザーガイド（Ver. 6.0、Web版）](https://userguides.novationmusic.com/hc/en-gb/sections/25494488182418-Launchpad-Pro-MK3-User-Guide)：導入、ハードウェア、[Session/Note/Chord/Custom](https://userguides.novationmusic.com/hc/en-gb/articles/25494530115346-Launchpad-Pro-MK3-interface)、[Sequencer/Projects](https://userguides.novationmusic.com/hc/en-gb/articles/25494505907346-Using-Launchpad-Pro-MK3-s-Sequencer)、[Setup](https://userguides.novationmusic.com/hc/en-gb/articles/25494545308306-Using-the-Launchpad-Pro-MK3-s-Setup-page)、[付録のMIDI配置](https://userguides.novationmusic.com/hc/en-gb/articles/25494488560914-Launchpad-Pro-MK3-appendix)。
- [公式Programmer’s Reference](https://fael-downloads-prod.focusrite.com/customer/prod/s3fs-public/downloads/LPP3_prog_ref_guide_200415.pdf)：3つのUSB MIDIインターフェース、SysExレイアウト切替、LED、DAW、Programmerの公開プロトコル。これは**純正のアプリケーションファームウェア全体のソースやComponentsの保存形式ではない**。
- [Novationのダウンロード一覧](https://downloads.novationmusic.com/novation/launchpad-mk3/launchpad-pro-mk3-0)には日本語版ユーザーガイドとファームウェア1.2追加ガイドもある。バージョン差分まで検証対象とする。

## 純正動作と現行CoreFWの差分

| 領域 | 純正に必要な動作 | 現行CoreFWの状況 / 判定 |
| --- | --- | --- |
| デバイス基盤 | RGB、圧力入力、USB MIDI 3インターフェース、MIDI In/Out1/Out2・Thru、クロック設定 | Pro MK3向けドライバとUSB/LEDはある。DIN・クロック・設定の純正同等性は未確認。 |
| Session / Ableton Live | 自動認識、クリップ/シーン、録音、ミキサー、トランスポート、Clip操作、Print to Clip | DAW/Sessionとフェーダーは存在。ただしLive上の全操作・表示・Print to Clip一致は未検証。 |
| Note / Drum | Chromatic/Scale/Drum切替、16スケール、ルート、重複幅、MIDI ch、Live Drum Rack自動追従 | 純正に相当するNote/Scale/Drumアプリは未実装。既存Performanceは代替ではない。 |
| Chord | Triads/Note Area、14スロットのChord Bank、Lock、Sustain、Noteとスケール設定共有 | 未実装。 |
| Custom 1–8 | Components編集・バックアップ、ノート/CC/Program Change/フェーダー、トグル/トリガー/モーメンタリ、個別Master ch、外部MIDI LED | 未実装。CoreFWの「Custom Palette」は**Custom Modeとは無関係**。Componentsとの編集・保存・読み戻し互換は別途解析・検証が必要。 |
| 4トラックSequencer | 32ステップ、各8パターン、16シーン、チェイン、Gate/Velocity/Probability/Mutation/Micro Steps、テンポ/スウィング、MIDIクロック、Live録音と外部MIDI録音 | 未実装。 |
| Projects | 64保存先、パターン/シーン/音階/チャンネル/Custom Master chの永続化とComponentsバックアップ | 未実装。フラッシュ割当・電源断時の整合性・バックアップ形式に設計が必要。 |
| Setup/Legacy/Programmer | LED/Velocity/Aftertouch/MIDI/Fader各設定、Live/Programmer/Legacy、対応SysEx | CoreFW固有SetupとProgrammerはあるが、純正の全ページ・Legacy・起動時Live動作は一致しない。 |
| 起動/復旧 | 純正ブートローダー更新、MSD、ID、復旧 | ブートローダー/Componentsの内部仕様はアプリの公開資料だけでは再現できない。純正への復旧を実機で検証するまで配布しない。 |

## 唯一の追加機能: Custom ModeのNote型パッド移調（設計、未実装）

- 8つのCustom Modeごとに移調量を別管理する。初期値0。元のパッド定義（音程、チャンネル、色、モーメンタリ/トグル等）を変更・破壊せず、**MIDI Note On/Offと対象ノートのAftertouchだけ**発音先へ同じ量を加算する。CC、Program Change、フェーダー、Sequencer、Note/Chord、DAW、外部MIDI Thruは移調しない。
- ボタン割当は純正操作と衝突させない。Customモードの選択・Master ch設定・Setup操作を優先し、実機の入力マップと純正のボタン組み合わせを照合して決める。押下中に移調・Custom切替・モード退出するときは、**押したときに送ったチャンネルと音程を記録した上で対応するNote Offを送信**する。範囲外の0..127は発音させない。
- MIDI In/Out/USBの全経路でノートが二重に変換されないこと、LEDと受信ノートの整合性、Componentsで転送した定義の保持をテストする。純正Custom設定の保存形式が分からない段階では、Components互換を主張しない。

## リリース条件

1. 上記すべての項目について純正ガイド・公開MIDI仕様と照合した自動テストを作る。公開資料だけでは判断できない箇所は**未確認**と表示する。
2. GitHub-hosted ActionsのみでPro MK3向けビルドとテストを実行する。X向けの成果物と混同しない。
3. Pro MK3実機で通常演奏・USB/DIN・DAW・Components編集・Sequencer保存/再起動を確認するまでは「純正と同じ」と呼ばず、実機書き込みを推奨しない。ノート鳴りっぱなしへのPanicと純正復旧経路を用意する。

**現時点では上記の大部分が未実装であり、一括の試作イメージや安全に書き込める成果物は作成していない。** 取扱説明書に記述のないComponents内部仕様やAbleton側との連携を、読んだだけで保証することはできない。

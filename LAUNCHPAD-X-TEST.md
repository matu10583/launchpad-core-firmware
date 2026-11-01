# Launchpad X 感度比較（実機未検証）

対象は **Launchpad X**。Launchpad Pro / Pro MK3 向けではありません。比較基準は上流 `code` の `3f5e3538973df842fb53fca83c8caec177467be7`。CoreFW は純正ファームウェアのパッチではなく**置き換え**です。純正での「軽く押すと鳴らない／保持中に消える」の原因は MIDI Note Off の実測で確定しておらず、CoreFW でも再現するか不明です。

## 取得と安全確認

GitHub の自分の fork → **Actions** → `Launchpad X firmware artifact` → 成功した実行 → **Artifacts** から ZIP をブラウザで取得。ZIP 内の `core-launchpad-x.syx`、`core-launchpad-x.manifest.json`、`SHA256SUMS.txt`、`説明.txt` を確認する。ZIP のチェックサムをファイル内容のチェックサムと混同しない。`SHA256SUMS.txt` は ZIP 内の syx と manifest に対する値。baseline と試作版はそれぞれの実行から保存しておく（保存期間14日）。CI 成功は実機動作の証明ではない。

workflow は専用ブランチへの push と `workflow_dispatch` に対応する。ただし GitHub の手動起動は **fork の既定ブランチにも workflow ファイルが存在する場合のみ**表示・起動できる。専用ブランチに追加するだけでは手動起動はできない。既定ブランチに workflow を導入しない状態では push 起動を使うこと（上流の公開 Release workflow は実行しない）。

**書き込む前に** Novation Components で自分の Launchpad X が通常モードで認識されること、設定・Custom Modes 等を退避できることを確認する。USBを外し **Capture MIDI** を押しながら接続して、ブートローダーもPCに認識されることを確認する。認識しない場合は書き込みを中止。純正に戻す経路として Novation 公式の [Components による更新・再インストール手順](https://support.novationmusic.com/hc/en-gb/articles/360002211360-Updating-firmware-using-Novation-Components) を事前に読んでおく。公式手順は Launchpad X のブートローダー起動に Capture MIDI を指定し、Components の Updates 画面（またはホーム画面）の再インストールを案内している。Components からの認識・復旧は状況や機器に依存し、**必ず戻せる保証はない**。

自己責任で PC から書き込む場合は、対象機種とブートローダーの MIDI ポートを再確認し、取得した ZIP の **X 用** `.syx` のみを、信頼できる SysEx 送信ツールでブートローダーの MIDI ポートへ送る。上流 [readme の手動書き込み節](readme.md#flashing-the-firmware-manually) も参照。ここでは書き込みコマンドを実行しない。書き込み後に失敗した場合は、まず公式 Components の再インストール手順に従い、復旧できなければ Novation サポートに相談する。ブートローダー自体やメモリマップは変更しない。

## モードと MIDI

上流 readme のロードマップには Note Mode、Chord Mode、Components 経由の Custom Modes が残っている。したがって純正の音階配置や機能が維持されるとは想定しない。比較には CoreFW の **Performance Mode** を使い、PC 側で MIDI 音源（チャンネル1）と MIDI モニターを接続する。コード上の経路は X の ADC/DMA → 3点中央値 → 正規化（raw ≤150 は0）→ 上昇時 1/2・下降時 1/8 のフィルタ → 無押下時の基準値追従 → `delta` の押下・解放ヒステリシス → `GridEvent` → `SurfaceEvent` → `PerformanceApp::on_surface` → `xy_to_dr` → MIDI `0x90, note, velocity`。解放は `0x90, note, 0`（Note On velocity 0 = Note Off 相当）であり、必ずしも `0x80` として表示されない。LED は受信 MIDI やモードの影響も受けるため LED だけで Note Off と断定しない。

内部の座標は左下の8×8パッドが `11`、右上が `88`。`src/utils/layout.rs` の Performance Mode の対応例は左下 `11` → MIDI note **36**、その右 `12` → **37**、右上 `88` → **99**。これは純正 Note Mode の音階ではない。音源・MIDIポート・チャンネル・ベロシティ設定を揃えて比較する。

LED確認版 `launchpad-x-led-feedback` は **baselineと同じ内部感度値**（開始300、解放144）で、Performance Modeの押下を受け付けたパッドを緑に点灯し、解放判定時に消灯する。音・MIDI送信処理は変えず、純正のNote Modeも追加しない。点灯は内部押下状態の表示であり、PCへのMIDI到達を保証しない。PCから同じパッドへLED用MIDIを送ると表示が上書きされ得るため、まずホスト側のLED送信を止めて確認する。点灯がない場合でも入力経路や明るさなど原因は確定しないので、可能ならMIDIログと併せて判断する。

`launchpad-x-hold-80-led` は LED確認版の解放閾値だけを **144 → 80** とした追加試作。以前の `112` でも保持中に Note On velocity 0 が観測されたため、保持側だけをもう一段試す。**80は実機の生データに基づく推奨値ではない**。押し始めの閾値300とベロシティ曲線は変わらず、軽い押下が認識されない問題は直らない。指を離したときの解放遅れ・鳴りっぱなし・放置時の誤発音を注意して比較する。なお、入力が実際にゼロ近くに落ちている場合はこの変更でも保持できず、測定なしに解放値をさらに下げるべきではない。

`launchpad-x-hold-48-led` は直前の保持80+LED版から**解放閾値のみ 80 → 48** とした追加試作。押下開始300とベロシティ曲線はそのまま。保持中の誤解放を減らせるか試すが、生データの実測に基づく推奨値ではない。解放後の遅延・鳴りっぱなしに注意し、問題があれば試験を中止する。押下開始の力は変わらない。保持中に正規化値がゼロへ落ちる場合はこの変更でも解放され得るため、さらに数値を下げ続ける前に入力経路を調べる。

`launchpad-x-hold-8-led` は保持48+LED版から**解放閾値だけ 48 → 8**に下げた極端な実験。押下開始300・フィルタ・カウント・ベロシティ曲線は不変。**実機測定に基づく推奨ではなく、常用前提ではない**。下降フィルタは入力が0、基準値も0でも `filt = (7*filt+4)>>3` が正の値4で止まり得るため、解放値0～3は指を離してもNote Offが出ない危険がある。8でも基準値との残差やノイズ次第で鳴りっぱなし・解放遅延の危険がある。PC音源の「全ノート停止／Panic」と旧版へ戻す経路を用意してから短時間だけ試し、**指を完全に離した後**に緑LEDが消え、MIDI Note On velocity 0が出ることを必ず確認。出ない場合は試験を中止し、音源側で全ノートを止める。開始の重さは変わらず、保持中に読み値が0付近まで落ちる原因は解決しない。

`launchpad-x-press-112-hold-8-led` は実機で保持／解放を確認した保持8+LED版から**押下開始値だけ 300 → 112** とした強い試作。解放値8、raw下限、フィルタ、カウント、holdoff、Aftertouch、LED動作は不変。押下開始値を参照していた `norm_to_velocity` は元の300を別定数に固定し、既存のベロシティ曲線を保つ（112～300で早めに押下成立した場合、ベロシティ有効時の値は1になり得る）。112は基準値追従のguard 96より大きいが近いため、**実機の生データに基づく推奨値ではなく、常用前提でもない**。無操作時の誤発音、隣のパッド、離した後のNote Offと鳴りっぱなしを重点的に確認。保持8の既存成果物は戻すために保存しておく。

## 変更対象と判定の限界

基準値は `LP_PRESS_START_NORM=300`、`LP_PRESS_RELEASE_NORM=144`、`LP_PRESS_ON_COUNT=1`、`LP_RELEASE_COUNT=8`、`LP_RELEASE_HOLDOFF=10`、`LP_BASELINE_GUARD=96`。これらは **CoreFW 内部値**で、純正設定・グラム・ミリ秒ではない。解放判定は `delta <= release` の連続8回かつ holdoff 終了後。押している間は基準値追従を止め、解放時に `grid_base=filt` とする。`LP_PRESS_START_NORM` はベロシティ計算にも使われるので、開始値を後で変更するなら曲線への影響を別途検討する。

baseline が CI 成功した後にのみ、別コミットの最初の試作として **解放閾値 144 → 112 のみ**を検討する。開始値 300 より小さいためヒステリシスの向きは保つが、実機測定による推奨値ではない。弱い保持で意図しない解放を減らせる可能性がある一方、押し始められない症状や raw 下限以下の検出は直せず、解放遅れ・鳴りっぱなしに注意。開始値、raw 下限、基準値、フィルタ、カウント、holdoff、Aftertouch を同時に変更しない。

## 同条件の実機記録表

押すパッド、音源とMIDIポート、設定、USB環境を固定し、順番を記録する。軽い押下・弱い保持・離す・連打を複数回試し、放置中の誤発音、隣接パッドの誤反応、鳴りっぱなしも見る。可能なら LED と同時に MIDI Note On / Note Off 相当（velocity 0 を含む）の時刻を残す。

| 版・ソースSHA | 条件／パッド | 軽い押下（発音回数/試行数） | 弱い保持（秒、消失の有無） | 離したとき（Off相当/遅れ） | 連打 | 放置時の誤発音 | 隣接誤反応 | 鳴りっぱなし | LED / MIDIログ・メモ |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 純正 / 版番号 | | | | | | | | | |
| CoreFW baseline / SHA | | | | | | | | | |
| CoreFW 試作版 / SHA | | | | | | | | | |

自宅 Linux ではビルド・テスト・ツールチェーンの導入や自動書き込みは行わない。Rust 拡張機能（rust-analyzer 等）の自動ビルド・自動導入にも注意し、コンパイルとテストは GitHub-hosted Actions のみで実施する。

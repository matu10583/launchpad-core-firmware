#!/usr/bin/env python3
"""Generate model-specific CoreFW manuals, requiring review on source changes.

Only mechanically verifiable statements are derived from Rust/Cargo. Curated
instructions are never silently rewritten when their source code changes.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "firmware"
LOCK = OUT / "source-review.json"

# Alias, feature, display name, driver folder. The alias/feature pairing is
# asserted against xtask's package command and .cargo/config.toml below.
DEVICES = {
    "lpx": ("launchpad-x", "Launchpad X", "launchpad-x"),
    "lppmk3": ("launchpad-pro-mk3", "Launchpad Pro Mk3", "launchpad-pro-mk3"),
    "mini": ("launchpad-mini-mk3", "Launchpad Mini Mk3", "launchpad-mini-mk3"),
    "lpp": ("launchpad-pro", "Launchpad Pro（旧モデル）", "launchpad-pro"),
    "mk2": ("launchpad-mk2", "Launchpad Mk2", "launchpad-mk2"),
    "minimk1": ("launchpad-mini-mk1", "Launchpad Mini Mk1", "launchpad-s-and-mini"),
    "lps": ("launchpad-s", "Launchpad S", "launchpad-s-and-mini"),
}

# Deliberately narrow, reviewed prose. These are not statements about Novation
# stock firmware, nor evidence that any untested hardware path works.
DETAILS = {
    "lpx": (
        "Noteボタン（入力番号96）でNote、Customボタン（97）でX Customに切り替わります。"
        "Noteを約500 ms押し続けるとNote内の設定表示になります（CoreFW全体のSetupとは別）。"
        "Customは再押下で既定の配列を選び直します。"
        "[根拠](../../src/app/host.rs)・[Note](../../src/app/note/mod.rs)・"
        "[Custom](../../src/app/x_custom.rs)",
        "XのNote／Customでは演奏用USB MIDIポートのガイドSysExを処理します。"
        "LiveからのDAWポート入力はDAW画面を手動で選ぶまで無視します。"
        "[根拠](../../src/app/host.rs)",
    ),
    "lppmk3": (
        "Noteボタン（入力番号94）でNote、Customボタン（96）で固定Custom Noteに切り替わります。"
        "Shift（90）を押しながらNote／Customを押すと各モード内の設定表示になります。"
        "NoteまたはCustom中はSession（93）でDAWに移ります。"
        "[根拠](../../src/app/host.rs)",
        "DIN In→USBのCC64オン／オフは実機確認済みです。Out 1／Out 2相当はUART送信を実装済みですが、"
        "出力端子との対応とPA7のOut／Thru方向は未実機確認です。"
        "試験用のPA7切替は通常操作では行いません。"
        "[実装と検証範囲](../../PRO-MK3-DIN-DIAGNOSTIC.md)・"
        "[送信経路](../../src/sys/driver/launchpad-pro-mk3/din_tx.rs)",
    ),
    "mini": (
        "起動アニメーション後はPerformanceが既定です。Setupのモード選択でDAW／Programmerにも切り替えられます。"
        "X専用Note／CustomとPro Mk3専用Custom Noteをこの機種の機能として扱わないでください。"
        "[根拠](../../src/app/boot/mod.rs)・[機種別モード定義](../../src/app/mod.rs)",
    ),
    "lpp": (
        "起動アニメーション後はPerformanceが既定です。SetupではDAW／Programmerも選択できます。"
        "X／Pro Mk3固有のNote／Custom操作をこの旧Pro用FWに当てはめないでください。"
        "[根拠](../../src/app/boot/mod.rs)・[機種別モード定義](../../src/app/mod.rs)",
    ),
    "mk2": (
        "起動アニメーション後はPerformanceが既定です。SetupでDAW／Programmerを選択できます。"
        "X／Pro Mk3固有のNote／Custom操作はこのFWの説明ではありません。"
        "[根拠](../../src/app/boot/mod.rs)・[機種別モード定義](../../src/app/mod.rs)",
    ),
    "minimk1": (
        "起動アニメーション後はPerformanceが既定です。SetupではDAW／Programmerも選択できます。"
        "非RGB機種です。RGB表示・カスタムパレット編集の操作はこの説明書では案内しません。"
        "[根拠](../../Cargo.toml)・[起動モード](../../src/app/boot/mod.rs)",
    ),
    "lps": (
        "起動アニメーション後はPerformanceが既定です。SetupではDAW／Programmerも選択できます。"
        "非RGB機種です。RGB表示・カスタムパレット編集の操作はこの説明書では案内しません。"
        "[根拠](../../Cargo.toml)・[起動モード](../../src/app/boot/mod.rs)",
    ),
}

SHARED = (
    "Cargo.toml", ".cargo/config.toml", "xtask/src/main.rs", "src/app/mod.rs", "src/app/host.rs", "src/app/guide.rs",
    "src/app/boot", "src/app/setup", "src/app/performance", "src/app/programmer",
    "src/app/daw", "src/app/palette_editor", "src/app/velocity.rs",
    "src/sys/settings.rs", "src/sys/rotation.rs", "src/sys/sysex", "src/sys/midi.rs",
    "src/utils/device.rs", "src/utils/layout.rs",
)
EXTRA = {
    "lpx": ("src/app/note", "src/app/x_custom.rs"),
    "lppmk3": ("src/app/pro_note", "src/app/custom_note", "PRO-MK3-DIN-DIAGNOSTIC.md"),
}


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message + "; review generator and firmware before updating manuals")


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def expanded_features(feature: str, declarations: dict) -> set[str]:
    result = {feature}
    for item in declarations[feature]:
        if item in declarations and item not in result:
            result.update(expanded_features(item, declarations))
    return result


def app_modes(features: set[str]) -> list[str]:
    enum = text("src/app/mod.rs").split("pub enum AppId {", 1)[1].split("}", 1)[0]
    modes = []
    condition = None
    for line in enum.splitlines():
        line = line.strip()
        if line.startswith("#[cfg("):
            require(bool(re.fullmatch(r'#\[cfg\((?:feature = "[\w-]+"|any\(feature = "[\w-]+"(?:, feature = "[\w-]+")+\))\)\]', line)),
                    f"Unknown AppId cfg: {line}")
            condition = line
        elif re.fullmatch(r"[A-Za-z]+,", line):
            required = re.findall(r'feature = "([^"]+)"', condition or "")
            if not required or any(f in features for f in required):
                modes.append(line[:-1])
            condition = None
        elif line:
            raise ValueError(f"Unrecognized AppId entry: {line}")
    require({"Boot", "Setup", "Performance", "Programmer", "Daw", "PaletteEditor"} <= set(modes),
            "AppId shared modes changed")
    return modes


def sources(alias: str, driver: str) -> dict[str, str]:
    paths = (*SHARED, *EXTRA.get(alias, ()), f"src/sys/driver/{driver}")
    files: set[Path] = set()
    for name in paths:
        path = ROOT / name
        require(path.exists(), f"Missing source: {name}")
        files.update(path.rglob("*.rs") if path.is_dir() else [path])
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(files)}


def source_digest(files: dict[str, str]) -> str:
    return hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()[:16]


def webflasher_catalog() -> str:
    """Match the updater.product field in an xtask build manifest to one MD."""
    package_source = text("xtask/src/main.rs")
    entries = []
    for alias, (feature, _, _) in DEVICES.items():
        block = re.search(r'"' + re.escape(alias) + r'"(?:\s*\|\s*"[^"]+")?\s*=> Some\(DeviceCfg \{(.*?)\n\s*\}\),',
                          package_source, re.DOTALL)
        require(block is not None, f"Missing xtask package configuration for {alias}")
        fields = block.group(1)
        cfg_feature = re.search(r'feature:\s*"([^"]+)"', fields)
        product = re.search(r'syx_product:\s*"([^"]+)"', fields)
        require(cfg_feature is not None and cfg_feature.group(1) == feature and product is not None,
                f"Updater product/feature changed for {alias}")
        entries.append({"alias": alias, "updater_product": product.group(1),
                        "manual": f"docs/firmware/{alias}.md"})
    require(len({entry["updater_product"] for entry in entries}) == len(entries),
            "Updater products are ambiguous")
    return json.dumps({"schema": 1, "manuals": entries}, ensure_ascii=False, indent=2) + "\n"


def render(alias: str, spec: tuple[str, str, str], declarations: dict, files: dict[str, str]) -> str:
    feature, name, _ = spec
    flags = expanded_features(feature, declarations)
    modes = app_modes(flags)
    host = text("src/app/host.rs")
    boot = text("src/app/boot/mod.rs")
    setup = text("src/app/setup/mod.rs")
    init = text("src/app/setup/page_init.rs")
    require('SETUP_HOLD_TICKS: u16 = 500' in host and 'SETUP_HOLD_BUTTON_INDEX: u8 = 95' in host
            and 'SETUP_BUTTON_INDEX: u8 = 0' in host, "Setup entry/hold changed")
    require('self.setup.finish_setup()' in host and 'settings::save()' in setup, "Setup exit/save changed")
    require('AppId::Note' in boot and 'AppId::Performance' in boot, "Boot default changed")
    require('89 => Some(PageId::Init)' in setup and '79 => Some(PageId::Leds)' in setup
            and '69 => Some(PageId::Velocity)' in setup and '59 => Some(PageId::Aftertouch)' in setup,
            "Setup page selection changed")
    programmer = text("src/app/programmer/mod.rs")
    performance = text("src/app/performance/mod.rs")
    require('driver::send_midi(MidiPort::Midi, &[0x90, event.index, velocity])' in programmer
            and 'else if s.velocity_enabled == 0 { 127 }' in programmer,
            "Programmer MIDI behaviour changed")
    require('driver::send_midi(MidiPort::Midi, &[0x90, note, velocity])' in performance
            and 'led::set(index, if event.pressed { 0x00ff00 } else { 0 })' in performance,
            "Performance MIDI/local LED behaviour changed")
    selectors = [(11, "Performance", "PERFORMANCE_BUTTON"), (12, "DAW", "DAW_BUTTON"),
                 (13, "Programmer", "PROGRAMMER_BUTTON")]
    if alias == "lpx":
        selectors.append((14, "Note", "NOTE_BUTTON"))
    if alias == "lppmk3":
        selectors += [(14, "Custom Note", "CUSTOM_NOTE_BUTTON"), (24, "Note", "NOTE_BUTTON")]
    for index, _, symbol in selectors:
        require(f"const {symbol}: u8 = {index};" in init, f"Setup selector {symbol} changed")
    pages = ["89: メイン（モード選択）", "79: LED・明るさ・回転"]
    if "pressure-sensitive" in flags:
        pages += ["69: Velocity（ベロシティ）", "59: Aftertouch（アフタータッチ）"]
    boot_mode = "Note" if "launchpad-x" in flags or "launchpad-pro-mk3" in flags else "Performance"
    enter = ("入力番号95を約500 ms押し続けるとSetup。Setup中は95の押下で退出。"
             if "no-setup-btn" in flags else
             "入力番号0（専用Setupボタン）を押すとSetup。Setup中に再度押すと退出。")
    hold_note = ("約500 msは実機上で押し続けます。" if "no-setup-btn" in flags else
                 "専用Setupボタンは短押しで入れます。")
    velocity_controls = ("- Velocityページ: 31でベロシティ変換のON/OFF、21／22／23でプリセット、"
                         "82で曲線編集、83で最低値の選択。31がOFFならPerformance／Programmerの押下出力は127です。"
                         "[根拠](../../src/app/setup/page_velocity.rs)・[演奏出力](../../src/app/performance/mod.rs)。\n"
                         "- Aftertouchページ: 31でOFF、32でPoly、33でChannel、21～23で曲線を選択。"
                         "曲線はAftertouchが有効なときのみ変更できます。"
                         "[根拠](../../src/app/setup/page_aftertouch.rs)。\n") if "pressure-sensitive" in flags else ""
    mirror_controls = ("- Pro Mk3のLEDページでは11で底辺ミラーのON/OFFを切り替えます。"
                       "[根拠](../../src/app/setup/page_leds.rs)。\n" if alias == "lppmk3" else "")
    note = "\n\n".join(DETAILS[alias])
    flag_text = "、".join(f for f in ("no-setup-btn", "pressure-sensitive", "rgb-color", "pro-din-probe") if f in flags) or "なし"
    mode_text = "、".join(modes)
    return f"""<!-- Generated by tools/generate_fw_manuals.py. Edit the generator, not this file. -->
# {name} — 現行CoreFWの操作説明

> 対象: このリポジトリの `cargo {alias}`（`{feature}`）ビルド。純正Novation FWの説明ではありません。\
実機確認と、コードからの確認は別です。ソース確認ID: `{source_digest(files)}`。

## 起動とモード

- 起動アニメーション後の既定モード: **{boot_mode}**。\
  [根拠](../../src/app/boot/mod.rs)。
- この機種のビルドに存在するアプリ（すべてが物理ボタンから直接開ける・実機検証済みという意味ではありません）: {mode_text}。\
  [根拠](../../src/app/mod.rs)・[遷移](../../src/app/host.rs)。
- 機種別featureで有効: {flag_text}。\
  [根拠](../../Cargo.toml)。

## Setup（本体設定）

{enter} {hold_note}\
[根拠](../../src/app/host.rs)。

- 右側のタブ: {"／".join(pages)}。\
  [根拠](../../src/app/setup/mod.rs)。
- メインページでは {"／".join(f"{i}: {label}" for i, label, _ in selectors)} を選択できます。\
  選択後にSetupを退出すると切り替わります。押下番号は回転設定が既定の場合の論理番号です。\
  [根拠](../../src/app/setup/page_init.rs)・[回転](../../src/app/host.rs)。
- Setupを退出すると設定保存処理が呼ばれます。フラッシュへの書込み成功を保証する記述ではありません。\
  [根拠](../../src/app/setup/mod.rs)。
- LEDページでは31～38で明るさを選択できます。回転もこのページで切り替えます。\
  回転時はメインのグリッド番号が変わるので、上記の番号は回転なしでの値です。\
  [根拠](../../src/app/setup/page_leds.rs)・[入力変換](../../src/app/host.rs)。
{velocity_controls}{mirror_controls}
## 演奏・表示の基本

- Performance: パッドを押すと演奏用MIDIポートへNote On、離すとvelocity 0のメッセージを送ります。\
  押下中は本体側LEDを緑で表示します。ベロシティ設定がOFFなら押下時127です。\
  [根拠](../../src/app/performance/mod.rs)。
- Programmer: 押したパッド番号をノート番号として演奏用MIDIポートへ送ります。\
  押下時127（ベロシティ設定OFFの場合）、離したとき0です。\
  [根拠](../../src/app/programmer/mod.rs)。
- DAW: DAW用MIDIポートを使います。DAWページ・レイアウトの詳細なホスト互換性は未検証です。\
  [根拠](../../src/app/daw/mod.rs)。

## この機種固有の操作・注意

{note}

## 制限と安全

この文書はこのコードの動作説明であり、純正FW互換性・電気的動作・すべてのMIDI／DAW操作を保証しません。\
機種違いの `.syx` を書き込まないでください。ブートローダーの入り方も機種ごとに異なるため、\
共通READMEの「Capture MIDI」手順を全機種へ適用しないでください。FWの書込みと最終的な実機確認はユーザーが行います。

## 更新方法

`python3 tools/generate_fw_manuals.py --check` で7機種の説明書と根拠ソースを照合します。\
ソースが変わったら説明をレビューし、`--accept-source-changes` で確認済みとして再生成してください。\
このIDはコードとの同期記録であって実機試験の証明ではありません。
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="fail on stale manuals or unreviewed code")
    group.add_argument("--write", action="store_true", help="regenerate from reviewed sources")
    group.add_argument("--accept-source-changes", action="store_true", help="reviewed code; refresh lock and manuals")
    args = parser.parse_args()
    cargo = tomllib.loads(text("Cargo.toml"))
    aliases = tomllib.loads(text(".cargo/config.toml"))["alias"]
    declarations = cargo["features"]
    require(set(DEVICES) == {a for a, command in aliases.items() if "-- package " in command},
            "Device build aliases changed")
    for alias, (feature, _, _) in DEVICES.items():
        require(feature in declarations and f"package {alias} --release" in aliases[alias],
                f"Device feature/alias changed: {alias}")

    manifests = {alias: sources(alias, spec[2]) for alias, spec in DEVICES.items()}
    lock_data = {"schema": 1, "source_sha256": manifests}
    old = json.loads(LOCK.read_text(encoding="utf-8")) if LOCK.exists() else None
    if not args.accept_source_changes and old != lock_data:
        print("Source changed or review lock missing: inspect firmware behaviour, then use --accept-source-changes", file=sys.stderr)
        return 1

    expected = {OUT / f"{alias}.md": render(alias, spec, declarations, manifests[alias])
                for alias, spec in DEVICES.items()}
    expected[OUT / "catalog.json"] = webflasher_catalog()
    if args.check:
        unexpected = set(OUT.glob("*.md")) - set(expected) - {OUT / "README.md"}
        if unexpected:
            print("Unexpected old manuals: " + ", ".join(str(p.relative_to(ROOT)) for p in sorted(unexpected)), file=sys.stderr)
            return 1
        stale = [str(p.relative_to(ROOT)) for p, content in expected.items()
                 if not p.exists() or p.read_text(encoding="utf-8") != content]
        if stale:
            print("Stale manuals: " + ", ".join(stale), file=sys.stderr)
            return 1
    else:
        OUT.mkdir(parents=True, exist_ok=True)
        for path, content in expected.items():
            path.write_text(content, encoding="utf-8")
        LOCK.write_text(json.dumps(lock_data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{'Checked' if args.check else 'Generated'} {len(DEVICES)} firmware manuals and catalog")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, KeyError, ValueError) as error:
        print(f"Cannot generate manuals: {error}", file=sys.stderr)
        sys.exit(1)

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister
// Copyright (C) 2026 ZephyrCodesStuff

use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// This pure mapping module can be tested on the hosted CI runner without
// compiling the ARM-specific firmware input driver for the host.
#[cfg(test)]
#[path = "../../src/app/note/layout.rs"]
mod note_layout;

#[cfg(test)]
#[path = "../../src/app/note/velocity.rs"]
mod note_velocity;

#[cfg(test)]
#[path = "../../src/app/note/transpose.rs"]
mod note_transpose;

// The private guide's envelope and session state are pure and host-testable.
#[cfg(test)]
#[path = "../../src/app/guide.rs"]
pub mod note_guide;

// Compile and run the actual Note-mode App tests with lightweight host drivers.
// The firmware library disables its own test target on non-ARM hosts.
#[cfg(test)]
#[allow(dead_code, unused_imports)]
mod app;
#[cfg(test)]
#[allow(dead_code)]
mod sys;
#[cfg(test)]
#[allow(dead_code)]
mod driver;

#[cfg(test)]
mod private_guide_mapping_tests {
    use super::{note_guide::{self, Guide}, note_layout::{note_for, Preset}};

    fn mapped(guide: &Guide, preset: Preset, transpose: i16) -> Vec<(u8, u8)> {
        let mut result = Vec::new();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                if let Some(color) = guide.pad_color(index, note_for(preset, index, transpose), false,
                    |side, pitch| note_guide::side_has_pitch(side, pitch,
                        |pad| note_for(preset, pad, transpose))) {
                    result.push((index, color));
                }
            }
        }
        result
    }

    #[test]
    fn pitched_guides_follow_layout_and_transpose_but_stay_in_one_half() {
        let mut guide = Guide::new();
        guide.begin();
        guide.set(0, 0, 48, 13);
        assert_eq!(mapped(&guide, Preset::Chromatic, 0), vec![(11, 13)]);
        assert_eq!(mapped(&guide, Preset::Chromatic, -4), vec![(21, 13)]);
        assert_eq!(mapped(&guide, Preset::Classic, 0), vec![(41, 13)]);
        guide.set(0, 1, 48, 21);
        assert_eq!(mapped(&guide, Preset::Classic, 0), vec![(41, 13)]); // pitch 48 absent on right
        guide.set(1, 1, 72, 21);
        assert_eq!(mapped(&guide, Preset::Chromatic, 0), vec![(11, 13), (15, 21)]);
        guide.set(0, 2, 72, 9);
        assert_eq!(mapped(&guide, Preset::Chromatic, 0), vec![(11, 13), (15, 21), (71, 9)]);
        guide.end();
        assert!(mapped(&guide, Preset::Chromatic, 0).is_empty());
    }
}

fn find_objcopy() -> Result<PathBuf, Box<dyn Error>> {
    let check = if cfg!(windows) {
        Command::new("where").arg("arm-none-eabi-objcopy").output()
    } else {
        Command::new("which").arg("arm-none-eabi-objcopy").output()
    };
    if let Ok(output) = check {
        if output.status.success() {
            return Ok(PathBuf::from("arm-none-eabi-objcopy"));
        }
    }

    if let Ok(sysroot_out) = Command::new("rustc").args(["--print", "sysroot"]).output() {
        if sysroot_out.status.success() {
            if let Ok(sysroot) = String::from_utf8(sysroot_out.stdout) {
                let sysroot = sysroot.trim();
                if let Ok(version_out) = Command::new("rustc").arg("-vV").output() {
                    if version_out.status.success() {
                        if let Ok(version_str) = String::from_utf8(version_out.stdout) {
                            if let Some(host_line) =
                                version_str.lines().find(|line| line.starts_with("host:"))
                            {
                                if let Some(host_triple) = host_line.split_whitespace().nth(1) {
                                    let llvm_objcopy = PathBuf::from(sysroot)
                                        .join("lib")
                                        .join("rustlib")
                                        .join(host_triple)
                                        .join("bin")
                                        .join(if cfg!(windows) {
                                            "llvm-objcopy.exe"
                                        } else {
                                            "llvm-objcopy"
                                        });
                                    if llvm_objcopy.exists() {
                                        return Ok(llvm_objcopy);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("Neither 'arm-none-eabi-objcopy' nor 'llvm-objcopy' (rustup component llvm-tools) was found.\n\
         Please install either the ARM GCC toolchain or add llvm-tools via:\n\
         rustup component add llvm-tools".into())
}

struct DeviceCfg {
    device: &'static str,
    model_name: &'static str,
    feature: &'static str,
    target_triple: &'static str,
    // Universal Device Inquiry family bytes, in the same LSB/MSB order used
    // by src/sys/sysex/device_inquiry.rs.
    identity_family: [u8; 2],
    syx_product: &'static str,
    // These updater IDs are copied from tools/syxtool.py. `None` is used for
    // legacy products because their updater product is a two-byte ID rather
    // than the modern family/product pair.
    updater_family_id: Option<u8>,
    updater_product_id: u16,
    default_version: &'static str,
    artifact_stem: &'static str,
    objcopy_pad_to: Option<&'static str>,
}

const ALL_DEVICES: &[&str] = &["lpx", "mini", "minimk1", "lps", "mk2", "lpp", "lppmk3"];

fn device_cfg(name: &str) -> Option<DeviceCfg> {
    match name {
        "lpx" => Some(DeviceCfg {
            device: "lpx",
            model_name: "Launchpad X",
            feature: "launchpad-x",
            target_triple: "thumbv7em-none-eabihf",
            identity_family: [0x03, 0x01],
            syx_product: "/x",
            updater_family_id: Some(0x02),
            updater_product_id: 0x0c,
            default_version: "351",
            artifact_stem: "core-launchpad-x",
            objcopy_pad_to: None,
        }),
        "mini" => Some(DeviceCfg {
            device: "mini",
            model_name: "Launchpad Mini MK3",
            feature: "launchpad-mini-mk3",
            target_triple: "thumbv7em-none-eabihf",
            identity_family: [0x13, 0x01],
            syx_product: "/minimk3",
            updater_family_id: Some(0x02),
            updater_product_id: 0x0d,
            default_version: "407",
            artifact_stem: "core-launchpad-mini-mk3",
            objcopy_pad_to: None,
        }),
        "lps" | "launchpad-s" => Some(DeviceCfg {
            device: "lps",
            model_name: "Launchpad S",
            feature: "launchpad-s",
            target_triple: "thumbv7m-none-eabi",
            identity_family: [0x20, 0x00],
            syx_product: "/lps",
            updater_family_id: None,
            updater_product_id: 0x20,
            default_version: "999",
            artifact_stem: "core-launchpad-s",
            objcopy_pad_to: None,
        }),
        "minimk1" => Some(DeviceCfg {
            device: "minimk1",
            model_name: "Launchpad Mini MK1",
            feature: "launchpad-mini-mk1",
            target_triple: "thumbv7m-none-eabi",
            identity_family: [0x36, 0x00],
            syx_product: "/minimk1",
            updater_family_id: None,
            updater_product_id: 0x36,
            default_version: "999",
            artifact_stem: "core-launchpad-mini-mk1",
            objcopy_pad_to: None,
        }),
        "mk2" => Some(DeviceCfg {
            device: "mk2",
            model_name: "Launchpad MK2",
            feature: "launchpad-mk2",
            target_triple: "thumbv7m-none-eabi",
            identity_family: [0x69, 0x00],
            syx_product: "/mk2",
            updater_family_id: Some(0x00),
            updater_product_id: 0x69,
            default_version: "999",
            artifact_stem: "core-launchpad-mk2",
            objcopy_pad_to: None,
        }),
        "lpp" | "pro" => Some(DeviceCfg {
            device: "lpp",
            model_name: "Launchpad Pro",
            feature: "launchpad-pro",
            target_triple: "thumbv7m-none-eabi",
            identity_family: [0x51, 0x00],
            syx_product: "/lpp",
            updater_family_id: Some(0x00),
            updater_product_id: 0x51,
            default_version: "154",
            artifact_stem: "core-launchpad-pro",
            objcopy_pad_to: None,
        }),
        "lppmk3" | "pro-mk3" => Some(DeviceCfg {
            device: "lppmk3",
            model_name: "Launchpad Pro MK3",
            feature: "launchpad-pro-mk3",
            target_triple: "thumbv7em-none-eabihf",
            identity_family: [0x23, 0x01],
            syx_product: "/lppmk3",
            updater_family_id: Some(0x02),
            updater_product_id: 0x0e,
            default_version: "999",
            artifact_stem: "core-launchpad-pro-mk3",
            objcopy_pad_to: Some("0x08080000"),
        }),
        _ => None,
    }
}

fn run(cmd: &str, args: &[&str], cwd: &Path) -> Result<(), Box<dyn Error>> {
    eprintln!("+ {} {}", cmd, args.join(" "));
    let status = Command::new(cmd).args(args).current_dir(cwd).status()?;
    if !status.success() {
        return Err(format!("command failed: {} {:?}", cmd, args).into());
    }
    Ok(())
}

fn parse_package_args(args: &[String]) -> Result<(String, Option<String>, bool), Box<dyn Error>> {
    if args.len() < 2 {
        return Err(
            "usage: cargo xtask package <lpx|mini|minimk1|lps|mk2|lpp|lppmk3> [--version <hex3>] [--release]"
                .into(),
        );
    }
    let device = args[1].clone();
    let mut version: Option<String> = None;
    let mut release = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--version" => {
                i += 1;
                if i >= args.len() {
                    return Err("--version requires a value".into());
                }
                version = Some(args[i].clone());
            }
            "--release" => {
                release = true;
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
        i += 1;
    }
    Ok((device, version, release))
}

const SHA256_K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (word, bytes) in schedule[..16].iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let mut working: [u32; 8] = state;
        for index in 0..64 {
            let s1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let choose = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let temp1 = working[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(SHA256_K[index])
                .wrapping_add(schedule[index]);
            let s0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let majority =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temp2 = s0.wrapping_add(majority);
            working[7] = working[6];
            working[6] = working[5];
            working[5] = working[4];
            working[4] = working[3].wrapping_add(temp1);
            working[3] = working[2];
            working[2] = working[1];
            working[1] = working[0];
            working[0] = temp1.wrapping_add(temp2);
        }
        for (word, value) in state.iter_mut().zip(working) {
            *word = (*word).wrapping_add(value);
        }
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn hex_digest(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

fn syx_metadata(data: &[u8]) -> Result<(Option<usize>, Option<u32>), Box<dyn Error>> {
    let mut count = 0usize;
    let mut crc = None;
    let mut index = 0usize;
    while index < data.len() {
        while index < data.len() && data[index] != 0xf0 {
            index += 1;
        }
        if index == data.len() {
            break;
        }
        let end = data[index + 1..]
            .iter()
            .position(|byte| *byte == 0xf7)
            .map(|offset| index + offset + 1)
            .ok_or("unterminated SysEx message")?;
        let message = &data[index + 1..end];
        if message.len() >= 5 && message[..4] == [0x00, 0x20, 0x29, 0x00] {
            count += 1;
            // LPX updater headers carry an eight-nibble MSB-first CRC32 in
            // payload bytes 15..23. Other products do not define that field,
            // so leave CRC metadata null for them rather than inventing one.
            if message[4] == 0x7c && message.len() >= 28 {
                let payload = &message[5..];
                let mut value = 0u32;
                for nibble in &payload[15..23] {
                    value = (value << 4) | (*nibble as u32 & 0x0f);
                }
                crc = Some(value);
            }
        }
        index = end + 1;
    }
    if count == 0 {
        return Err("no Novation SysEx messages found".into());
    }
    Ok((Some(count), crc))
}

fn source_commit(repo: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!commit.is_empty()).then_some(commit)
}

fn source_dirty_override(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "dirty" => Some(true),
        "0" | "false" | "no" | "clean" => Some(false),
        _ => None,
    }
}

fn source_dirty(repo: &Path) -> Result<bool, Box<dyn Error>> {
    // Release automation can set SOURCE_DIRTY=clean/dirty (or 0/1) to make
    // provenance deterministic when the checkout is assembled externally.
    if let Ok(value) = env::var("SOURCE_DIRTY") {
        return source_dirty_override(&value)
            .ok_or_else(|| "SOURCE_DIRTY must be clean/dirty, true/false, or 0/1".into());
    }

    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(repo)
        .output()?;
    if !output.status.success() {
        return Err("unable to determine source checkout status".into());
    }
    Ok(!output.stdout.is_empty())
}

struct ArtifactMetadata {
    name: String,
    path: String,
    size: u64,
    sha256: String,
    message_count: Option<usize>,
    crc32: Option<u32>,
}

fn artifact_metadata(
    repo: &Path,
    path: &Path,
    name: String,
) -> Result<ArtifactMetadata, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let (message_count, crc32) = if path.extension().and_then(|ext| ext.to_str()) == Some("syx") {
        syx_metadata(&bytes)?
    } else {
        (None, None)
    };
    let relative = path
        .strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(ArtifactMetadata {
        name,
        path: relative,
        size: bytes.len() as u64,
        sha256: hex_digest(&sha256(&bytes)),
        message_count,
        crc32,
    })
}

fn write_manifest(
    repo: &Path,
    cfg: &DeviceCfg,
    version: &str,
    artifacts: &[ArtifactMetadata],
) -> Result<String, Box<dyn Error>> {
    let mut manifest = String::new();
    manifest.push_str("{\n");
    manifest.push_str("  \"schema\": 1,\n");
    writeln!(
        &mut manifest,
        "  \"model\": {{\"name\": {:?}, \"family_lsb\": {}, \"family_msb\": {}}},",
        cfg.model_name, cfg.identity_family[0], cfg.identity_family[1]
    )
    .unwrap();
    write!(
        &mut manifest,
        "  \"updater\": {{\"product\": {:?}, ",
        cfg.syx_product
    )
    .unwrap();
    match cfg.updater_family_id {
        Some(family) => write!(&mut manifest, "\"family_id\": {}, ", family).unwrap(),
        None => manifest.push_str("\"family_id\": null, "),
    }
    writeln!(
        &mut manifest,
        "\"product_id\": {}, \"version\": {:?}}},",
        cfg.updater_product_id, version
    )
    .unwrap();
    match source_commit(repo) {
        Some(commit) => writeln!(&mut manifest, "  \"source_commit\": {:?},", commit).unwrap(),
        None => manifest.push_str("  \"source_commit\": null,\n"),
    }
    writeln!(
        &mut manifest,
        "  \"source_dirty\": {},",
        source_dirty(repo)?
    )
    .unwrap();
    manifest.push_str("  \"artifacts\": [\n");
    for (index, artifact) in artifacts.iter().enumerate() {
        write!(
            &mut manifest,
            "    {{\"name\": {:?}, \"path\": {:?}, \"size\": {}, \"sha256\": {:?}, \"message_count\": ",
            artifact.name, artifact.path, artifact.size, artifact.sha256
        )
        .unwrap();
        match artifact.message_count {
            Some(count) => write!(&mut manifest, "{count}").unwrap(),
            None => manifest.push_str("null"),
        }
        manifest.push_str(", \"crc32\": ");
        match artifact.crc32 {
            Some(value) => write!(
                &mut manifest,
                "{{\"algorithm\": \"novation-lpx\", \"value\": \"0x{value:08X}\"}}"
            )
            .unwrap(),
            None => manifest.push_str("null"),
        }
        if index + 1 == artifacts.len() {
            manifest.push_str("}\n");
        } else {
            manifest.push_str("},\n");
        }
    }
    manifest.push_str("  ]\n}\n");

    let manifest_path = repo.join("build").join(cfg.device).join("manifest.json");
    fs::write(&manifest_path, &manifest)?;
    let release_manifest = repo
        .join("build")
        .join(format!("{}.manifest.json", cfg.artifact_stem));
    fs::write(&release_manifest, &manifest)?;
    Ok(manifest_path.display().to_string())
}

fn package(
    repo: &Path,
    device: &str,
    version: Option<&str>,
    release: bool,
) -> Result<(), Box<dyn Error>> {
    let cfg = device_cfg(device)
        .ok_or("device must be one of: lppmk3, lpx, mini, lpp, mk2, lps, minimk1")?;
    let release = release || cfg.device == "mk2" || cfg.device == "minimk1";
    let env_version = env::var("FW_VERSION").ok();
    let version = version
        .or(env_version.as_deref())
        .unwrap_or(cfg.default_version);
    let profile = if release { "release" } else { "debug" };

    let mut cargo_args = vec![
        "build",
        "--bin",
        "core",
        "--target",
        cfg.target_triple,
        "--no-default-features",
        "--features",
        cfg.feature,
    ];
    if release {
        cargo_args.push("--release");
    }
    // The outer cargo run does not propagate --locked to this child cargo build.
    if env::var_os("CORE_FW_CARGO_LOCKED").is_some() {
        cargo_args.push("--locked");
    }
    run("cargo", &cargo_args, repo)?;

    let elf = repo
        .join("target")
        .join(cfg.target_triple)
        .join(profile)
        .join("core");
    if !elf.exists() {
        return Err(format!("ELF not found: {}", elf.display()).into());
    }

    let out_dir = repo.join("build").join(cfg.device);
    fs::create_dir_all(&out_dir)?;
    let bin = out_dir.join("fw.bin");
    let syx = out_dir.join("fw.syx");
    let final_syx = repo
        .join("build")
        .join(format!("{}.syx", cfg.artifact_stem));

    let objcopy_bin = find_objcopy()?;
    let elf_arg = elf.display().to_string();
    let bin_arg = bin.display().to_string();
    if let Some(pad_to) = cfg.objcopy_pad_to {
        run(
            &objcopy_bin.to_string_lossy(),
            &[
                "-O",
                "binary",
                "--pad-to",
                pad_to,
                "--gap-fill",
                "0xFF",
                &elf_arg,
                &bin_arg,
            ],
            repo,
        )?;
    } else {
        run(
            &objcopy_bin.to_string_lossy(),
            &["-O", "binary", &elf_arg, &bin_arg],
            repo,
        )?;
    }
    run(
        "python3",
        &[
            "tools/syxtool.py",
            "--to-syx",
            cfg.syx_product,
            version,
            &bin.display().to_string(),
            &syx.display().to_string(),
        ],
        repo,
    )?;
    fs::copy(&syx, &final_syx)?;

    let artifacts = [
        artifact_metadata(repo, &bin, "fw.bin".to_owned())?,
        artifact_metadata(repo, &syx, "fw.syx".to_owned())?,
        artifact_metadata(repo, &final_syx, format!("{}.syx", cfg.artifact_stem))?,
    ];
    let manifest_path = write_manifest(repo, &cfg, version, &artifacts)?;

    println!("{}", final_syx.display());
    println!("{}", manifest_path);
    Ok(())
}

fn package_all(repo: &Path) -> Result<(), Box<dyn Error>> {
    for device in ALL_DEVICES {
        eprintln!("==> packaging {device} --release");
        package(repo, device, None, true)?;
    }
    Ok(())
}

fn repo_root() -> Result<PathBuf, Box<dyn Error>> {
    let cwd = env::current_dir()?;
    Ok(cwd)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err("usage: cargo xtask <package>".into());
    }

    let repo = repo_root()?;
    match args[0].as_str() {
        "all" => package_all(&repo),
        "package" => {
            let (device, version, release) = parse_package_args(&args)?;
            package(&repo, &device, version.as_deref(), release)
        }
        cmd => Err(format!("unknown xtask command: {cmd}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_empty_input() {
        assert_eq!(
            hex_digest(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn syx_metadata_counts_messages_and_lpx_crc() {
        let mut message = vec![0xf0, 0x00, 0x20, 0x29, 0x00, 0x7c];
        message.extend([0; 15]);
        message.extend([0, 1, 2, 3, 4, 5, 6, 7]);
        message.push(0xf7);
        let (count, crc) = syx_metadata(&message).expect("valid SysEx");
        assert_eq!(count, Some(1));
        assert_eq!(crc, Some(0x0123_4567));
    }

    #[test]
    fn source_dirty_override_is_explicit_and_deterministic() {
        assert_eq!(source_dirty_override("dirty"), Some(true));
        assert_eq!(source_dirty_override("1"), Some(true));
        assert_eq!(source_dirty_override("clean"), Some(false));
        assert_eq!(source_dirty_override("0"), Some(false));
        assert_eq!(source_dirty_override("maybe"), None);
    }
}

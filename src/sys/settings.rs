// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::cell::UnsafeCell;

use crate::driver;
use crate::utils::palette::PALETTE_NOVATION;

pub const SETTINGS_FLASH_SIZE: usize = 6 * 1024;
const SETTINGS_MAGIC: [u8; 6] = *b"COREFW";
const SETTINGS_CRC_OFFSET: usize = SETTINGS_MAGIC.len();
const SETTINGS_CRC_SIZE: usize = 4;
const SETTINGS_DATA_OFFSET: usize = SETTINGS_CRC_OFFSET + SETTINGS_CRC_SIZE;
const BASIC_SETTINGS_SIZE: usize = 7;
const PALETTE_OFFSET: usize = SETTINGS_DATA_OFFSET + BASIC_SETTINGS_SIZE;
const CUSTOM_PALETTE_BYTES: usize = 3 * 3 * 128;
const SETTINGS_WIRE_SIZE: usize = PALETTE_OFFSET + CUSTOM_PALETTE_BYTES;

// The two existing velocity bytes hold two Bezier points, floor, and toggle.
pub const VELOCITY_POINT_NONE: u8 = crate::app::velocity::NO_POINT;

#[cfg(feature = "launchpad-pro-mk3")]
const _: () = assert!(
    SETTINGS_WIRE_SIZE
        <= crate::sys::driver::common::storage::external::SETTINGS_SIZE as usize
);

#[derive(Copy, Clone)]
pub struct Settings {
    pub brightness: u8,
    pub velocity_enabled: u8,
    pub velocity_floor: u8, // floor selection 0..7
    pub velocity_point_a: u8, // first Bezier point, 63 = absent
    pub velocity_point_b: u8, // second Bezier point, 63 = absent
    #[cfg(feature = "launchpad-x")]
    pub x_note_scale_mode: u8,
    #[cfg(feature = "launchpad-x")]
    pub x_note_scale: u8,
    #[cfg(feature = "launchpad-x")]
    pub x_note_root: u8,
    #[cfg(feature = "launchpad-x")]
    pub x_note_overlap: u8,
    #[cfg(feature = "launchpad-x")]
    pub x_note_channel: u8,
    pub aftertouch_mode: u8,
    pub aftertouch_curve: u8,
    pub palette: u8,
    pub mirror_enabled: u8,
    pub custom_palette: [[[u8; 128]; 3]; 3],
}

impl Settings {
    pub const fn empty() -> Self {
        Self {
            brightness: 0,
            velocity_enabled: 0,
            velocity_floor: 0,
            velocity_point_a: VELOCITY_POINT_NONE,
            velocity_point_b: VELOCITY_POINT_NONE,
            #[cfg(feature = "launchpad-x")]
            x_note_scale_mode: 0,
            #[cfg(feature = "launchpad-x")]
            x_note_scale: 0,
            #[cfg(feature = "launchpad-x")]
            x_note_root: 0,
            #[cfg(feature = "launchpad-x")]
            x_note_overlap: 5,
            #[cfg(feature = "launchpad-x")]
            x_note_channel: 0,
            aftertouch_mode: 0,
            aftertouch_curve: 0,
            palette: 0,
            mirror_enabled: 0,
            custom_palette: [[[0; 128]; 3]; 3],
        }
    }

    pub const fn defaults() -> Self {
        Self {
            brightness: 7,
            velocity_enabled: 0,
            velocity_floor: if cfg!(feature = "launchpad-x") { 5 } else { 0 },
            velocity_point_a: VELOCITY_POINT_NONE,
            velocity_point_b: VELOCITY_POINT_NONE,
            #[cfg(feature = "launchpad-x")]
            x_note_scale_mode: 0,
            #[cfg(feature = "launchpad-x")]
            x_note_scale: 0,
            #[cfg(feature = "launchpad-x")]
            x_note_root: 0,
            #[cfg(feature = "launchpad-x")]
            x_note_overlap: 5,
            #[cfg(feature = "launchpad-x")]
            x_note_channel: 0,
            aftertouch_mode: 0,
            aftertouch_curve: 0,
            palette: 0,
            mirror_enabled: 0,
            custom_palette: [[[0; 128]; 3]; 3],
        }
    }

    fn sanitize(&mut self) {
        if self.brightness > 7 {
            self.brightness = 7;
        }
        if self.velocity_enabled > 1 {
            self.velocity_enabled = 0;
        }
        self.velocity_floor = self.velocity_floor.min(7);
        self.velocity_point_a = self.velocity_point_a.min(VELOCITY_POINT_NONE);
        self.velocity_point_b = self.velocity_point_b.min(VELOCITY_POINT_NONE);
        #[cfg(feature = "launchpad-x")]
        {
            self.x_note_scale_mode = self.x_note_scale_mode.min(1);
            self.x_note_scale = self.x_note_scale.min(15);
            self.x_note_root = self.x_note_root.min(11);
            if !matches!(self.x_note_overlap, 0 | 2..=5) { self.x_note_overlap = 5; }
            self.x_note_channel = self.x_note_channel.min(15);
        }
        if self.aftertouch_mode > 2 {
            self.aftertouch_mode = 0;
        }
        if self.aftertouch_curve > 2 {
            self.aftertouch_curve = 0;
        }
        if self.palette > 6 {
            self.palette = 0;
        }
        if self.mirror_enabled > 1 {
            self.mirror_enabled = 0;
        }
    }

    fn fill_empty_custom_palettes(&mut self) -> bool {
        let mut changed = false;

        for slot in 0..3 {
            let is_zeroed = self.custom_palette[slot]
                .iter()
                .all(|channel| channel.iter().all(|value| *value == 0));
            let is_erased = self.custom_palette[slot]
                .iter()
                .all(|channel| channel.iter().all(|value| *value == 0xff));

            if !is_zeroed && !is_erased {
                continue;
            }

            for index in 0..128 {
                let (r, g, b) = PALETTE_NOVATION.rgb(index as u8);
                self.custom_palette[slot][0][index] = r;
                self.custom_palette[slot][1][index] = g;
                self.custom_palette[slot][2][index] = b;
            }
            changed = true;
        }

        changed
    }

    fn encode(&self, out: &mut [u8]) {
        out.fill(0);
        out[..SETTINGS_MAGIC.len()].copy_from_slice(&SETTINGS_MAGIC);
        out[SETTINGS_CRC_OFFSET..SETTINGS_DATA_OFFSET].fill(0);
        out[SETTINGS_DATA_OFFSET] = self.brightness;
        let (velocity_low, velocity_high) = crate::app::velocity::encode_settings(
            self.velocity_enabled, self.velocity_floor, self.velocity_point_a, self.velocity_point_b);
        out[SETTINGS_DATA_OFFSET + 1] = velocity_low;
        out[SETTINGS_DATA_OFFSET + 2] = velocity_high;
        out[SETTINGS_DATA_OFFSET + 3] = self.aftertouch_mode;
        out[SETTINGS_DATA_OFFSET + 4] = self.aftertouch_curve;
        out[SETTINGS_DATA_OFFSET + 5] = self.palette;
        out[SETTINGS_DATA_OFFSET + 6] = self.mirror_enabled;
        #[cfg(feature = "launchpad-x")]
        {
            // The marker in the formerly unused high bit of aftertouch_curve
            // distinguishes new images from old ones. Wire size and CRC stay
            // unchanged; lower bits of all original settings remain intact.
            let (brightness, mode, curve) = crate::app::note::wire::pack(
                self.brightness, self.aftertouch_mode, self.aftertouch_curve,
                crate::app::note::wire::NotePreferences {
                    scale_mode: self.x_note_scale_mode, scale: self.x_note_scale,
                    root: self.x_note_root, overlap: self.x_note_overlap,
                    channel: self.x_note_channel,
                });
            out[SETTINGS_DATA_OFFSET] = brightness;
            out[SETTINGS_DATA_OFFSET + 3] = mode;
            out[SETTINGS_DATA_OFFSET + 4] = curve;
        }

        let mut pos = PALETTE_OFFSET;
        for slot in self.custom_palette {
            for channel in slot {
                out[pos..pos + 128].copy_from_slice(&channel);
                pos += 128;
            }
        }

        let crc = crc32(&out[SETTINGS_DATA_OFFSET..SETTINGS_WIRE_SIZE]);
        out[SETTINGS_CRC_OFFSET..SETTINGS_DATA_OFFSET].copy_from_slice(&crc.to_le_bytes());
    }

    fn decode(input: &[u8]) -> Self {
        let mut settings = Self::defaults();

        settings.brightness = input[SETTINGS_DATA_OFFSET];
        (settings.velocity_enabled, settings.velocity_floor,
         settings.velocity_point_a, settings.velocity_point_b) = crate::app::velocity::decode_settings(
            input[SETTINGS_DATA_OFFSET + 1], input[SETTINGS_DATA_OFFSET + 2]);
        settings.aftertouch_mode = input[SETTINGS_DATA_OFFSET + 3];
        settings.aftertouch_curve = input[SETTINGS_DATA_OFFSET + 4];
        settings.palette = input[SETTINGS_DATA_OFFSET + 5];
        settings.mirror_enabled = input[SETTINGS_DATA_OFFSET + 6];
        #[cfg(feature = "launchpad-x")]
        if let Some((brightness, mode, curve, prefs)) = crate::app::note::wire::unpack(
            settings.brightness, settings.aftertouch_mode, settings.aftertouch_curve)
        {
            settings.brightness = brightness;
            settings.aftertouch_mode = mode;
            settings.aftertouch_curve = curve;
            settings.x_note_scale_mode = prefs.scale_mode;
            settings.x_note_scale = prefs.scale;
            settings.x_note_root = prefs.root;
            settings.x_note_overlap = prefs.overlap;
            settings.x_note_channel = prefs.channel;
        }

        if input.len() >= SETTINGS_WIRE_SIZE {
            let mut pos = PALETTE_OFFSET;
            for slot in 0..3 {
                for channel in 0..3 {
                    settings.custom_palette[slot][channel].copy_from_slice(&input[pos..pos + 128]);
                    pos += 128;
                }
            }
        }

        settings.sanitize();
        settings
    }
}

struct SettingsSlot {
    inner: UnsafeCell<Settings>,
}

unsafe impl Sync for SettingsSlot {}

static SETTINGS: SettingsSlot = SettingsSlot {
    inner: UnsafeCell::new(Settings::empty()),
};

pub fn get() -> Settings {
    unsafe { *SETTINGS.inner.get() }
}

pub fn with<R>(f: impl FnOnce(&Settings) -> R) -> R {
    unsafe { f(&*SETTINGS.inner.get()) }
}

pub fn update(f: impl FnOnce(&mut Settings)) {
    unsafe {
        let settings = &mut *SETTINGS.inner.get();
        f(settings);
        settings.sanitize();
    }
}

pub fn load() {
    // Device drivers may need to move settings away from storage owned by the
    // stock firmware before the normal logical settings mapping is accessed.
    driver::prepare_settings_storage(SETTINGS_WIRE_SIZE, validate_wire_image);

    let flash_size = driver::flash_size() as usize;
    let read_len = min_usize(
        min_usize(flash_size, SETTINGS_FLASH_SIZE),
        SETTINGS_WIRE_SIZE,
    );
    let mut buf = [0xff; SETTINGS_WIRE_SIZE];

    if read_len != 0 {
        driver::read_flash(0, &mut buf[..read_len]);
    }

    if read_len < SETTINGS_WIRE_SIZE || !validate_wire_image(&buf) {
        let mut settings = Settings::defaults();
        settings.fill_empty_custom_palettes();

        unsafe {
            *SETTINGS.inner.get() = settings;
        }
        driver::set_brightness(with(|settings| settings.brightness));
        save();
        return;
    }

    let mut settings = Settings::decode(&buf);
    let settings_changed = settings.fill_empty_custom_palettes();

    unsafe {
        *SETTINGS.inner.get() = settings;
    }
    driver::set_brightness(with(|settings| settings.brightness));

    if settings_changed {
        save();
    }
}

pub fn save() {
    let flash_size = driver::flash_size() as usize;
    let write_len = min_usize(
        min_usize(flash_size, SETTINGS_FLASH_SIZE),
        SETTINGS_WIRE_SIZE,
    );
    if write_len == 0 {
        return;
    }

    let mut buf = [0; SETTINGS_WIRE_SIZE];
    with(|settings| settings.encode(&mut buf));
    driver::write_flash(0, &buf[..write_len]);
}

const fn min_usize(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

fn validate_wire_image(input: &[u8]) -> bool {
    if input.len() < SETTINGS_WIRE_SIZE {
        return false;
    }

    if input[..SETTINGS_MAGIC.len()] != SETTINGS_MAGIC {
        return false;
    }

    let stored_crc = u32::from_le_bytes([
        input[SETTINGS_CRC_OFFSET],
        input[SETTINGS_CRC_OFFSET + 1],
        input[SETTINGS_CRC_OFFSET + 2],
        input[SETTINGS_CRC_OFFSET + 3],
    ]);
    let computed_crc = crc32(&input[SETTINGS_DATA_OFFSET..SETTINGS_WIRE_SIZE]);

    stored_crc == computed_crc
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;

    for byte in bytes {
        crc ^= *byte as u32;

        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }

    !crc
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Aezurolp
// Copyright (C) 2025-2026 Anthony Hofmeister

#[cfg(not(all(feature = "host-emulator", not(target_os = "none"))))]
use core::cell::UnsafeCell;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Rotation {
    Default,
    Left,
    Right,
    UpsideDown,
}

#[cfg(not(all(feature = "host-emulator", not(target_os = "none"))))]
struct RotationSlot {
    inner: UnsafeCell<Rotation>,
}

#[cfg(not(all(feature = "host-emulator", not(target_os = "none"))))]
unsafe impl Sync for RotationSlot {}

#[cfg(not(all(feature = "host-emulator", not(target_os = "none"))))]
static ROTATION: RotationSlot = RotationSlot {
    inner: UnsafeCell::new(Rotation::Default),
};

// The hosted tests run in parallel: isolate this runtime setting per runner
// while sharing the firmware's actual index mapping below.
#[cfg(all(feature = "host-emulator", not(target_os = "none")))]
std::thread_local! {
    static ROTATION: core::cell::Cell<Rotation> = const { core::cell::Cell::new(Rotation::Default) };
}

// Current rotation. Not persisted to flash.
pub fn get() -> Rotation {
    #[cfg(all(feature = "host-emulator", not(target_os = "none")))]
    { ROTATION.with(|rotation| rotation.get()) }
    #[cfg(not(all(feature = "host-emulator", not(target_os = "none"))))]
    unsafe { *ROTATION.inner.get() }
}

pub fn set(rotation: Rotation) {
    #[cfg(all(feature = "host-emulator", not(target_os = "none")))]
    ROTATION.with(|current| current.set(rotation));
    #[cfg(not(all(feature = "host-emulator", not(target_os = "none"))))]
    unsafe {
        *ROTATION.inner.get() = rotation;
    }
}

// Physical to logical index.
pub fn to_canonical(raw_index: u8) -> u8 {
    apply(raw_index, get(), false, false)
}

// Logical to physical index.
pub fn to_raw(canonical_index: u8) -> u8 {
    apply(canonical_index, get(), true, false)
}

// Physical -> logical index, grid only.
pub fn to_canonical_grid_only(raw_index: u8) -> u8 {
    apply(raw_index, get(), false, true)
}

// Logical -> physical index, grid only.
pub fn to_raw_grid_only(canonical_index: u8) -> u8 {
    apply(canonical_index, get(), true, true)
}

#[inline(never)]
fn apply(index: u8, rotation: Rotation, inverse: bool, grid_only: bool) -> u8 {
    if index == 99 || index >= 100 || rotation == Rotation::Default {
        return index;
    }

    let row = index / 10;
    let col = index % 10;

    let is_edge = col == 0 || col == 9 || row == 0 || row == 9;
    if is_edge {
        if (row == 0 && (col == 0 || col == 9)) || (row == 9 && col == 0) || grid_only {
            return index;
        }
    }

    let effective = match (rotation, inverse) {
        (Rotation::Left, false) | (Rotation::Right, true) => Rotation::Left,
        (Rotation::Right, false) | (Rotation::Left, true) => Rotation::Right,
        _ => Rotation::UpsideDown,
    };

    match effective {
        Rotation::Left => col * 10 + (9 - row),
        Rotation::Right => (9 - col) * 10 + row,
        Rotation::UpsideDown => 99 - index,
        _ => index,
    }
}

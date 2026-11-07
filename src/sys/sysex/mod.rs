// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub mod device_inquiry;
pub mod fastled;
pub mod led_control;
pub mod modes;
pub mod palette;
// Roadrunner queries M0 and flash through the hardware driver. Keep the
// firmware path unchanged; only the hosted emulator excludes that transport.
#[cfg(all(feature = "launchpad-pro-mk3", not(all(feature = "host-emulator", not(target_os = "none")))))]
pub mod roadrunner;
pub mod version_inquiry;

use crate::app::AppId;
use crate::sys::midi::MidiPort;

#[inline(never)]
pub fn execute(app: AppId, port: MidiPort, data: &[u8]) -> bool {
    #[cfg(all(feature = "launchpad-pro-mk3", not(all(feature = "host-emulator", not(target_os = "none")))))]
    if roadrunner::execute(app, port, data) {
        return true;
    }

    if device_inquiry::execute(app, port, data) {
        return true;
    }

    if app == AppId::Performance && (fastled::execute(app, port, data) || led_control::execute(app, port, data)) {
        return true;
    }

    if version_inquiry::execute(app, port, data) {
        return true;
    }

    if palette::execute(app, port, data) {
        return true;
    }

    false
}

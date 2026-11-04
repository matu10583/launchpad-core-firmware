// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub mod apptrait;
pub mod boot;
#[cfg(feature = "launchpad-pro-mk3")]
pub mod custom_note;
pub mod daw;
pub mod events;
#[cfg(any(feature = "launchpad-x", feature = "launchpad-pro-mk3"))]
pub mod guide;
pub mod host;
#[cfg(feature = "launchpad-x")]
pub mod note;
#[cfg(feature = "launchpad-pro-mk3")]
pub mod pro_note;
pub mod palette_editor;
pub mod performance;
pub mod programmer;
pub mod setup;
pub mod velocity;

pub use crate::sys::midi::MidiPort;
pub use apptrait::App;
pub use boot::{BootAnimationApp, BootApp};
pub use events::{AftertouchEvent, MidiEvent, SurfaceEvent};
pub use host::AppHost;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum AppId {
    Boot,
    Setup,
    Performance,
    #[cfg(any(feature = "launchpad-x", feature = "launchpad-pro-mk3"))]
    Note,
    Programmer,
    Daw,
    PaletteEditor,
    #[cfg(feature = "launchpad-pro-mk3")]
    CustomNote,
}

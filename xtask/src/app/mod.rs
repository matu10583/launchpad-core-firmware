pub use crate::note_guide as guide;
#[cfg(all(feature = "launchpad-x", feature = "launchpad-pro-mk3"))]
compile_error!("Build the emulator for exactly one model: use default features for X, or --no-default-features --features launchpad-pro-mk3 for Pro Mk3");
#[cfg(not(any(feature = "launchpad-x", feature = "launchpad-pro-mk3")))]
compile_error!("Select launchpad-x or launchpad-pro-mk3 for xtask host tests");

#[path = "../../../src/app/apptrait.rs"]
pub mod apptrait;
#[path = "../../../src/app/events.rs"]
pub mod events;
pub use apptrait::App;
pub use events::{AftertouchEvent, MidiEvent, SurfaceEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppId { Boot, Setup, Performance, Note, #[cfg(feature="launchpad-x")] XCustom, #[cfg(feature="launchpad-pro-mk3")] CustomNote, Programmer, Daw, PaletteEditor }

#[path = "../../../src/app/performance/mod.rs"]
pub mod performance;
#[path = "../../../src/app/host.rs"]
pub mod host;
#[path = "../../../src/app/boot/mod.rs"]
pub mod boot;
#[path = "../../../src/app/daw/mod.rs"]
pub mod daw;
#[path = "../../../src/app/programmer/mod.rs"]
pub mod programmer;
#[path = "../../../src/app/palette_editor/mod.rs"]
pub mod palette_editor;

#[path = "../../../src/app/velocity.rs"]
pub mod velocity;
pub mod setup;

#[path = "../../../src/app/note/mod.rs"]
pub mod note;
#[path = "../../../src/app/pro_note/mod.rs"]
pub mod pro_note;
#[path = "../../../src/app/custom_note/mod.rs"]
pub mod custom_note;
#[path = "../../../src/app/x_custom.rs"]
pub mod x_custom;

pub mod emulator;

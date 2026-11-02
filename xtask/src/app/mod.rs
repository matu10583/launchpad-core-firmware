pub use crate::note_guide as guide;

#[path = "../../../src/app/apptrait.rs"]
pub mod apptrait;
#[path = "../../../src/app/events.rs"]
pub mod events;
pub use apptrait::App;
pub use events::{AftertouchEvent, MidiEvent, SurfaceEvent};

pub enum AppId { Note }

pub mod performance {
    pub(crate) fn apply_velocity_curve(_curve: u8, value: u8) -> u8 { value }
}

#[path = "../../../src/app/note/mod.rs"]
pub mod note;
#[path = "../../../src/app/pro_note/mod.rs"]
pub mod pro_note;
#[path = "../../../src/app/custom_note/mod.rs"]
pub mod custom_note;

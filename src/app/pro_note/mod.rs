// SPDX-License-Identifier: GPL-3.0-only
//! Pro MK3 Note mode (standalone USB MIDI). Drum Rack detection needs DAW integration.

use crate::app::performance::apply_velocity_curve;
use crate::app::{AftertouchEvent, App, MidiEvent, SurfaceEvent};
use crate::app::guide::{self, Command as GuideCommand, Guide};
use crate::driver;
use crate::sys::midi::MidiPort;
use crate::sys::{led, settings};

// Scale names/order match the official Note settings display (two rows of eight).
const SCALES: [&[u8]; 16] = [
    &[0, 2, 3, 5, 7, 8, 10],       // Natural Minor (default)
    &[0, 2, 4, 5, 7, 9, 11],       // Natural Major
    &[0, 2, 3, 5, 7, 9, 10],       // Dorian
    &[0, 1, 3, 5, 7, 8, 10],       // Phrygian
    &[0, 2, 4, 5, 7, 9, 10],       // Mixolydian
    &[0, 2, 3, 5, 7, 9, 11],       // Melodic Minor
    &[0, 2, 3, 5, 7, 8, 11],       // Harmonic Minor
    &[0, 2, 3, 4, 5, 7, 9, 10],    // Bebop Dorian
    &[0, 3, 5, 6, 7, 10],          // Blues
    &[0, 3, 5, 7, 10],             // Minor Pentatonic
    &[0, 2, 3, 6, 7, 8, 11],       // Hungarian Minor
    &[0, 2, 3, 6, 7, 9, 10],       // Ukrainian Dorian
    &[0, 1, 4, 6, 7, 9, 11],       // Marva
    &[0, 1, 3, 6, 7, 8, 11],       // Todi
    &[0, 2, 4, 6, 8, 10],          // Whole Tone
    &[0, 2, 3, 7, 8],              // Hirajoshi
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadColor {
    Rgb(u32),
    Novation(u8),
}

pub struct NoteApp {
    semitones: i16,
    octaves: i16,
    channel: u8, // MIDI channels are zero based internally.
    root: u8,
    scale: usize,
    overlap: u8, // 0 = sequential; otherwise row stride in notes/scale degrees.
    scale_layout: bool,
    settings_view: bool,
    held: [Option<u8>; 64],
    external: [bool; 128],
    preview: [u8; 128],
    current: [u8; 128],
    pad_preview: [u8; 64],
    pad_current: [u8; 64],
    guide: Guide,
}

impl NoteApp {
    pub const fn new() -> Self {
        Self {
            semitones: 0, octaves: 0, channel: 0, root: 0, scale: 0,
            overlap: 5, scale_layout: false, settings_view: false,
            held: [None; 64], external: [false; 128],
            preview: [0; 128], current: [0; 128],
            pad_preview: [0; 64], pad_current: [0; 64],
            guide: Guide::new(),
        }
    }

    pub fn show_settings(&mut self, show: bool) {
        if show != self.settings_view {
            self.release_all();
            self.settings_view = show;
            led::clear();
            self.draw();
        }
    }

    fn offset(&self) -> i16 { self.octaves * 12 + self.semitones }

    fn slot(index: u8) -> Option<usize> {
        let row = index / 10;
        let col = index % 10;
        if (1..=8).contains(&row) && (1..=8).contains(&col) {
            Some(((row - 1) * 8 + col - 1) as usize)
        } else { None }
    }

    fn degree(&self, index: u8) -> Option<usize> {
        let slot = Self::slot(index)?;
        let row = slot / 8;
        let col = slot % 8;
        Some(row * (if self.overlap == 0 { 8 } else { self.overlap as usize }) + col)
    }

    fn pitch(&self, index: u8) -> Option<u8> {
        let degree = self.degree(index)?;
        let relative = if self.scale_layout {
            let scale = SCALES[self.scale];
            12 * (degree / scale.len()) as i16 + scale[degree % scale.len()] as i16
        } else {
            degree as i16
        };
        let pitch = 48 + self.root as i16 + self.offset() + relative;
        u8::try_from(pitch).ok().filter(|&p| p <= 127)
    }

    fn held_count(&self, pitch: u8) -> usize {
        self.held.iter().filter(|&&note| note == Some(pitch)).count()
    }

    fn in_scale(&self, pitch: u8) -> bool {
        SCALES[self.scale].contains(&((pitch as i16 - self.root as i16).rem_euclid(12) as u8))
    }

    fn clear_pad_feedback(&mut self) {
        self.pad_preview.fill(0);
        self.pad_current.fill(0);
    }

    fn clear_legacy_feedback(&mut self) {
        self.external.fill(false);
        self.preview.fill(0);
        self.current.fill(0);
        self.clear_pad_feedback();
    }

    fn guide_reply(&self, payload: &[u8]) {
        let mut response = [0u8; 96];
        if let Some(len) = guide::reply(payload, guide::MODEL_PRO_MK3, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
    }

    pub fn receive_guide_sysex(&mut self, data: &[u8]) -> bool {
        let Some(command) = guide::decode(data, guide::MODEL_PRO_MK3) else { return false };
        match command {
            GuideCommand::Query => self.guide_reply(&[0x41, 0x07, 1, self.guide.active() as u8]),
            GuideCommand::Layout => {
                let payload = guide::pro_layout(self.scale_layout as u8, self.root,
                    self.scale as u8, self.overlap, self.offset(), |index| self.pitch(index));
                self.guide_reply(&payload);
            }
            GuideCommand::Begin => {
                self.clear_legacy_feedback();
                self.guide.begin();
                self.draw();
                self.guide_reply(&[0x50, 0x10, 1]);
            }
            GuideCommand::Keepalive => {
                if self.guide.active() {
                    self.guide.renew();
                    self.guide_reply(&[0x50, 0x11, 1]);
                }
            }
            GuideCommand::End => {
                self.guide.end();
                self.clear_legacy_feedback();
                self.draw();
                self.guide_reply(&[0x50, 0x12, 0]);
            }
            GuideCommand::Set { layer, side, pitch, color } => {
                if self.guide.set(layer, side, pitch, color) && !self.settings_view { self.draw(); }
            }
        }
        true
    }

    fn color(&self, index: u8) -> PadColor {
        let Some(slot) = Self::slot(index) else { return PadColor::Rgb(0) };
        if let Some(color) = self.guide.pad_color(index, self.pitch(index), self.held[slot].is_some()) {
            return PadColor::Novation(color);
        }
        if self.pad_current[slot] != 0 {
            return PadColor::Novation(self.pad_current[slot]);
        }
        // A new hit on a different pad must remain visible while the old
        // hit's pitch-wide ch3 current indication is still active.
        if self.pad_preview[slot] != 0 && self.held[slot].is_none() {
            return PadColor::Novation(self.pad_preview[slot]);
        }
        let Some(p) = self.pitch(index) else { return PadColor::Rgb(0) };
        if self.current[p as usize] != 0 {
            PadColor::Novation(self.current[p as usize])
        } else if self.held_count(p) != 0 {
            PadColor::Rgb(0xffffff)
        } else if self.preview[p as usize] != 0 {
            PadColor::Novation(self.preview[p as usize])
        } else if self.external[p as usize] {
            PadColor::Rgb(0xffffff)
        } else if (p as i16 - self.root as i16).rem_euclid(12) == 0 {
            PadColor::Rgb(0x903ec0)
        } else if self.in_scale(p) {
            PadColor::Rgb(0x207ac0)
        } else {
            PadColor::Rgb(0x080808)
        }
    }

    fn receive_feedback(&mut self, event: MidiEvent) -> bool {
        if event.port != MidiPort::Midi || event.data1 > 127 || event.data2 > 127 {
            return false;
        }
        let value = match event.status & 0xf0 {
            0x90 => event.data2,
            0x80 => 0,
            _ => return false,
        };
        let channel = event.status & 0x0f;
        match channel {
            0 => self.external[event.data1 as usize] = value != 0,
            1 => self.preview[event.data1 as usize] = value,
            2 => self.current[event.data1 as usize] = value,
            3 | 4 => {
                let Some(slot) = Self::slot(event.data1) else { return false };
                if channel == 3 {
                    self.pad_preview[slot] = value;
                } else {
                    self.pad_current[slot] = value;
                }
            }
            // Preserve the old selected-channel feedback for channels 6–16.
            // Channels 2–5 are reserved for the same LED protocol as X.
            _ if channel == self.channel => self.external[event.data1 as usize] = value != 0,
            _ => return false,
        }
        true
    }

    fn draw(&self) {
        if self.settings_view { self.draw_settings(); return; }
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                match self.color(index) {
                    PadColor::Rgb(rgb) => led::set(index, rgb),
                    PadColor::Novation(color) => led::novation(index, color),
                }
            }
        }
        for index in [80, 70, 91, 92] { led::set(index, 0x345060); }
        led::set(94, 0x90d0a0);
        led::set(96, 0x303030);
        led::set(93, 0x303030);
    }

    fn draw_settings(&self) {
        for index in [82, 83, 84, 85, 86] {
            let selected = if self.overlap == 0 { index == 82 } else { index == 81 + self.overlap };
            led::set(index, if selected { 0xffc070 } else { 0x303030 });
        }
        led::set(88, if self.scale_layout { 0x00dd65 } else { 0x8a2424 });
        // Piano-style root viewer, C-D-E-F-G-A-B on row 6; sharps on row 7.
        for (index, pitch) in [(62, 0), (63, 2), (64, 4), (65, 5), (66, 7),
                               (67, 9), (68, 11), (73, 1), (74, 3), (76, 6),
                               (77, 8), (78, 10)] {
            led::set(index, if pitch == self.root { 0x9635c8 }
                     else if self.in_scale(pitch) { 0x287ccc } else { 0x242424 });
        }
        for i in 0..16u8 {
            let index = if i < 8 { 51 + i } else { 41 + i - 8 };
            led::set(index, if i as usize == self.scale { 0xe4e4e4 } else { 0x143655 });
            let channel_index = if i < 8 { 31 + i } else { 21 + i - 8 };
            led::set(channel_index, if i == self.channel { 0xc9df4c } else { 0x46491e });
        }
        led::set(94, 0x90d0a0);
        led::set(96, 0x303030);
        led::set(93, 0x303030);
    }

    fn configure(&mut self, index: u8) {
        match index {
            82 => self.overlap = 0,
            83..=86 => self.overlap = index - 81,
            88 => self.scale_layout = !self.scale_layout,
            62..=68 => self.root = [0, 2, 4, 5, 7, 9, 11][(index - 62) as usize],
            73 | 74 | 76..=78 => self.root = match index {
                73 => 1, 74 => 3, 76 => 6, 77 => 8, _ => 10,
            },
            51..=58 => self.scale = (index - 51) as usize,
            41..=48 => self.scale = (index - 41 + 8) as usize,
            31..=38 => {
                self.channel = index - 31;
                self.external = [false; 128];
            }
            21..=28 => {
                self.channel = index - 21 + 8;
                self.external = [false; 128];
            }
            _ => return,
        }
        self.clear_pad_feedback();
        self.draw();
    }

    fn release_all(&mut self) {
        for pitch in 0..=127u8 {
            if self.held_count(pitch) != 0 {
                driver::send_midi(MidiPort::Midi, &[0x90 | self.channel, pitch, 0]);
            }
        }
        self.held = [None; 64];
    }

    fn shift(&mut self, amount: i16) {
        let next = self.offset() + amount;
        if !(-100..=60).contains(&next) { return; }
        if amount.abs() == 12 { self.octaves += amount / 12; }
        else { self.semitones += amount; }
        self.clear_pad_feedback();
        self.draw();
    }
}

impl App for NoteApp {
    fn on_enter(&mut self) {
        self.guide.end();
        self.clear_legacy_feedback();
        self.draw();
    }
    fn on_exit(&mut self) {
        self.release_all();
        self.guide.end();
        self.clear_legacy_feedback();
        self.settings_view = false;
    }
    fn on_surface(&mut self, event: SurfaceEvent) {
        if self.settings_view {
            if event.pressed { self.configure(event.index); }
            return;
        }
        if event.pressed {
            match event.index {
                80 => { self.shift(12); return; }
                70 => { self.shift(-12); return; }
                91 => { self.shift(-1); return; }
                92 => { self.shift(1); return; }
                _ => {}
            }
        }
        let Some(slot) = Self::slot(event.index) else { return; };
        if event.pressed {
            if self.held[slot].is_some() { return; }
            let Some(pitch) = self.pitch(event.index) else { return; };
            let first = self.held_count(pitch) == 0;
            self.held[slot] = Some(pitch);
            if first {
                let velocity = settings::with(|s| if s.velocity_enabled != 0 {
                    apply_velocity_curve(s.velocity_curve, event.value).max(1)
                } else { 127 });
                driver::send_midi(MidiPort::Midi, &[0x90 | self.channel, pitch, velocity]);
            }
        } else if let Some(pitch) = self.held[slot].take() {
            if self.held_count(pitch) == 0 {
                driver::send_midi(MidiPort::Midi, &[0x90 | self.channel, pitch, 0]);
            }
        }
        self.draw();
    }
    fn on_midi(&mut self, event: MidiEvent) {
        if self.guide.active() && event.port == MidiPort::Midi
            && (event.status & 0x0f) < 5 { return; }
        if self.receive_feedback(event) && !self.settings_view { self.draw(); }
    }
    fn on_aftertouch(&mut self, event: AftertouchEvent) {
        let Some(slot) = Self::slot(event.index) else { return; };
        let Some(pitch) = self.held[slot] else { return; };
        match settings::with(|s| s.aftertouch_mode) {
            1 => driver::send_midi(MidiPort::Midi, &[0xa0 | self.channel, pitch, event.value]),
            2 => driver::send_midi(MidiPort::Midi, &[0xd0 | self.channel, event.value]),
            _ => {}
        }
    }
    fn on_tick(&mut self) {
        if self.guide.tick() {
            self.clear_legacy_feedback();
            self.draw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn midi(port: MidiPort, status: u8, index: u8, value: u8) -> MidiEvent {
        MidiEvent { port, status, data1: index, data2: value }
    }

    #[test]
    fn default_chromatic_uses_five_finger_overlap_and_c_minor() {
        let note = NoteApp::new();
        assert_eq!(note.pitch(11), Some(48));
        assert_eq!(note.pitch(16), note.pitch(21));
        assert!(note.in_scale(51));
        assert!(!note.in_scale(52));
    }

    #[test]
    fn scale_and_channel_are_independent_of_custom() {
        let mut note = NoteApp::new();
        note.scale_layout = true;
        note.channel = 15;
        assert_eq!(note.pitch(12), Some(50));
        assert_eq!(note.pitch(13), Some(51));
        assert_eq!(note.pitch(16), note.pitch(21));
        note.overlap = 0;
        assert_ne!(note.pitch(16), note.pitch(21));
    }

    #[test]
    fn pitch_and_pad_colors_are_independent_on_duplicate_notes() {
        let mut note = NoteApp::new();
        let pitch = note.pitch(16).unwrap();
        assert_eq!(note.pitch(21), Some(pitch));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x90, pitch, 100)));
        assert_eq!(note.color(16), PadColor::Rgb(0xffffff));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x91, pitch, 13)));
        assert_eq!(note.color(16), PadColor::Novation(13));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x92, pitch, 42)));
        assert_eq!(note.color(21), PadColor::Novation(42));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x93, 21, 5)));
        assert_eq!(note.color(16), PadColor::Novation(42));
        assert_eq!(note.color(21), PadColor::Novation(5));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x94, 16, 127)));
        assert_eq!(note.color(16), PadColor::Novation(127));
        assert_eq!(note.color(21), PadColor::Novation(5));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x84, 16, 0)));
        assert_eq!(note.color(16), PadColor::Novation(42));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x83, 21, 0)));
        assert_eq!(note.color(21), PadColor::Novation(42));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x82, pitch, 0)));
        assert_eq!(note.color(16), PadColor::Novation(13));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x81, pitch, 0)));
        assert_eq!(note.color(16), PadColor::Rgb(0xffffff));
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x80, pitch, 0)));
        assert_ne!(note.color(16), PadColor::Rgb(0xffffff));
    }

    #[test]
    fn pad_feedback_respects_local_press_and_rejects_non_grid_indices() {
        let mut note = NoteApp::new();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                assert!(note.receive_feedback(midi(MidiPort::Midi, 0x93, index, 1)));
                assert_eq!(note.color(index), PadColor::Novation(1));
            }
        }
        for index in 0..=127 {
            if NoteApp::slot(index).is_none() {
                assert!(!note.receive_feedback(midi(MidiPort::Midi, 0x94, index, 1)));
            }
        }
        for event in [
            midi(MidiPort::Daw, 0x94, 11, 10),
            midi(MidiPort::Midi, 0x95, 11, 10),
            midi(MidiPort::Midi, 0xb4, 11, 10),
            midi(MidiPort::Midi, 0x94, 11, 128),
        ] {
            assert!(!note.receive_feedback(event));
        }
        note.held[0] = note.pitch(11);
        assert_eq!(note.color(11), PadColor::Rgb(0xffffff));
        note.receive_feedback(midi(MidiPort::Midi, 0x94, 11, 127));
        assert_eq!(note.color(11), PadColor::Novation(127));
        note.held[0] = None;
        note.receive_feedback(midi(MidiPort::Midi, 0x94, 11, 0));
        assert_eq!(note.color(11), PadColor::Novation(1));
    }

    #[test]
    fn selected_channel_feedback_only_on_non_reserved_channels() {
        let mut note = NoteApp::new();
        note.channel = 6; // ch7 keeps the original selected-channel feedback.
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x96, 48, 99)));
        assert!(note.external[48]);
        note.channel = 3; // ch4 is reserved for pad previews, not pitch feedback.
        assert!(!note.receive_feedback(midi(MidiPort::Midi, 0x93, 49, 99)));
        assert!(!note.external[49]);
        assert!(note.receive_feedback(midi(MidiPort::Midi, 0x93, 11, 99)));
        assert_eq!(note.pad_preview[0], 99);
    }

    #[test]
    fn layout_changes_clear_pad_layers_but_pitch_layers_follow_the_note() {
        let mut note = NoteApp::new();
        note.receive_feedback(midi(MidiPort::Midi, 0x92, 48, 12));
        note.receive_feedback(midi(MidiPort::Midi, 0x93, 11, 13));
        note.clear_pad_feedback(); // configure() and shift() call this before redrawing.
        assert_eq!(note.color(11), PadColor::Novation(12));
        note.overlap = 0;
        note.octaves = 1;
        assert_ne!(note.color(11), PadColor::Novation(12));
        note.octaves = 0;
        assert_eq!(note.color(11), PadColor::Novation(12));
        note.receive_feedback(midi(MidiPort::Midi, 0x94, 11, 21));
        note.on_exit();
        assert_eq!(note.pad_current[0], 0);
        assert_eq!(note.pad_preview[0], 0);
        assert_ne!(note.color(11), PadColor::Novation(12));
    }

    #[test]
    fn private_guide_follows_note_layout_and_clears_on_exit() {
        let mut note = NoteApp::new();
        note.guide.begin();
        note.guide.set(0, 0, 48, 13);
        assert_eq!(note.color(11), PadColor::Novation(13));
        assert_ne!(note.color(15), PadColor::Novation(13));
        note.semitones = -5;
        assert_eq!(note.color(21), PadColor::Novation(13));
        note.semitones = 0;
        note.held[0] = Some(48);
        assert_ne!(note.color(11), PadColor::Novation(13));
        note.guide.set(1, 0, 48, 21);
        assert_eq!(note.color(11), PadColor::Novation(21));
        note.on_exit();
        assert!(!note.guide.active());
        assert_ne!(note.color(11), PadColor::Novation(21));
    }
}

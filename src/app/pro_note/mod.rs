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
    guide: Guide,
}

impl NoteApp {
    pub const fn new() -> Self {
        Self {
            semitones: 0, octaves: 0, channel: 0, root: 0, scale: 0,
            overlap: 5, scale_layout: false, settings_view: false,
            held: [None; 64],
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

    fn guide_reply(&self, payload: &[u8]) {
        let mut response = [0u8; 96];
        if let Some(len) = guide::reply(payload, guide::MODEL_PRO_MK3, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
    }

    pub fn receive_guide_sysex(&mut self, data: &[u8]) -> bool {
        let Some(command) = guide::decode(data, guide::MODEL_PRO_MK3) else { return false };
        match command {
            GuideCommand::Query => self.guide_reply(&[0x41, 0x03, 1, self.guide.active() as u8]),
            GuideCommand::Layout => {
                let payload = guide::pro_layout(self.scale_layout as u8, self.root,
                    self.scale as u8, self.overlap, self.offset(), |index| self.pitch(index));
                self.guide_reply(&payload);
            }
            GuideCommand::Begin => {
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
        if let Some(color) = self.guide.pad_color(index, self.pitch(index), self.held[slot].is_some(),
            |side, pitch| guide::side_has_pitch(side, pitch, |pad| self.pitch(pad))) {
            return PadColor::Novation(color);
        }
        let Some(p) = self.pitch(index) else { return PadColor::Rgb(0) };
        if self.held_count(p) != 0 {
            PadColor::Rgb(0xffffff)
        } else if (p as i16 - self.root as i16).rem_euclid(12) == 0 {
            PadColor::Rgb(0x903ec0)
        } else if self.in_scale(p) {
            PadColor::Rgb(0x207ac0)
        } else {
            PadColor::Rgb(0x080808)
        }
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
            }
            21..=28 => {
                self.channel = index - 21 + 8;
            }
            _ => return,
        }
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
        self.draw();
    }
}

impl App for NoteApp {
    fn on_enter(&mut self) {
        self.guide.end();
        self.draw();
    }
    fn on_exit(&mut self) {
        self.release_all();
        self.guide.end();
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
    fn on_midi(&mut self, _event: MidiEvent) {}
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
    fn note_layout_and_channel_settings_remain_available() {
        let mut note = NoteApp::new();
        assert_eq!(note.pitch(11), Some(48));
        assert_eq!(note.pitch(16), note.pitch(21));
        note.scale_layout = true;
        note.channel = 15;
        assert_eq!(note.pitch(12), Some(50));
        assert_eq!(note.pitch(13), Some(51));
        note.overlap = 0;
        assert_ne!(note.pitch(16), note.pitch(21));
    }

    #[test]
    fn incoming_note_feedback_never_lights_pads() {
        let mut note = NoteApp::new();
        note.channel = 6;
        let base = note.color(11);
        for session in 0..3 {
            if session == 1 { note.guide.begin(); }
            if session == 2 { note.guide.end(); }
            for channel in [0, 1, 2, 3, 4, 6] {
                for port in [MidiPort::Midi, MidiPort::Daw] {
                    note.on_midi(midi(port, 0x90 | channel, 48, 127));
                    note.on_midi(midi(port, 0x90 | channel, 11, 127));
                    note.on_midi(midi(port, 0x80 | channel, 48, 0));
                    assert_eq!(note.color(11), base);
                }
            }
        }
        note.held[0] = Some(48);
        assert_eq!(note.color(11), PadColor::Rgb(0xffffff));
        note.on_midi(midi(MidiPort::Midi, 0x80, 48, 0));
        assert_eq!(note.color(11), PadColor::Rgb(0xffffff));
    }

    #[test]
    fn guide_fallback_tracks_layout_and_held_priority() {
        let mut note = NoteApp::new();
        note.guide.begin();
        // 48 starts on the left only. When shifted, 53 is on both halves.
        note.guide.set(0, 1, 48, 21);
        assert_eq!(note.color(11), PadColor::Novation(21));
        note.held[0] = Some(48);
        assert_eq!(note.color(11), PadColor::Rgb(0xffffff));
        note.guide.set(1, 1, 48, 31);
        assert_eq!(note.color(11), PadColor::Novation(31));
        note.held[0] = None;
        note.guide.set(1, 1, 48, 0);
        note.guide.set(0, 1, 48, 0);
        note.guide.set(0, 1, 53, 21);
        assert_eq!(note.color(16), PadColor::Novation(21));
        assert_ne!(note.color(21), PadColor::Novation(21));
        note.guide.set(0, 0, 53, 13);
        assert_eq!(note.color(21), PadColor::Novation(13));
        note.guide.set(1, 1, 53, 31);
        assert_eq!(note.color(16), PadColor::Novation(31));
        assert_eq!(note.color(21), PadColor::Novation(13));
        note.guide.set(1, 1, 53, 0);
        note.overlap = 0;
        assert_eq!(note.color(16), PadColor::Novation(21));
        assert_ne!(note.color(21), PadColor::Novation(21));
        note.guide.set(0, 0, 48, 13);
        note.octaves = 1;
        assert_ne!(note.color(11), PadColor::Novation(13)); // pitch 48 left the grid
        note.on_exit();
        assert!(!note.guide.active());
    }

    #[test]
    fn guide_model_and_malformed_frames_are_ignored_without_side_effects() {
        let mut note = NoteApp::new();
        let mut frame = [0; 16];
        let n = guide::reply(&[0x10], guide::MODEL_X, &mut frame).unwrap();
        assert!(!note.receive_guide_sysex(&frame[..n]));
        assert!(!note.guide.active());
        let n = guide::reply(&[0x10], guide::MODEL_PRO_MK3, &mut frame).unwrap();
        frame[6] = 2;
        assert!(!note.receive_guide_sysex(&frame[..n]));
        frame[6] = 1;
        assert!(!note.receive_guide_sysex(&frame[..n - 1]));
        assert!(note.receive_guide_sysex(&frame[..n]));
        assert!(note.guide.active());
    }

    #[test]
    fn playing_still_sends_on_selected_channel() {
        let mut note = NoteApp::new();
        note.channel = 6;
        crate::driver::take_midi();
        note.on_surface(SurfaceEvent { index: 11, pressed: true, value: 100 });
        note.on_surface(SurfaceEvent { index: 11, pressed: false, value: 0 });
        assert_eq!(crate::driver::take_midi(), vec![
            (MidiPort::Midi, vec![0x96, 48, 127]),
            (MidiPort::Midi, vec![0x96, 48, 0]),
        ]);
    }
}

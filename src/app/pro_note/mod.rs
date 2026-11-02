// SPDX-License-Identifier: GPL-3.0-only
//! Pro MK3 Note mode (standalone USB MIDI). Drum Rack detection needs DAW integration.

use crate::app::performance::apply_velocity_curve;
use crate::app::{AftertouchEvent, App, MidiEvent, SurfaceEvent};
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
}

impl NoteApp {
    pub const fn new() -> Self {
        Self {
            semitones: 0, octaves: 0, channel: 0, root: 0, scale: 0,
            overlap: 5, scale_layout: false, settings_view: false,
            held: [None; 64], external: [false; 128],
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

    fn draw(&self) {
        if self.settings_view { self.draw_settings(); return; }
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                let color = match self.pitch(index) {
                    None => 0,
                    Some(p) if self.held_count(p) != 0 || self.external[p as usize] => 0xffffff,
                    Some(p) if (p as i16 - self.root as i16).rem_euclid(12) == 0 => 0x903ec0,
                    Some(p) if self.in_scale(p) => 0x207ac0,
                    Some(_) => 0x080808,
                };
                led::set(index, color);
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
    fn on_enter(&mut self) { self.draw(); }
    fn on_exit(&mut self) {
        self.release_all();
        self.external = [false; 128];
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
        if event.port != MidiPort::Midi || event.status & 0x0f != self.channel || event.data1 > 127 { return; }
        if matches!(event.status & 0xf0, 0x80 | 0x90) {
            self.external[event.data1 as usize] = event.status & 0xf0 == 0x90 && event.data2 != 0;
            if !self.settings_view { self.draw(); }
        }
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
    fn on_tick(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

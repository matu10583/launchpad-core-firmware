// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

mod layout;
mod transpose;

use crate::app::apptrait::App;
use crate::app::events::{AftertouchEvent, MidiEvent, SurfaceEvent};
use crate::app::performance::apply_velocity_curve;
use crate::driver;
use crate::sys::led;
use crate::sys::midi::MidiPort;
use crate::sys::settings;

use layout::{base_color, grid_slot, note_for, Preset};
use transpose::Transpose;
use crate::app::guide::{self, Command as GuideCommand, Guide};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadColor {
    Rgb(u32),
    Novation(u8),
}

pub struct NoteApp {
    preset: Preset,
    transpose: Transpose,
    up_held: bool,
    down_held: bool,
    left_held: bool,
    right_held: bool,
    choosing: bool,
    // Latch the actual sounding note so releasing a pad cannot turn off a
    // different pitch after a layout or transpose change.
    active: [Option<u8>; 64],
    guide: Guide,
}

impl NoteApp {
    pub const fn new() -> Self {
        Self {
            preset: Preset::Chromatic,
            transpose: Transpose::new(),
            up_held: false,
            down_held: false,
            left_held: false,
            right_held: false,
            choosing: false,
            active: [None; 64],
            guide: Guide::new(),
        }
    }

    fn release_all(&mut self) {
        // One Note On per pitch, even when two pads map to the same pitch.
        for slot in 0..self.active.len() {
            if let Some(note) = self.active[slot].take() {
                if !self.active.contains(&Some(note)) {
                    driver::send_midi(MidiPort::Midi, &[0x90, note, 0]);
                }
            }
        }
    }

    fn guide_reply(&self, payload: &[u8]) {
        let mut response = [0u8; 96];
        if let Some(len) = guide::reply(payload, guide::MODEL_X, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
    }

    // Private prototype: only AppHost's LPX (MIDI) / Note route calls this.
    // No Novation or universal SysEx identifier is reused.
    pub fn receive_guide_sysex(&mut self, data: &[u8]) -> bool {
        let Some(command) = guide::decode(data, guide::MODEL_X) else { return false };
        match command {
            GuideCommand::Query => {
                // Read-only: protocol capability bits, Note mode, session state.
                self.guide_reply(&[0x41, 0x03, 1, self.guide.active() as u8]);
            }
            GuideCommand::Layout => {
                // 64 validity bits packed into ten 7-bit bytes, then 64 pitches.
                let mut payload = [0u8; 77];
                payload[0] = 0x42;
                payload[1] = self.preset.slot();
                payload[2] = (self.transpose.total() + 64) as u8;
                for slot in 0..64 {
                    let index = (slot / 8 + 1) as u8 * 10 + (slot % 8 + 1) as u8;
                    if let Some(pitch) = note_for(self.preset, index, self.transpose.total()) {
                        payload[3 + slot / 7] |= 1 << (slot % 7);
                        payload[13 + slot] = pitch;
                    }
                }
                // 77-byte payload + 8-byte private framing = 85 bytes,
                // within the USB MIDI TX queue's 32 packets.
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
                if self.guide.set(layer, side, pitch, color) && !self.choosing {
                    self.draw();
                }
            }
        }
        true
    }

    fn pad_color(&self, note: u8) -> PadColor {
        let played_color = if (note as i16 - self.transpose.total()).rem_euclid(12) == 0 {
            PadColor::Rgb(0xffa2ea)
        } else {
            PadColor::Rgb(0x9beaff)
        };
        if self.active.contains(&Some(note)) {
            played_color
        } else {
            PadColor::Rgb(base_color(note, self.transpose.total()))
        }
    }

    fn grid_color(&self, index: u8) -> PadColor {
        let Some(slot) = grid_slot(index) else { return PadColor::Rgb(0) };
        if let Some(color) = self.guide.pad_color(
            index,
            note_for(self.preset, index, self.transpose.total()),
            self.active[slot].is_some(),
            |side, pitch| guide::side_has_pitch(side, pitch,
                |pad| note_for(self.preset, pad, self.transpose.total())),
        ) {
            return PadColor::Novation(color);
        }
        note_for(self.preset, index, self.transpose.total())
            .map_or(PadColor::Rgb(0), |note| self.pad_color(note))
    }

    fn set_pad(index: u8, color: PadColor) {
        match color {
            PadColor::Rgb(rgb) => led::set(index, rgb),
            PadColor::Novation(color) => led::novation(index, color),
        }
    }

    fn refresh_pitch(&self, pitch: u8) {
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                if note_for(self.preset, index, self.transpose.total()) == Some(pitch) {
                    Self::set_pad(index, self.grid_color(index));
                }
            }
        }
    }

    fn draw(&self) {
        led::clear();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                let color = if self.choosing {
                    if row == 1 && col <= 3 {
                        let slot = col - 1;
                        PadColor::Rgb(if self.preset.slot() == slot { 0xff47ce } else { 0x239fd1 })
                    } else {
                        PadColor::Rgb(0)
                    }
                } else {
                    self.grid_color(index)
                };
                Self::set_pad(index, color);
            }
        }

        for (index, step) in [(91, 12), (92, -12), (93, -1), (94, 1)] {
            let disabled = self.choosing || self.transpose.shifted(step).is_none();
            let color = if !self.choosing && step == 12 && self.transpose.octaves > 0 {
                0x40e8ff // one press upwards (maximum is +1 octave)
            } else if !self.choosing && step == -12 && self.transpose.octaves < 0 {
                if self.transpose.octaves <= -2 { 0x00eaff } else { 0x146778 }
            } else if disabled {
                0x102020
            } else if step.abs() == 12 {
                0x16434c // neutral octave buttons
            } else {
                0xc039c2
            };
            led::set_raw(index, color);
        }
        led::set_raw(95, 0x401010); // Hold Session to enter Setup.
        led::set_raw(96, 0xff47ce); // Note mode.
        led::set_raw(97, if self.choosing { 0xff47ce } else { 0x239fd1 }); // Preset selector.
    }

    fn change_transpose(&mut self, next: Transpose) {
        self.release_all();
        self.transpose = next;
        self.draw();
    }

    fn set_transpose(&mut self, step: i16) {
        if let Some(next) = self.transpose.shifted(step) {
            self.change_transpose(next);
        }
    }

    fn transpose_button(&mut self, index: u8, pressed: bool) {
        let held = match index {
            91 => &mut self.up_held,
            92 => &mut self.down_held,
            93 => &mut self.left_held,
            94 => &mut self.right_held,
            _ => return,
        };
        let was_held = *held;
        *held = pressed;
        if !pressed || was_held || self.choosing {
            return;
        }
        if (index == 91 || index == 92) && self.up_held && self.down_held {
            self.change_transpose(self.transpose.without_octaves());
        } else if (index == 93 || index == 94) && self.left_held && self.right_held {
            self.change_transpose(self.transpose.without_semitones());
        } else {
            self.set_transpose(match index { 91 => 12, 92 => -12, 93 => -1, _ => 1 });
        }
    }
}

impl App for NoteApp {
    fn on_enter(&mut self) {
        self.guide.end();
        self.choosing = false;
        self.up_held = false;
        self.down_held = false;
        self.left_held = false;
        self.right_held = false;
        self.draw();
    }

    fn on_exit(&mut self) {
        self.release_all();
        self.guide.end();
        self.choosing = false;
        self.up_held = false;
        self.down_held = false;
        self.left_held = false;
        self.right_held = false;
    }

    fn on_surface(&mut self, event: SurfaceEvent) {
        let Some(slot) = grid_slot(event.index) else { return };
        if self.choosing {
            if event.pressed && event.index / 10 == 1 {
                if let Some(preset) = Preset::from_slot(event.index % 10 - 1) {
                    self.release_all();
                    self.preset = preset;
                    self.choosing = false;
                    self.draw();
                }
            }
            return;
        }

        if event.pressed {
            if self.active[slot].is_some() {
                return;
            }
            let Some(note) = note_for(self.preset, event.index, self.transpose.total()) else { return };
            let velocity = settings::with(|s| {
                if s.velocity_enabled == 0 { 127 } else { apply_velocity_curve(s.velocity_curve, event.value) }
            });
            let already_sounding = self.active.contains(&Some(note));
            self.active[slot] = Some(note);
            if !already_sounding {
                driver::send_midi(MidiPort::Midi, &[0x90, note, velocity.max(1)]);
            }
            self.refresh_pitch(note);
        } else if let Some(note) = self.active[slot].take() {
            if !self.active.contains(&Some(note)) {
                driver::send_midi(MidiPort::Midi, &[0x90, note, 0]);
            }
            self.refresh_pitch(note);
        }
    }

    fn on_surface_raw(&mut self, event: SurfaceEvent) {
        match event.index {
            91..=94 => self.transpose_button(event.index, event.pressed),
            96 if event.pressed && self.choosing => { self.choosing = false; self.draw(); }
            97 if event.pressed => { self.release_all(); self.choosing = !self.choosing; self.draw(); }
            _ => {}
        }
    }

    fn on_midi(&mut self, _event: MidiEvent) {}

    fn on_aftertouch(&mut self, event: AftertouchEvent) {
        let Some(note) = grid_slot(event.index).and_then(|slot| self.active[slot]) else { return };
        match settings::with(|s| s.aftertouch_mode) {
            1 => driver::send_midi(MidiPort::Midi, &[0xa0, note, event.value]),
            2 => driver::send_midi(MidiPort::Midi, &[0xd0, event.value]),
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

    fn midi(port: MidiPort, status: u8, note: u8, velocity: u8) -> MidiEvent {
        MidiEvent { port, status, data1: note, data2: velocity }
    }

    #[test]
    fn legacy_midi_never_lights_pads_before_during_or_after_guide_session() {
        let mut app = NoteApp::new();
        let base = app.grid_color(11);
        for session in 0..3 {
            if session == 1 { app.guide.begin(); }
            if session == 2 { app.guide.end(); }
            for channel in 0..5 {
                for status in [0x80 | channel, 0x90 | channel] {
                    for port in [MidiPort::Midi, MidiPort::Daw] {
                        app.on_midi(midi(port, status, 48, 127));
                        app.on_midi(midi(port, status, 11, 127));
                        assert_eq!(app.grid_color(11), base);
                    }
                }
            }
        }
        app.active[0] = Some(48);
        assert_eq!(app.grid_color(11), PadColor::Rgb(0xffa2ea));
        app.on_midi(midi(MidiPort::Midi, 0x80, 48, 0));
        assert_eq!(app.grid_color(11), PadColor::Rgb(0xffa2ea));
        app.active[0] = None;
        app.guide.begin();
        app.guide.set(1, 0, 48, 21);
        app.on_midi(midi(MidiPort::Midi, 0x92, 48, 127));
        assert_eq!(app.grid_color(11), PadColor::Novation(21));
    }

    #[test]
    fn query_reports_only_sysex_guide_capabilities() {
        let mut app = NoteApp::new();
        let mut request = [0; 16];
        let len = guide::reply(&[0x01], guide::MODEL_X, &mut request).unwrap();
        assert!(app.receive_guide_sysex(&request[..len]));
        let replies = crate::driver::take_midi();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].0, MidiPort::Midi);
        assert_eq!(&replies[0].1[7..11], &[0x41, 0x03, 1, 0]);
    }

    #[test]
    fn pad_press_and_release_still_send_performance_notes() {
        let mut app = NoteApp::new();
        crate::driver::take_midi();
        app.on_surface(SurfaceEvent { index: 11, pressed: true, value: 112 });
        app.on_surface(SurfaceEvent { index: 11, pressed: false, value: 8 });
        assert_eq!(crate::driver::take_midi(), vec![
            (MidiPort::Midi, vec![0x90, 48, 127]),
            (MidiPort::Midi, vec![0x90, 48, 0]),
        ]);
    }

    #[test]
    fn overlapping_pads_keep_local_color_until_both_are_released() {
        let mut app = NoteApp::new();
        let pitch = note_for(Preset::Chromatic, 15, 0).unwrap();
        assert_eq!(note_for(Preset::Chromatic, 71, 0), Some(pitch));
        app.active[4] = Some(pitch);
        app.active[48] = Some(pitch);
        app.active[4] = None;
        assert_eq!(app.grid_color(15), PadColor::Rgb(0xffa2ea));
        app.active[48] = None;
        assert_eq!(app.grid_color(15), PadColor::Rgb(base_color(pitch, 0)));
    }

    #[test]
    fn private_guide_tracks_pitch_across_transpose_and_preset_and_clears_on_exit() {
        let mut app = NoteApp::new();
        app.guide.begin();
        app.guide.set(0, 0, 48, 13);
        assert_eq!(app.grid_color(11), PadColor::Novation(13));
        assert_ne!(app.grid_color(15), PadColor::Novation(13));
        for _ in 0..4 { app.transpose = app.transpose.shifted(-1).unwrap(); }
        assert_eq!(app.grid_color(21), PadColor::Novation(13));
        app.preset = Preset::Classic;
        app.transpose = app.transpose.without_semitones();
        assert_eq!(app.grid_color(41), PadColor::Novation(13));
        app.active[24] = Some(48);
        assert_ne!(app.grid_color(41), PadColor::Novation(13));
        app.active[24] = None;
        app.guide.set(1, 0, 48, 21);
        assert_eq!(app.grid_color(41), PadColor::Novation(21));
        app.on_exit();
        assert!(!app.guide.active());
        assert_ne!(app.grid_color(41), PadColor::Novation(21));
    }

    #[test]
    fn fallback_tracks_live_layout_and_resolves_collisions() {
        let mut app = NoteApp::new();
        app.guide.begin();
        app.guide.set(0, 1, 48, 21); // pitch 48 only exists on the left
        assert_eq!(app.grid_color(11), PadColor::Novation(21));
        app.active[0] = Some(48);
        assert_eq!(app.grid_color(11), PadColor::Rgb(0xffa2ea));
        app.guide.set(1, 1, 48, 31);
        assert_eq!(app.grid_color(11), PadColor::Novation(31));
        app.active[0] = None;
        app.guide.set(1, 1, 48, 0);
        app.guide.set(0, 1, 48, 0);
        app.guide.set(0, 0, 72, 13);
        assert_eq!(app.grid_color(71), PadColor::Novation(13));
        assert_ne!(app.grid_color(15), PadColor::Novation(13)); // 72 also on left
        app.guide.set(1, 1, 72, 31);
        assert_eq!(app.grid_color(15), PadColor::Novation(31));
        app.guide.set(1, 1, 72, 0);
        app.transpose = app.transpose.shifted(12).unwrap();
        assert_eq!(app.grid_color(41), PadColor::Novation(13)); // left pitch 72 follows transpose
        app.guide.end();
        assert_ne!(app.grid_color(41), PadColor::Novation(13));
    }
}

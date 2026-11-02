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
    // External note feedback is separate from local pad state: an incoming
    // Note Off must not extinguish a pitch still held on the device.
    external: [bool; 128],
    preview: [u8; 128],
    current: [u8; 128],
    // Physical grid overlays (ch4/ch5), independent of overlapping pitches.
    pad_preview: [u8; 64],
    pad_current: [u8; 64],
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
            external: [false; 128],
            preview: [0; 128],
            current: [0; 128],
            pad_preview: [0; 64],
            pad_current: [0; 64],
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

    fn update_external(&mut self, event: MidiEvent) -> Option<u8> {
        if event.port != MidiPort::Midi || event.data1 > 127 || event.data2 > 127 {
            return None;
        }

        let value = match event.status & 0xf0 {
            0x90 => event.data2, // Note On with velocity 0 is Note Off.
            0x80 => 0,
            _ => return None,
        };
        let note = event.data1;
        let channel = event.status & 0x0f;
        match channel {
            0 => {
                let lit = value != 0;
                if self.external[note as usize] == lit {
                    return None;
                }
                self.external[note as usize] = lit;
            }
            1 | 2 => {
                let layer = if channel == 1 {
                    &mut self.preview
                } else {
                    &mut self.current
                };
                if layer[note as usize] == value {
                    return None;
                }
                layer[note as usize] = value;
            }
            _ => return None,
        }
        Some(note)
    }

    fn update_pad_feedback(&mut self, event: MidiEvent) -> Option<u8> {
        if event.port != MidiPort::Midi || event.data2 > 127 {
            return None;
        }
        let slot = grid_slot(event.data1)?;
        let value = match event.status & 0xf0 {
            0x90 => event.data2,
            0x80 => 0,
            _ => return None,
        };
        let layer = match event.status & 0x0f {
            3 => &mut self.pad_preview,
            4 => &mut self.pad_current,
            _ => return None,
        };
        if layer[slot] == value {
            return None;
        }
        layer[slot] = value;
        Some(event.data1)
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
                self.guide_reply(&[0x41, 0x07, 1, self.guide.active() as u8]);
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
        // Current > locally held > preview > legacy ch1 > scale background.
        // Clearing any one layer always reveals the next active layer.
        if self.current[note as usize] != 0 {
            PadColor::Novation(self.current[note as usize])
        } else if self.active.contains(&Some(note)) {
            played_color
        } else if self.preview[note as usize] != 0 {
            PadColor::Novation(self.preview[note as usize])
        } else if self.external[note as usize] {
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
        ) {
            return PadColor::Novation(color);
        }
        if self.pad_current[slot] != 0 {
            return PadColor::Novation(self.pad_current[slot]);
        }
        // A pad-specific preview must remain visible even if a previous hit
        // on another pad with the same pitch is still marked current (ch3).
        // A physically held pad keeps its legacy display until released.
        if self.pad_preview[slot] != 0 && self.active[slot].is_none() {
            return PadColor::Novation(self.pad_preview[slot]);
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

    fn refresh_pad(&self, index: u8) {
        Self::set_pad(index, self.grid_color(index));
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
        self.clear_pad_feedback();
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
        self.clear_legacy_feedback();
        self.clear_pad_feedback();
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
        self.clear_legacy_feedback();
        self.clear_pad_feedback();
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
                    self.clear_pad_feedback();
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

    fn on_midi(&mut self, event: MidiEvent) {
        if self.guide.active() { return; } // Suppress old ch1–5 LEDs during private guide session.
        if let Some(index) = self.update_pad_feedback(event) {
            if !self.choosing {
                self.refresh_pad(index);
            }
        } else if let Some(note) = self.update_external(event) && !self.choosing {
            self.refresh_pitch(note);
        }
    }

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
            self.clear_legacy_feedback();
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
    fn external_note_on_and_both_off_forms_update_feedback() {
        let mut app = NoteApp::new();
        assert_eq!(app.update_external(midi(MidiPort::Midi, 0x90, 48, 100)), Some(48));
        assert_eq!(app.pad_color(48), PadColor::Rgb(0xffa2ea));
        assert_eq!(app.update_external(midi(MidiPort::Midi, 0x90, 48, 0)), Some(48));
        assert_eq!(app.pad_color(48), PadColor::Rgb(base_color(48, 0)));
        assert_eq!(app.update_external(midi(MidiPort::Midi, 0x90, 48, 1)), Some(48));
        assert_eq!(app.update_external(midi(MidiPort::Midi, 0x80, 48, 64)), Some(48));
        assert_eq!(app.pad_color(48), PadColor::Rgb(base_color(48, 0)));
    }

    #[test]
    fn independent_layers_reveal_the_next_active_color() {
        let mut app = NoteApp::new();
        let note = note_for(Preset::Chromatic, 15, 0).unwrap();
        assert_eq!(note_for(Preset::Chromatic, 71, 0), Some(note));
        app.update_external(midi(MidiPort::Midi, 0x90, note, 127));
        app.update_external(midi(MidiPort::Midi, 0x91, note, 13));
        assert_eq!(app.pad_color(note), PadColor::Novation(13));
        app.update_external(midi(MidiPort::Midi, 0x92, note, 42));
        assert_eq!(app.pad_color(note), PadColor::Novation(42));
        // Clearing the hidden preview must not clear current.
        app.update_external(midi(MidiPort::Midi, 0x81, note, 0));
        assert_eq!(app.pad_color(note), PadColor::Novation(42));
        app.update_external(midi(MidiPort::Midi, 0x91, note, 13));
        app.update_external(midi(MidiPort::Midi, 0x92, note, 0));
        assert_eq!(app.pad_color(note), PadColor::Novation(13));
        app.update_external(midi(MidiPort::Midi, 0x91, note, 0));
        assert_eq!(app.pad_color(note), PadColor::Rgb(0xffa2ea));
        app.update_external(midi(MidiPort::Midi, 0x80, note, 0));
        assert_eq!(app.pad_color(note), PadColor::Rgb(base_color(note, 0)));
    }

    #[test]
    fn local_press_survives_external_off_and_preview() {
        let mut app = NoteApp::new();
        app.active[0] = Some(48);
        app.update_external(midi(MidiPort::Midi, 0x90, 48, 127));
        app.update_external(midi(MidiPort::Midi, 0x80, 48, 0));
        app.update_external(midi(MidiPort::Midi, 0x91, 48, 5));
        assert_eq!(app.pad_color(48), PadColor::Rgb(0xffa2ea));
        app.update_external(midi(MidiPort::Midi, 0x92, 48, 9));
        assert_eq!(app.pad_color(48), PadColor::Novation(9));
        app.update_external(midi(MidiPort::Midi, 0x82, 48, 0));
        assert_eq!(app.pad_color(48), PadColor::Rgb(0xffa2ea));
        app.active[0] = None;
        assert_eq!(app.pad_color(48), PadColor::Novation(5));
    }

    #[test]
    fn overlapping_pads_keep_local_color_until_both_are_released() {
        let mut app = NoteApp::new();
        let note = note_for(Preset::Chromatic, 15, 0).unwrap();
        app.active[4] = Some(note);
        app.active[48] = Some(note);
        app.update_external(midi(MidiPort::Midi, 0x91, note, 13));
        app.active[4] = None;
        assert_eq!(app.pad_color(note), PadColor::Rgb(0xffa2ea));
        app.active[48] = None;
        assert_eq!(app.pad_color(note), PadColor::Novation(13));
    }

    #[test]
    fn only_note_messages_on_channels_one_to_three_of_midi_port_change_feedback() {
        let mut app = NoteApp::new();
        for event in [
            midi(MidiPort::Daw, 0x90, 48, 127),
            midi(MidiPort::Daw, 0x91, 48, 127),
            midi(MidiPort::Midi, 0x93, 48, 127),
            midi(MidiPort::Midi, 0xb0, 48, 127),
            midi(MidiPort::Midi, 0x90, 128, 127),
            midi(MidiPort::Midi, 0x92, 48, 128),
        ] {
            assert_eq!(app.update_external(event), None);
        }
        assert_eq!(app.pad_color(48), PadColor::Rgb(base_color(48, 0)));
        assert_eq!(app.update_external(midi(MidiPort::Midi, 0x91, 48, 127)), Some(48));
        assert_eq!(app.update_external(midi(MidiPort::Midi, 0x91, 48, 127)), None);
        assert_eq!(app.pad_color(48), PadColor::Novation(127));
    }

    #[test]
    fn layer_state_follows_pitch_across_layout_and_transpose_and_clears_on_exit() {
        let mut app = NoteApp::new();
        app.update_external(midi(MidiPort::Midi, 0x92, 48, 12));
        assert_eq!(app.pad_color(note_for(Preset::Chromatic, 11, 0).unwrap()), PadColor::Novation(12));
        app.preset = Preset::Classic;
        assert_eq!(app.pad_color(note_for(Preset::Classic, 11, 0).unwrap()), PadColor::Rgb(base_color(36, 0)));
        assert_eq!(app.pad_color(note_for(Preset::Classic, 41, 0).unwrap()), PadColor::Novation(12));
        app.transpose = app.transpose.shifted(12).unwrap();
        assert_eq!(app.pad_color(note_for(Preset::Classic, 11, 12).unwrap()), PadColor::Novation(12));
        app.on_exit();
        assert_eq!(app.pad_color(48), PadColor::Rgb(base_color(48, 12)));
    }

    #[test]
    fn pad_layers_target_only_one_of_two_pads_with_the_same_pitch() {
        let mut app = NoteApp::new();
        let pitch = note_for(Preset::Chromatic, 15, 0).unwrap();
        assert_eq!(note_for(Preset::Chromatic, 71, 0), Some(pitch));
        app.update_external(midi(MidiPort::Midi, 0x92, pitch, 42));
        assert_eq!(app.grid_color(15), PadColor::Novation(42));
        assert_eq!(app.grid_color(71), PadColor::Novation(42));

        // Next strike on a different pad stays visible over ch3's pitch-wide current.
        assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x93, 71, 13)), Some(71));
        assert_eq!(app.grid_color(15), PadColor::Novation(42));
        assert_eq!(app.grid_color(71), PadColor::Novation(13));
        assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x94, 15, 21)), Some(15));
        assert_eq!(app.grid_color(15), PadColor::Novation(21));
        assert_eq!(app.grid_color(71), PadColor::Novation(13));

        assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x84, 15, 127)), Some(15));
        assert_eq!(app.grid_color(15), PadColor::Novation(42));
        assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x93, 71, 0)), Some(71));
        assert_eq!(app.grid_color(71), PadColor::Novation(42));
        app.update_external(midi(MidiPort::Midi, 0x82, pitch, 0));
        assert_eq!(app.grid_color(15), PadColor::Rgb(base_color(pitch, 0)));
        assert_eq!(app.grid_color(71), PadColor::Rgb(base_color(pitch, 0)));
    }

    #[test]
    fn pad_current_wins_and_preview_does_not_hide_a_pressed_pad() {
        let mut app = NoteApp::new();
        app.update_pad_feedback(midi(MidiPort::Midi, 0x93, 11, 1));
        app.active[0] = Some(48);
        assert_eq!(app.grid_color(11), PadColor::Rgb(0xffa2ea));
        app.update_pad_feedback(midi(MidiPort::Midi, 0x94, 11, 127));
        assert_eq!(app.grid_color(11), PadColor::Novation(127));
        app.update_pad_feedback(midi(MidiPort::Midi, 0x94, 11, 0));
        assert_eq!(app.grid_color(11), PadColor::Rgb(0xffa2ea));
        app.active[0] = None;
        assert_eq!(app.grid_color(11), PadColor::Novation(1));
        app.update_pad_feedback(midi(MidiPort::Midi, 0x83, 11, 0));
        assert_eq!(app.grid_color(11), PadColor::Rgb(base_color(48, 0)));
    }

    #[test]
    fn pad_feedback_accepts_exactly_the_64_grid_indices_on_midi_port() {
        let mut app = NoteApp::new();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x93, index, 127)), Some(index));
                assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x93, index, 127)), None);
                assert_eq!(app.grid_color(index), PadColor::Novation(127));
            }
        }
        for index in 0..=127 {
            if grid_slot(index).is_none() {
                assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x94, index, 1)), None);
            }
        }
        for event in [
            midi(MidiPort::Daw, 0x93, 11, 1),
            midi(MidiPort::Midi, 0x95, 11, 1),
            midi(MidiPort::Midi, 0xb3, 11, 1),
            midi(MidiPort::Midi, 0x94, 11, 128),
        ] {
            assert_eq!(app.update_pad_feedback(event), None);
        }
        assert_eq!(app.update_pad_feedback(midi(MidiPort::Midi, 0x83, 11, 0)), Some(11));
        assert_eq!(app.grid_color(11), PadColor::Rgb(base_color(48, 0)));
    }

    #[test]
    fn pad_overlays_clear_on_layout_or_transpose_change_and_mode_exit() {
        let mut app = NoteApp::new();
        app.update_external(midi(MidiPort::Midi, 0x92, 48, 9));
        app.update_pad_feedback(midi(MidiPort::Midi, 0x93, 11, 13));
        app.clear_pad_feedback(); // Called by preset selection and transpose changes.
        assert_eq!(app.grid_color(11), PadColor::Novation(9));
        app.preset = Preset::Classic;
        assert_eq!(app.grid_color(11), PadColor::Rgb(base_color(36, 0)));
        app.transpose = app.transpose.shifted(12).unwrap();
        assert_eq!(app.grid_color(11), PadColor::Novation(9));
        app.update_pad_feedback(midi(MidiPort::Midi, 0x94, 11, 21));
        app.on_exit();
        assert_eq!(app.pad_current[0], 0);
        assert_eq!(app.pad_preview[0], 0);
        assert_eq!(app.grid_color(11), PadColor::Rgb(base_color(48, 12)));
    }

    #[test]
    fn private_guide_tracks_pitch_across_transpose_and_preset_and_expires_on_exit() {
        let mut app = NoteApp::new();
        app.guide.begin();
        app.guide.set(0, 0, 48, 13);
        assert_eq!(app.grid_color(11), PadColor::Novation(13));
        assert_ne!(app.grid_color(15), PadColor::Novation(13));
        app.transpose = app.transpose.shifted(-4).unwrap();
        assert_eq!(app.grid_color(21), PadColor::Novation(13));
        app.preset = Preset::Classic;
        app.transpose = app.transpose.shifted(4).unwrap();
        assert_eq!(app.grid_color(41), PadColor::Novation(13));
        app.active[24] = Some(48); // Pad 41 is physically held: preview must not hide it.
        assert_ne!(app.grid_color(41), PadColor::Novation(13));
        app.active[24] = None;
        app.guide.set(1, 0, 48, 21);
        assert_eq!(app.grid_color(41), PadColor::Novation(21));
        app.on_exit();
        assert!(!app.guide.active());
        assert_ne!(app.grid_color(41), PadColor::Novation(21));
    }
}

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
                    Self::set_pad(index, self.pad_color(pitch));
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
                    note_for(self.preset, index, self.transpose.total())
                        .map_or(PadColor::Rgb(0), |note| self.pad_color(note))
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
        self.external.fill(false);
        self.preview.fill(0);
        self.current.fill(0);
        self.choosing = false;
        self.up_held = false;
        self.down_held = false;
        self.left_held = false;
        self.right_held = false;
        self.draw();
    }

    fn on_exit(&mut self) {
        self.release_all();
        self.external.fill(false);
        self.preview.fill(0);
        self.current.fill(0);
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

    fn on_midi(&mut self, event: MidiEvent) {
        if let Some(note) = self.update_external(event) && !self.choosing {
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

    fn on_tick(&mut self) {}
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
}

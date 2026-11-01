// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

mod layout;

use crate::app::apptrait::App;
use crate::app::events::{AftertouchEvent, MidiEvent, SurfaceEvent};
use crate::app::performance::apply_velocity_curve;
use crate::driver;
use crate::sys::led;
use crate::sys::midi::MidiPort;
use crate::sys::settings;

use layout::{base_color, grid_slot, note_for, Preset};

const MIN_TRANSPOSE: i16 = -24;
const MAX_TRANSPOSE: i16 = 12;

pub struct NoteApp {
    preset: Preset,
    transpose: i16,
    octave_presses: i8,
    up_held: bool,
    down_held: bool,
    choosing: bool,
    // Latch the actual sounding note so releasing a pad cannot turn off a
    // different pitch after a layout or transpose change.
    active: [Option<u8>; 64],
}

impl NoteApp {
    pub const fn new() -> Self {
        Self {
            preset: Preset::Chromatic,
            transpose: 0,
            octave_presses: 0,
            up_held: false,
            down_held: false,
            choosing: false,
            active: [None; 64],
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

    // All LED decisions for a pitch go through this predicate. A future
    // receive-MIDI implementation can add its own note state here; local
    // Note Off must never clear an externally held pitch.
    fn pitch_lit(&self, note: u8) -> bool {
        self.active.contains(&Some(note))
    }

    fn pad_color(&self, note: u8) -> u32 {
        if self.pitch_lit(note) {
            if (note as i16 - self.transpose).rem_euclid(12) == 0 {
                0xffa2ea
            } else {
                0x9beaff
            }
        } else {
            base_color(note, self.transpose)
        }
    }

    fn refresh_pitch(&self, pitch: u8) {
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                if note_for(self.preset, index, self.transpose) == Some(pitch) {
                    led::set(index, self.pad_color(pitch));
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
                        if self.preset.slot() == slot { 0xff47ce } else { 0x239fd1 }
                    } else {
                        0
                    }
                } else {
                    note_for(self.preset, index, self.transpose)
                        .map_or(0, |note| self.pad_color(note))
                };
                led::set(index, color);
            }
        }

        for (index, step) in [(91, 12), (92, -12), (93, -1), (94, 1)] {
            let disabled = self.choosing || (self.transpose + step).clamp(MIN_TRANSPOSE, MAX_TRANSPOSE) == self.transpose;
            let color = if !self.choosing && step == 12 && self.octave_presses > 0 {
                0x40e8ff // one press upwards (maximum is +1 octave)
            } else if !self.choosing && step == -12 && self.octave_presses < 0 {
                if self.octave_presses <= -2 { 0x00eaff } else { 0x146778 }
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

    fn set_transpose(&mut self, step: i16) {
        let next = (self.transpose + step).clamp(MIN_TRANSPOSE, MAX_TRANSPOSE);
        if next == self.transpose {
            return;
        }
        self.release_all();
        self.transpose = next;
        if step == 12 {
            self.octave_presses = (self.octave_presses + 1).min(1);
        } else if step == -12 {
            self.octave_presses = (self.octave_presses - 1).max(-2);
        }
        self.draw();
    }

    fn octave_button(&mut self, up: bool, pressed: bool) {
        let was_held = if up { self.up_held } else { self.down_held };
        if up { self.up_held = pressed; } else { self.down_held = pressed; }
        if !pressed || was_held || self.choosing {
            return;
        }
        if self.up_held && self.down_held {
            self.release_all();
            self.transpose = 0;
            self.octave_presses = 0;
            self.draw();
        } else {
            self.set_transpose(if up { 12 } else { -12 });
        }
    }
}

impl App for NoteApp {
    fn on_enter(&mut self) {
        self.choosing = false;
        self.up_held = false;
        self.down_held = false;
        self.draw();
    }

    fn on_exit(&mut self) {
        self.release_all();
        self.choosing = false;
        self.up_held = false;
        self.down_held = false;
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
            let Some(note) = note_for(self.preset, event.index, self.transpose) else { return };
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
            91 => self.octave_button(true, event.pressed),
            92 => self.octave_button(false, event.pressed),
            93 if event.pressed && !self.choosing => self.set_transpose(-1),
            94 if event.pressed && !self.choosing => self.set_transpose(1),
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

    fn on_tick(&mut self) {}
}

// SPDX-License-Identifier: GPL-3.0-only
// Fixed Pro MK3 Custom layout. DIN routing is deliberately not claimed here.

use crate::app::{AftertouchEvent, App, MidiEvent, SurfaceEvent};
use crate::app::performance::apply_velocity_curve;
use crate::driver;
use crate::sys::{led, settings};
use crate::sys::midi::MidiPort;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadColor {
    Rgb(u32),
    Novation(u8),
}

pub struct CustomNoteApp {
    octaves: i16,
    semitones: i16,
    held: [Option<u8>; 64],
    external: [bool; 128],
    preview: [u8; 128],
    current: [u8; 128],
    pad_preview: [u8; 64],
    pad_current: [u8; 64],
    arrows: [bool; 4],
    reset_latch: [bool; 2],
    channel: u8,
    channel_view: bool,
}

impl CustomNoteApp {
    pub const fn new() -> Self {
        Self {
            octaves: 0, semitones: 0, held: [None; 64],
            external: [false; 128], preview: [0; 128], current: [0; 128],
            pad_preview: [0; 64], pad_current: [0; 64],
            arrows: [false; 4], reset_latch: [false; 2],
            channel: 0, channel_view: false,
        }
    }

    fn transpose(&self) -> i16 { self.octaves * 12 + self.semitones }

    // Programmer layout grid indices, bottom-left = MIDI 48.
    fn pitch(index: u8, transpose: i16) -> Option<u8> {
        let col = index % 10;
        let row = index / 10;
        if !(1..=8).contains(&col) || !(1..=8).contains(&row) { return None; }
        let pitch = 48 + (row as i16 - 1) * 4 + (col as i16 - 1) % 4
            + (if col >= 5 { 24 } else { 0 }) + transpose;
        u8::try_from(pitch).ok().filter(|&p| p <= 127)
    }

    fn slot(index: u8) -> Option<usize> {
        let col = index % 10;
        let row = index / 10;
        if (1..=8).contains(&col) && (1..=8).contains(&row) {
            Some(((row - 1) * 8 + col - 1) as usize)
        } else { None }
    }

    fn held_count(&self, pitch: u8) -> usize {
        self.held.iter().filter(|&&p| p == Some(pitch)).count()
    }

    fn clear_pad_feedback(&mut self) {
        self.pad_preview.fill(0);
        self.pad_current.fill(0);
    }

    fn color(&self, index: u8) -> PadColor {
        let Some(slot) = Self::slot(index) else { return PadColor::Rgb(0) };
        if self.pad_current[slot] != 0 {
            return PadColor::Novation(self.pad_current[slot]);
        }
        // Keep the next hit visible on a different pad even when the
        // previous hit still has a pitch-wide current indication.
        if self.pad_preview[slot] != 0 && self.held[slot].is_none() {
            return PadColor::Novation(self.pad_preview[slot]);
        }
        let Some(p) = Self::pitch(index, self.transpose()) else { return PadColor::Rgb(0) };
        if self.current[p as usize] != 0 {
            PadColor::Novation(self.current[p as usize])
        } else if self.held_count(p) != 0 {
            PadColor::Rgb(0xffffff)
        } else if self.preview[p as usize] != 0 {
            PadColor::Novation(self.preview[p as usize])
        } else if self.external[p as usize] {
            PadColor::Rgb(0xffffff)
        } else if (p as i16 - self.transpose()).rem_euclid(12) == 0 {
            PadColor::Rgb(0xff4090)
        } else if matches!((p as i16 - self.transpose()).rem_euclid(12), 2 | 4 | 5 | 7 | 9 | 11) {
            PadColor::Rgb(0x1050d0)
        } else {
            PadColor::Rgb(0x101024)
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
            // Channels 2–5 are reserved for display; channels 6–16 retain
            // the previous selected-channel feedback behavior.
            _ if channel == self.channel => self.external[event.data1 as usize] = value != 0,
            _ => return false,
        }
        true
    }

    fn redraw(&self) {
        if self.channel_view {
            for i in 0..16u8 {
                let index = if i < 8 { 11 + i } else { 21 + i - 8 };
                led::set(index, if self.channel == i { 0x70e0a0 } else { 0x303030 });
            }
            led::set(96, 0x70e0a0);
            led::set(94, 0x404040);
            led::set(93, 0x404040);
            return;
        }
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                match self.color(index) {
                    PadColor::Rgb(rgb) => led::set(index, rgb),
                    PadColor::Novation(color) => led::novation(index, color),
                }
            }
        }
        // Physical navigation: top 91/92 = left/right, side 80/70 = up/down.
        for (index, amount) in [(80, self.octaves), (70, -self.octaves),
                                (92, self.semitones), (91, -self.semitones)] {
            led::set(index, if amount > 0 { 0x00dfff } else { 0x164060 });
        }
        led::set(96, 0x70e0a0); // Custom
        led::set(94, 0x404040); // Note
        led::set(93, 0x404040); // Session
    }

    fn arrow(&mut self, index: u8, pressed: bool) {
        let (pair, side, step) = match index {
            80 => (0, 0, 12), 70 => (0, 1, -12),
            91 => (1, 0, -1), 92 => (1, 1, 1),
            _ => return,
        };
        if self.arrows[pair * 2 + side] == pressed { return; }
        let previous_transpose = self.transpose();
        self.arrows[pair * 2 + side] = pressed;
        if self.arrows[pair * 2] && self.arrows[pair * 2 + 1] {
            if !self.reset_latch[pair] {
                if pair == 0 { self.octaves = 0; } else { self.semitones = 0; }
                self.reset_latch[pair] = true;
            }
        } else if !pressed && !self.arrows[pair * 2] && !self.arrows[pair * 2 + 1] {
            self.reset_latch[pair] = false;
        } else if pressed && !self.reset_latch[pair] {
            // Keep at least the lowest pad in MIDI range; individual out-of-range
            // pads stay dark and never send an invalid note.
            let next = self.transpose() + step;
            if (-102..=79).contains(&next) {
                if pair == 0 { self.octaves += step / 12; }
                else { self.semitones += step; }
            }
        }
        if previous_transpose != self.transpose() {
            self.clear_pad_feedback();
        }
        self.redraw();
    }

    fn release_all(&mut self) {
        for pitch in 0..=127u8 {
            if self.held_count(pitch) != 0 {
                driver::send_midi(MidiPort::Midi, &[0x90 | self.channel, pitch, 0]);
            }
        }
        self.held = [None; 64];
    }

    pub fn show_channel_settings(&mut self, show: bool) {
        if self.channel_view != show {
            self.release_all();
            self.channel_view = show;
            led::clear();
            self.redraw();
        }
    }
}

impl App for CustomNoteApp {
    fn on_enter(&mut self) {
        self.external.fill(false);
        self.preview.fill(0);
        self.current.fill(0);
        self.clear_pad_feedback();
        self.redraw();
    }
    fn on_exit(&mut self) {
        self.release_all();
        self.external = [false; 128];
        self.preview.fill(0);
        self.current.fill(0);
        self.clear_pad_feedback();
        self.arrows = [false; 4];
        self.reset_latch = [false; 2];
        self.channel_view = false;
    }
    fn on_surface(&mut self, event: SurfaceEvent) {
        if self.channel_view {
            if !event.pressed { return; }
            let channel = match event.index {
                11..=18 => Some(event.index - 11),
                21..=28 => Some(event.index - 21 + 8),
                _ => None,
            };
            if let Some(channel) = channel {
                if self.channel != channel {
                    self.external = [false; 128];
                    self.clear_pad_feedback();
                }
                self.channel = channel;
                self.redraw();
            }
            return;
        }
        if matches!(event.index, 80 | 70 | 91 | 92) {
            self.arrow(event.index, event.pressed);
            return;
        }
        let Some(slot) = Self::slot(event.index) else { return; };
        if event.pressed {
            if self.held[slot].is_some() { return; }
            let Some(pitch) = Self::pitch(event.index, self.transpose()) else { return; };
            let already_held = self.held_count(pitch) != 0;
            self.held[slot] = Some(pitch);
            if !already_held {
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
        self.redraw();
    }
    fn on_midi(&mut self, event: MidiEvent) {
        if self.receive_feedback(event) && !self.channel_view { self.redraw(); }
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
    use super::{CustomNoteApp as C, PadColor};
    use crate::app::{App, MidiEvent};
    use crate::sys::midi::MidiPort;

    fn midi(port: MidiPort, status: u8, index: u8, value: u8) -> MidiEvent {
        MidiEvent { port, status, data1: index, data2: value }
    }

    #[test]
    fn fixed_layout_and_range() {
        assert_eq!(C::pitch(11, 0), Some(48));
        assert_eq!(C::pitch(14, 0), Some(51));
        assert_eq!(C::pitch(15, 0), Some(72));
        assert_eq!(C::pitch(71, 0), C::pitch(15, 0));
        assert_eq!(C::pitch(88, 0), Some(103));
        assert_eq!(C::pitch(11, -49), None);
        assert_eq!(C::pitch(88, 25), None);
    }

    #[test]
    fn held_pitch_is_independent_of_future_transposition() {
        let mut app = C::new();
        let pitch = C::pitch(15, 0).unwrap();
        app.held[C::slot(15).unwrap()] = Some(pitch);
        app.held[C::slot(71).unwrap()] = Some(pitch);
        app.octaves = 1;
        assert_eq!(app.held_count(pitch), 2);
        assert_eq!(C::pitch(15, app.transpose()), Some(pitch + 12));
        app.held[C::slot(15).unwrap()] = None;
        assert_eq!(app.held_count(pitch), 1);
    }

    #[test]
    fn custom_pitch_and_pad_layers_are_independent_on_overlapping_pads() {
        let mut app = C::new();
        let pitch = C::pitch(15, 0).unwrap();
        assert_eq!(C::pitch(71, 0), Some(pitch));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x90, pitch, 100)));
        assert_eq!(app.color(15), PadColor::Rgb(0xffffff));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x91, pitch, 13)));
        assert_eq!(app.color(71), PadColor::Novation(13));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x92, pitch, 42)));
        assert_eq!(app.color(15), PadColor::Novation(42));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x93, 71, 5)));
        assert_eq!(app.color(15), PadColor::Novation(42));
        assert_eq!(app.color(71), PadColor::Novation(5));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x94, 15, 127)));
        assert_eq!(app.color(15), PadColor::Novation(127));
        assert_eq!(app.color(71), PadColor::Novation(5));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x84, 15, 0)));
        assert_eq!(app.color(15), PadColor::Novation(42));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x83, 71, 0)));
        assert_eq!(app.color(71), PadColor::Novation(42));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x82, pitch, 0)));
        assert_eq!(app.color(15), PadColor::Novation(13));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x81, pitch, 0)));
        assert_eq!(app.color(15), PadColor::Rgb(0xffffff));
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x80, pitch, 0)));
        assert_ne!(app.color(15), PadColor::Rgb(0xffffff));
    }

    #[test]
    fn custom_pad_feedback_only_accepts_grid_and_does_not_hide_local_press() {
        let mut app = C::new();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                assert!(app.receive_feedback(midi(MidiPort::Midi, 0x93, index, 1)));
                assert_eq!(app.color(index), PadColor::Novation(1));
            }
        }
        for index in 0..=127 {
            if C::slot(index).is_none() {
                assert!(!app.receive_feedback(midi(MidiPort::Midi, 0x94, index, 1)));
            }
        }
        for event in [
            midi(MidiPort::Daw, 0x94, 11, 10),
            midi(MidiPort::Midi, 0x95, 11, 10),
            midi(MidiPort::Midi, 0xb4, 11, 10),
            midi(MidiPort::Midi, 0x94, 11, 128),
        ] {
            assert!(!app.receive_feedback(event));
        }
        app.held[0] = C::pitch(11, 0);
        assert_eq!(app.color(11), PadColor::Rgb(0xffffff));
        app.receive_feedback(midi(MidiPort::Midi, 0x94, 11, 127));
        assert_eq!(app.color(11), PadColor::Novation(127));
        app.held[0] = None;
        app.receive_feedback(midi(MidiPort::Midi, 0x94, 11, 0));
        assert_eq!(app.color(11), PadColor::Novation(1));
    }

    #[test]
    fn custom_feedback_clears_pad_layers_on_transpose_and_all_on_exit() {
        let mut app = C::new();
        app.receive_feedback(midi(MidiPort::Midi, 0x92, 48, 12));
        app.receive_feedback(midi(MidiPort::Midi, 0x93, 11, 13));
        app.clear_pad_feedback(); // arrow() clears this when the offset changes.
        assert_eq!(app.color(11), PadColor::Novation(12));
        app.octaves = 1;
        assert_ne!(app.color(11), PadColor::Novation(12));
        app.octaves = 0;
        assert_eq!(app.color(11), PadColor::Novation(12));
        app.receive_feedback(midi(MidiPort::Midi, 0x94, 11, 21));
        app.on_exit();
        assert_eq!(app.pad_current[0], 0);
        assert_eq!(app.pad_preview[0], 0);
        assert_ne!(app.color(11), PadColor::Novation(12));
    }

    #[test]
    fn custom_selected_channel_feedback_keeps_channels_six_to_sixteen_only() {
        let mut app = C::new();
        app.channel = 6; // ch7 retains the old selected-channel feedback.
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x96, 48, 99)));
        assert!(app.external[48]);
        app.channel = 3; // ch4 is reserved for pad feedback.
        assert!(!app.receive_feedback(midi(MidiPort::Midi, 0x93, 49, 99)));
        assert!(!app.external[49]);
        assert!(app.receive_feedback(midi(MidiPort::Midi, 0x93, 11, 99)));
        assert_eq!(app.pad_preview[0], 99);
    }
}

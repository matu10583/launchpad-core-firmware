// SPDX-License-Identifier: GPL-3.0-only
// Fixed Pro MK3 Custom layout. DIN routing is deliberately not claimed here.

use crate::app::{AftertouchEvent, App, MidiEvent, SurfaceEvent};
use crate::app::performance::apply_velocity_curve;
use crate::driver;
use crate::sys::{led, settings};
use crate::sys::midi::MidiPort;

pub struct CustomNoteApp {
    octaves: i16,
    semitones: i16,
    held: [Option<u8>; 64],
    external: [bool; 128],
    arrows: [bool; 4],
    reset_latch: [bool; 2],
    channel: u8,
    channel_view: bool,
}

impl CustomNoteApp {
    pub const fn new() -> Self {
        Self {
            octaves: 0, semitones: 0, held: [None; 64],
            external: [false; 128], arrows: [false; 4], reset_latch: [false; 2],
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
                let color = match Self::pitch(index, self.transpose()) {
                    None => 0,
                    Some(p) if self.held_count(p) != 0 || self.external[p as usize] => 0xffffff,
                    Some(p) if (p as i16 - self.transpose()).rem_euclid(12) == 0 => 0xff4090,
                    Some(p) if matches!((p as i16 - self.transpose()).rem_euclid(12), 2 | 4 | 5 | 7 | 9 | 11) => 0x1050d0,
                    Some(_) => 0x101024,
                };
                led::set(index, color);
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
    fn on_enter(&mut self) { self.redraw(); }
    fn on_exit(&mut self) {
        self.release_all();
        self.external = [false; 128];
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
                if self.channel != channel { self.external = [false; 128]; }
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
        if event.port != MidiPort::Midi || event.status & 0x0f != self.channel { return; }
        if matches!(event.status & 0xf0, 0x80 | 0x90) && event.data1 < 128 {
            self.external[event.data1 as usize] = event.status & 0xf0 == 0x90 && event.data2 != 0;
            self.redraw();
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
    use super::CustomNoteApp as C;

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
}

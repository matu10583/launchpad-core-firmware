// SPDX-License-Identifier: GPL-3.0-only
// Fixed Pro MK3 Custom layout. DIN routing is deliberately not claimed here.

use crate::app::{AftertouchEvent, App, MidiEvent, SurfaceEvent};
use crate::app::guide::{self, Command as GuideCommand, Guide};
use crate::app::performance::apply_velocity_settings;
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
    guide: Guide,
    arrows: [bool; 4],
    reset_latch: [bool; 2],
    channel: u8,
    channel_view: bool,
}

impl CustomNoteApp {
    pub const fn new() -> Self {
        Self {
            octaves: 0, semitones: 0, held: [None; 64],
            guide: Guide::new(),
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

    fn guide_reply(&self, payload: &[u8]) {
        let mut response = [0u8; 96];
        if let Some(len) = guide::reply(payload, guide::MODEL_PRO_MK3, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
    }

    pub fn receive_guide_sysex(&mut self, data: &[u8]) -> bool {
        let Some(command) = guide::decode(data, guide::MODEL_PRO_MK3) else { return false };
        match command {
            GuideCommand::Query => self.guide_reply(&[0x41, 0x03, 2, self.guide.active() as u8]),
            GuideCommand::Layout => {
                let payload = guide::pro_layout(2, 0, 0, 0, self.transpose(),
                    |index| Self::pitch(index, self.transpose()));
                self.guide_reply(&payload);
            }
            GuideCommand::Begin => {
                self.guide.begin();
                self.redraw();
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
                self.redraw();
                self.guide_reply(&[0x50, 0x12, 0]);
            }
            GuideCommand::Set { layer, side, pitch, color } => {
                if self.guide.set(layer, side, pitch, color) && !self.channel_view { self.redraw(); }
            }
        }
        true
    }

    fn color(&self, index: u8) -> PadColor {
        let Some(slot) = Self::slot(index) else { return PadColor::Rgb(0) };
        if let Some(color) = self.guide.pad_color(index,
            Self::pitch(index, self.transpose()), self.held[slot].is_some(),
            |side, pitch| guide::side_has_pitch(side, pitch,
                |pad| Self::pitch(pad, self.transpose()))) {
            return PadColor::Novation(color);
        }
        let Some(p) = Self::pitch(index, self.transpose()) else { return PadColor::Rgb(0) };
        if self.held_count(p) != 0 {
            PadColor::Rgb(0xffffff)
        } else if (p as i16 - self.transpose()).rem_euclid(12) == 0 {
            PadColor::Rgb(0xff4090)
        } else if matches!((p as i16 - self.transpose()).rem_euclid(12), 2 | 4 | 5 | 7 | 9 | 11) {
            PadColor::Rgb(0x1050d0)
        } else {
            PadColor::Rgb(0x101024)
        }
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
    fn on_enter(&mut self) {
        self.guide.end();
        self.redraw();
    }
    fn on_exit(&mut self) {
        self.release_all();
        self.guide.end();
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
                    apply_velocity_settings(s, event.value).max(1)
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
            self.redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CustomNoteApp as C, PadColor};
    use crate::app::{guide, App, MidiEvent, SurfaceEvent};
    use crate::sys::midi::MidiPort;

    fn midi(port: MidiPort, status: u8, index: u8, value: u8) -> MidiEvent {
        MidiEvent { port, status, data1: index, data2: value }
    }

    #[test]
    fn fixed_layout_and_held_notes_survive_transpose() {
        assert_eq!(C::pitch(11, 0), Some(48));
        assert_eq!(C::pitch(15, 0), C::pitch(71, 0));
        assert_eq!(C::pitch(88, 0), Some(103));
        assert_eq!(C::pitch(11, -49), None);
        let mut app = C::new();
        app.held[C::slot(15).unwrap()] = Some(72);
        app.octaves = 1;
        assert_eq!(app.held_count(72), 1);
        assert_eq!(C::pitch(15, app.transpose()), Some(84));
    }

    #[test]
    fn incoming_note_feedback_never_lights_pads() {
        let mut app = C::new();
        app.channel = 6;
        let base = app.color(11);
        for session in 0..3 {
            if session == 1 { app.guide.begin(); }
            if session == 2 { app.guide.end(); }
            for channel in [0, 1, 2, 3, 4, 6] {
                for port in [MidiPort::Midi, MidiPort::Daw] {
                    app.on_midi(midi(port, 0x90 | channel, 48, 127));
                    app.on_midi(midi(port, 0x90 | channel, 11, 127));
                    app.on_midi(midi(port, 0x80 | channel, 48, 0));
                    assert_eq!(app.color(11), base);
                }
            }
        }
        app.held[0] = Some(48);
        assert_eq!(app.color(11), PadColor::Rgb(0xffffff));
        app.on_midi(midi(MidiPort::Midi, 0x80, 48, 0));
        assert_eq!(app.color(11), PadColor::Rgb(0xffffff));
    }

    #[test]
    fn guide_fallback_reacts_to_transpose_and_held_priority() {
        let mut app = C::new();
        app.guide.begin();
        app.guide.set(0, 0, 48, 13); // left only
        assert_eq!(app.color(11), PadColor::Novation(13));
        app.guide.set(0, 1, 48, 21); // right hand falls back to left
        assert_eq!(app.color(11), PadColor::Novation(13)); // local wins
        app.guide.set(0, 0, 48, 0);
        assert_eq!(app.color(11), PadColor::Novation(21));
        app.held[0] = Some(48);
        assert_eq!(app.color(11), PadColor::Rgb(0xffffff));
        app.guide.set(1, 1, 48, 31);
        assert_eq!(app.color(11), PadColor::Novation(31));
        app.held[0] = None;
        app.guide.set(0, 1, 48, 0);
        app.guide.set(1, 1, 48, 0);
        app.guide.set(0, 0, 103, 13); // right only: left hand falls back
        assert_eq!(app.color(88), PadColor::Novation(13));
        app.guide.set(0, 0, 103, 0);
        app.guide.set(0, 0, 72, 13);
        assert_eq!(app.color(71), PadColor::Novation(13));
        assert_ne!(app.color(15), PadColor::Novation(13));
        app.semitones = -1;
        assert_eq!(app.color(72), PadColor::Novation(13));
        app.on_exit();
        assert!(!app.guide.active());
        assert_ne!(app.color(72), PadColor::Novation(13));
    }

    #[test]
    fn malformed_or_other_model_commands_do_not_begin_session() {
        let mut app = C::new();
        let mut frame = [0; 16];
        let n = guide::reply(&[0x10], guide::MODEL_X, &mut frame).unwrap();
        assert!(!app.receive_guide_sysex(&frame[..n]));
        let n = guide::reply(&[0x10], guide::MODEL_PRO_MK3, &mut frame).unwrap();
        frame[6] = 2;
        assert!(!app.receive_guide_sysex(&frame[..n]));
        frame[6] = 1;
        assert!(!app.receive_guide_sysex(&frame[..n - 1]));
        assert!(!app.guide.active());
        assert!(app.receive_guide_sysex(&frame[..n]));
        assert!(app.guide.active());
    }

    #[test]
    fn playing_still_sends_on_selected_channel() {
        let mut app = C::new();
        app.channel = 6;
        crate::driver::take_midi();
        app.on_surface(SurfaceEvent { index: 11, pressed: true, value: 100 });
        app.on_surface(SurfaceEvent { index: 11, pressed: false, value: 0 });
        assert_eq!(crate::driver::take_midi(), vec![
            (MidiPort::Midi, vec![0x96, 48, 127]),
            (MidiPort::Midi, vec![0x96, 48, 0]),
        ]);
    }
}

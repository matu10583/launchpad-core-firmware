// SPDX-License-Identifier: GPL-3.0-only
//! Source-defined Launchpad X factory Custom 1–4 layouts.
use crate::app::{AftertouchEvent, App, MidiEvent, SurfaceEvent};
use crate::driver;
use crate::sys::{led, midi::MidiPort, settings};

const DRUM: [[u8; 8]; 8] = [
    [36, 37, 38, 39, 68, 69, 70, 71],
    [40, 41, 42, 43, 72, 73, 74, 75],
    [44, 45, 46, 47, 76, 77, 78, 79],
    [48, 49, 50, 51, 80, 81, 82, 83],
    [52, 53, 54, 55, 84, 85, 86, 87],
    [56, 57, 58, 59, 88, 89, 90, 91],
    [60, 61, 62, 63, 92, 93, 94, 95],
    [64, 65, 66, 67, 96, 97, 98, 99],
];
const KEYS: [[u8; 8]; 8] = [
    [36, 38, 40, 41, 43, 45, 47, 36],
    [0, 37, 39, 0, 42, 44, 46, 0],
    [48, 50, 52, 53, 55, 57, 59, 60],
    [0, 49, 51, 0, 54, 56, 58, 0],
    [60, 62, 64, 65, 67, 69, 71, 72],
    [0, 61, 63, 0, 66, 68, 70, 0],
    [72, 74, 76, 77, 79, 81, 83, 84],
    [0, 73, 75, 0, 78, 80, 82, 0],
];

pub struct CustomApp {
    slot: u8,
    held: [bool; 64],
    feedback: [u8; 128],
}

impl CustomApp {
    pub const fn new() -> Self {
        Self { slot: 0, held: [false; 64], feedback: [0; 128] }
    }

    pub fn select_default(&mut self) {
        self.release_all();
        self.slot = 0;
        self.feedback = [0; 128];
        self.draw();
    }

    fn pitch(&self, index: u8) -> Option<u8> {
        let row = index / 10;
        let col = index % 10;
        if !(1..=8).contains(&row) || !(1..=8).contains(&col) { return None; }
        let pitch = match self.slot {
            0 | 2 => DRUM[(row - 1) as usize][(col - 1) as usize],
            1 => KEYS[(row - 1) as usize][(col - 1) as usize],
            _ => index,
        };
        (pitch != 0).then_some(pitch)
    }

    fn release_all(&mut self) {
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                let pos = ((row - 1) * 8 + col - 1) as usize;
                if self.held[pos] {
                    if let Some(pitch) = self.pitch(index) {
                        driver::send_midi(MidiPort::Midi, &[0x90, pitch, 0]);
                    }
                    self.held[pos] = false;
                }
            }
        }
    }

    fn draw(&self) {
        led::clear();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                let Some(pitch) = self.pitch(index) else { continue };
                let pos = ((row - 1) * 8 + col - 1) as usize;
                if self.slot >= 2 {
                    led::novation(index, self.feedback[pitch as usize]);
                } else {
                    let color = if self.held[pos] { 0xffffff }
                        else if self.slot == 0 { [0xffbd19, 0xff47ce, 0x43aaff, 0x50c45a][((row - 1) / 4 * 2 + (col - 1) / 4) as usize] }
                        else if pitch % 12 == 0 { 0xff47ce } else { 0x239fd1 };
                    led::set(index, color);
                }
            }
        }
        for (n, index) in [89, 79, 69, 59].into_iter().enumerate() {
            led::set_raw(index, if self.slot as usize == n { 0x10ff10 } else { 0x16434c });
        }
        led::set_raw(96, 0x16434c);
        led::set_raw(97, 0x10ff10);
        led::set_raw(95, 0x401010);
    }
}

impl App for CustomApp {
    fn on_enter(&mut self) { self.slot = 0; self.feedback = [0; 128]; self.draw(); }
    fn on_exit(&mut self) { self.release_all(); self.feedback = [0; 128]; }
    fn on_surface(&mut self, event: SurfaceEvent) {
        let Some(pitch) = self.pitch(event.index) else { return };
        let row = event.index / 10;
        let col = event.index % 10;
        let pos = ((row - 1) * 8 + col - 1) as usize;
        if self.held[pos] == event.pressed { return; }
        self.held[pos] = event.pressed;
        let velocity = if event.pressed {
            settings::with(|s| if s.velocity_enabled == 0 { 127 }
                else { crate::app::performance::apply_velocity_settings(s, event.value).max(1) })
        } else { 0 };
        driver::send_midi(MidiPort::Midi, &[0x90, pitch, velocity]);
        if self.slot < 2 { self.draw(); }
    }
    fn on_surface_raw(&mut self, event: SurfaceEvent) {
        if !event.pressed { return; }
        if let Some(slot) = [89, 79, 69, 59].iter().position(|&index| index == event.index) {
            self.release_all();
            self.slot = slot as u8;
            self.feedback = [0; 128];
            self.draw();
        }
    }
    fn on_midi(&mut self, event: MidiEvent) {
        if event.port != MidiPort::Midi || self.slot < 2 { return; }
        if (event.status & 0xf0) == 0x90 || (event.status & 0xf0) == 0x80 {
            let pitch = event.data1 as usize;
            if pitch >= 128 { return; }
            self.feedback[pitch] = if (event.status & 0xf0) == 0x80 { 0 } else { event.data2 };
            self.draw();
        }
    }
    fn on_aftertouch(&mut self, event: AftertouchEvent) {
        let row = event.index / 10;
        let col = event.index % 10;
        if !(1..=8).contains(&row) || !(1..=8).contains(&col) { return; }
        let pos = ((row - 1) * 8 + col - 1) as usize;
        if !self.held[pos] { return; }
        let Some(pitch) = self.pitch(event.index) else { return };
        match settings::with(|s| s.aftertouch_mode) {
            1 => driver::send_midi(MidiPort::Midi, &[0xa0, pitch, event.value]),
            2 => driver::send_midi(MidiPort::Midi, &[0xd0, event.value]),
            _ => {}
        }
    }
    fn on_tick(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(index: u8, pressed: bool) -> SurfaceEvent {
        SurfaceEvent { index, pressed, value: 70 }
    }

    #[test]
    fn factory_slots_route_pitches_and_release_on_switch() {
        let mut app = CustomApp::new();
        app.on_enter();
        assert_eq!(app.pitch(11), Some(36));
        assert_eq!(app.pitch(88), Some(99));
        app.on_surface(event(11, true));
        app.on_surface_raw(event(79, true));
        assert_eq!(app.slot, 1);
        assert_eq!(app.pitch(11), Some(36));
        assert_eq!(app.pitch(21), None);
        app.on_surface_raw(event(69, true));
        assert_eq!(app.pitch(11), Some(36));
        app.on_surface_raw(event(59, true));
        assert_eq!(app.pitch(11), Some(11));
        assert_eq!(app.pitch(88), Some(88));
        app.select_default();
        assert_eq!(app.slot, 0);
        let notes = crate::driver::take_midi();
        assert_eq!(notes[0].1, [0x90, 36, 127]);
        assert_eq!(notes[1].1, [0x90, 36, 0]);
    }
}

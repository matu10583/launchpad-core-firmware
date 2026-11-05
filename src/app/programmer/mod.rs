// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::apptrait::App;
use crate::app::events::{AftertouchEvent, MidiEvent, SurfaceEvent};
use crate::driver;
use crate::sys::midi::MidiPort;
use crate::sys::{led, settings};

pub struct ProgrammerApp;

impl ProgrammerApp {
    pub const fn new() -> Self {
        Self
    }
}

impl App for ProgrammerApp {
    fn on_enter(&mut self) {
        led::clear();
    }

    fn on_exit(&mut self) {}

    fn on_surface(&mut self, event: SurfaceEvent) {
        let velocity = settings::with(|s| if !event.pressed { 0 }
            else if s.velocity_enabled == 0 { 127 }
            else { crate::app::performance::apply_velocity_settings(s, event.value.min(127)).max(1) });

        driver::send_midi(MidiPort::Midi, &[0x90, event.index, velocity]);
    }

    fn on_midi(&mut self, event: MidiEvent) {
        if event.port != MidiPort::Midi {
            return;
        }

        let idx = event.data1;

        match event.status {
            0x90 => {
                if idx != 0 {
                    led::set_palette(idx, event.data2);
                }
            }
            0x80 => {
                if idx != 0 {
                    led::set_palette(idx, 0);
                }
            }
            0xb0 => {
                if (90..=99).contains(&event.data1) {
                    led::set_palette(event.data1, event.data2);
                }
            }
            _ => {}
        }
    }

    fn on_aftertouch(&mut self, event: AftertouchEvent) {
        match settings::with(|s| s.aftertouch_mode) {
            1 => driver::send_midi(MidiPort::Midi, &[0xa0, event.index, event.value]),
            2 => driver::send_midi(MidiPort::Midi, &[0xd0, event.value]),
            _ => {}
        }
    }

    fn on_tick(&mut self) {}
}

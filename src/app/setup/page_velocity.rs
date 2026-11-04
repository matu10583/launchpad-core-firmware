// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::SurfaceEvent;
use crate::app::setup::page::Page;
use crate::app::setup::text::Text;
use crate::sys::{led, settings};

const PAGE_BUTTONS: [u8; 3] = [81, 82, 83]; // basic, gain, low-end

pub struct VelocityPage {
    text: Text,
    page: u8,
}

impl VelocityPage {
    pub const fn new() -> Self {
        Self {
            text: Text::new(
                [0b10111110, 0b10111010, 0b10110010, 0b01011111],
                0b11100011, 0xFFAA00, 0xFFFBCC,
            ),
            page: 0,
        }
    }

    fn draw(&self) {
        for row in 1..=8 {
            for col in 1..=8 { led::set(row * 10 + col, 0); }
        }
        if self.page == 0 {
            self.text.draw();
            let (enabled, curve) = settings::with(|s| (s.velocity_enabled, s.velocity_curve));
            led::set(31, if enabled != 0 { 0x10ff10 } else { 0xff1010 });
            if enabled != 0 {
                for i in 0..3u8 { led::set(21 + i, 0x101010); }
                led::set(21 + curve.min(2), 0xff4000);
            }
        } else {
            let selected = settings::with(|s| if self.page == 1 { s.velocity_gain } else { s.velocity_soft });
            // 7 rows of 8 pads give 56 fine steps. Row 8 is navigation.
            for position in 0..settings::VELOCITY_FINE_STEPS {
                let index = (position / 8 + 1) * 10 + position % 8 + 1;
                let color = if position == selected { 0xffffff }
                    else if position < selected { if self.page == 1 { 0x007060 } else { 0x643060 } }
                    else { 0x101010 };
                led::set(index, color);
            }
        }
        for (page, index) in PAGE_BUTTONS.into_iter().enumerate() {
            led::set(index, if self.page as usize == page { 0xffffff } else { 0x304050 });
        }
    }
}

impl Page for VelocityPage {
    fn on_enter(&mut self) {
        self.page = 0;
        led::set_raw(69, 0x551500);
        self.draw();
    }

    fn on_surface(&mut self, event: SurfaceEvent) {
        if !event.pressed { return; }
        if let Some(page) = PAGE_BUTTONS.iter().position(|&index| index == event.index) {
            self.page = page as u8;
            self.draw();
            return;
        }
        if self.page != 0 {
            let row = event.index / 10;
            let col = event.index % 10;
            if (1..=7).contains(&row) && (1..=8).contains(&col) {
                let position = (row - 1) * 8 + col - 1;
                settings::update(|s| if self.page == 1 { s.velocity_gain = position; }
                    else { s.velocity_soft = position; });
                self.draw();
            }
            return;
        }
        if event.index == 31 {
            settings::update(|s| s.velocity_enabled = if s.velocity_enabled != 0 { 0 } else { 1 });
            self.draw();
        } else if (21..=23).contains(&event.index) && settings::with(|s| s.velocity_enabled) != 0 {
            settings::update(|s| s.velocity_curve = event.index - 21);
            self.draw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(index: u8) -> SurfaceEvent {
        SurfaceEvent { index, pressed: true, value: 127 }
    }

    #[test]
    fn fine_pages_select_endpoints_and_keep_legacy_controls() {
        let mut page = VelocityPage::new();
        page.on_enter();
        assert_eq!(settings::with(|s| (s.velocity_gain, s.velocity_soft)), (17, 28));
        page.on_surface(press(31));
        assert_ne!(settings::with(|s| s.velocity_enabled), 0);
        page.on_surface(press(23));
        assert_eq!(settings::with(|s| s.velocity_curve), 2);
        page.on_surface(press(82));
        page.on_surface(press(32));
        assert_eq!(settings::with(|s| s.velocity_gain), 17);
        page.on_surface(press(11));
        assert_eq!(settings::with(|s| s.velocity_gain), 0);
        page.on_surface(press(78));
        assert_eq!(settings::with(|s| s.velocity_gain), 55);
        page.on_surface(press(83));
        page.on_surface(press(45));
        assert_eq!(settings::with(|s| s.velocity_soft), 28);
        page.on_surface(press(11));
        assert_eq!(settings::with(|s| s.velocity_soft), 0);
        page.on_surface(press(78));
        assert_eq!(settings::with(|s| s.velocity_soft), 55);
        page.on_surface(press(81));
        assert_eq!(page.page, 0);
        assert_eq!(settings::with(|s| s.velocity_curve), 2);
    }
}

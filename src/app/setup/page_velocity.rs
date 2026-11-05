// SPDX-License-Identifier: GPL-3.0-only
// Velocity graph: tap empty pad to add a handle, tap a handle to remove it,
// or move to a neighboring pad before the tap grace period ends to drag it.
use crate::app::SurfaceEvent;
use crate::app::setup::page::Page;
use crate::app::velocity;
use crate::sys::{led, settings};

const BACK: u8 = 81;
const DRAG_GRACE_TICKS: u8 = 130; // Setup ticks once per millisecond.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Touch { pad: u8, handle: u8, moved: bool, created: bool }

pub struct VelocityPage {
    page: u8, // 0 = basic, 1 = Bezier graph, 2 = minimum velocity
    touch: Option<Touch>,
    pending: Option<(Touch, u8)>,
}

impl VelocityPage {
    pub const fn new() -> Self { Self { page: 0, touch: None, pending: None } }

    fn points() -> (u8, u8, u8) {
        settings::with(|s| (s.velocity_point_a, s.velocity_point_b, s.velocity_floor))
    }

    fn draw(&self) {
        let (first, second, floor) = Self::points();
        for row in 1..=8 {
            for col in 1..=8 {
                let index = row * 10 + col;
                let color = match self.page {
                    0 => match index {
                        31 => if settings::with(|s| s.velocity_enabled != 0) { 0x10ff10 } else { 0xff1010 },
                        21 | 22 | 23 => 0x304050,
                        82 => 0x00a0d0, 83 => 0xc050a0,
                        _ => 0,
                    },
                    1 => {
                        let x = ((col - 1) as u16 * 127 / 7) as u8;
                        let y = velocity::tune(x.max(1), first, second, floor);
                        let graph_row = 1 + ((y.saturating_sub(1) as u16 * 7 + 63) / 126) as u8;
                        if index == BACK { 0xff8000 }
                        else if velocity::point_to_pad(first) == Some(index) { 0x00eaff }
                        else if velocity::point_to_pad(second) == Some(index) { 0xff40c0 }
                        else if row == graph_row { 0x484848 }
                        else { 0 }
                    }
                    _ => {
                        if index == BACK { 0xff8000 }
                        else if row == 1 {
                            if col - 1 == floor { 0xffffff } else { 0x404040 }
                        } else { 0 }
                    }
                };
                led::set(index, color);
            }
        }
    }

    fn neighbor(from: u8, to: u8) -> bool {
        (from / 10).abs_diff(to / 10) <= 2
            && (from % 10).abs_diff(to % 10) <= 2
            && from != to
    }

    fn valid_point(handle: u8, point: u8) -> bool {
        let Some(pad) = velocity::point_to_pad(point) else { return false };
        let (first, second, floor) = Self::points();
        let y = (pad / 10 - 1) as u16 * 126 / 7 + 1;
        if y < velocity::floor(floor) as u16 { return false; }
        let other = if handle == 0 { second } else { first };
        if let Some(other_pad) = velocity::point_to_pad(other) {
            if pad == other_pad { return false; }
            if handle == 0 && (pad % 10 > other_pad % 10 || pad / 10 > other_pad / 10) { return false; }
            if handle == 1 && (pad % 10 < other_pad % 10 || pad / 10 < other_pad / 10) { return false; }
        }
        true
    }

    fn set_point(handle: u8, point: u8) {
        settings::update(|s| if handle == 0 { s.velocity_point_a = point; } else { s.velocity_point_b = point; });
    }

    fn delete(&mut self, touch: Touch) {
        if !touch.moved && !touch.created {
            Self::set_point(touch.handle, velocity::NO_POINT);
            self.draw();
        }
    }

    fn graph_press(&mut self, index: u8) {
        let Some(point) = velocity::pad_to_point(index) else { return; };
        if let Some(touch) = self.touch {
            if Self::neighbor(touch.pad, index) {
                if Self::valid_point(touch.handle, point) {
                    Self::set_point(touch.handle, point);
                    self.touch = Some(Touch { pad: index, moved: true, ..touch });
                    self.draw();
                } else { self.touch = Some(Touch { moved: true, ..touch }); }
                return;
            }
            self.touch = None;
            self.delete(touch);
        }
        if let Some((touch, ticks)) = self.pending.take() {
            if Self::neighbor(touch.pad, index) {
                if Self::valid_point(touch.handle, point) {
                    Self::set_point(touch.handle, point);
                    self.touch = Some(Touch { pad: index, moved: true, ..touch });
                    self.draw();
                } else { self.pending = Some((Touch { moved: true, ..touch }, ticks)); }
                return;
            }
            self.delete(touch);
        }
        let (first, second, _) = Self::points();
        if velocity::point_to_pad(first) == Some(index) {
            self.touch = Some(Touch { pad: index, handle: 0, moved: false, created: false });
        } else if velocity::point_to_pad(second) == Some(index) {
            self.touch = Some(Touch { pad: index, handle: 1, moved: false, created: false });
        } else {
            if (index / 10 - 1) as u16 * 126 / 7 + 1
                < velocity::floor(settings::with(|s| s.velocity_floor)) as u16 { return; }
            if first != velocity::NO_POINT && second == velocity::NO_POINT {
                let previous = velocity::point_to_pad(first).unwrap();
                if index % 10 <= previous % 10 && index / 10 <= previous / 10 {
                    // Keep handle A left/below B regardless of insertion order.
                    settings::update(|s| { s.velocity_point_b = first; s.velocity_point_a = velocity::NO_POINT; });
                }
            } else if first == velocity::NO_POINT && second != velocity::NO_POINT {
                let previous = velocity::point_to_pad(second).unwrap();
                if index % 10 >= previous % 10 && index / 10 >= previous / 10 {
                    settings::update(|s| { s.velocity_point_a = second; s.velocity_point_b = velocity::NO_POINT; });
                }
            }
            let (first, second, _) = Self::points();
            let handle = if first == velocity::NO_POINT { 0 }
                else if second == velocity::NO_POINT { 1 } else { return };
            if !Self::valid_point(handle, point) { return; }
            Self::set_point(handle, point);
            self.touch = Some(Touch { pad: index, handle, moved: false, created: true });
            self.draw();
        }
    }
}

impl Page for VelocityPage {
    fn on_enter(&mut self) {
        self.page = 0;
        self.touch = None;
        self.pending = None;
        led::set_raw(69, 0x551500);
        self.draw();
    }

    fn on_surface(&mut self, event: SurfaceEvent) {
        if self.page != 0 && event.index == BACK && event.pressed {
            if let Some((touch, _)) = self.pending.take() { self.delete(touch); }
            self.touch = None;
            self.page = 0;
            self.draw();
            return;
        }
        if self.page == 1 {
            if event.pressed { self.graph_press(event.index); }
            else if self.touch.is_some_and(|touch| touch.pad == event.index) {
                let touch = self.touch.take().unwrap();
                if !touch.moved { self.pending = Some((touch, 0)); }
            }
            return;
        }
        if !event.pressed { return; }
        if self.page == 2 {
            if event.index / 10 == 1 && (1..=8).contains(&(event.index % 10)) {
                settings::update(|s| {
                    s.velocity_floor = event.index % 10 - 1;
                    let min_row = 1 + (velocity::floor(s.velocity_floor) - 1).div_ceil(18);
                    for point in [&mut s.velocity_point_a, &mut s.velocity_point_b] {
                        if let Some(pad) = velocity::point_to_pad(*point) {
                            *point = velocity::pad_to_point(min_row.max(pad / 10) * 10 + pad % 10).unwrap();
                        }
                    }
                    if s.velocity_point_a == s.velocity_point_b { s.velocity_point_b = velocity::NO_POINT; }
                });
                self.draw();
            }
            return;
        }
        match event.index {
            31 => settings::update(|s| s.velocity_enabled = u8::from(s.velocity_enabled == 0)),
            82 => { self.page = 1; self.draw(); return; }
            83 => { self.page = 2; self.draw(); return; }
            21 | 22 | 23 => {
                let floor = settings::with(|s| s.velocity_floor);
                let hard_a = if floor == 7 { 43 } else if floor >= 4 { 33 } else { 23 };
                let (first, second) = match event.index {
                    21 => (velocity::pad_to_point(hard_a).unwrap(), velocity::pad_to_point(66).unwrap()),
                    22 => (velocity::NO_POINT, velocity::NO_POINT),
                    _ => (velocity::pad_to_point(53).unwrap(), velocity::pad_to_point(76).unwrap()),
                };
                settings::update(|s| { s.velocity_point_a = first; s.velocity_point_b = second; });
            }
            _ => return,
        }
        self.draw();
    }

    fn on_tick(&mut self) {
        if let Some((touch, ticks)) = self.pending {
            if ticks >= DRAG_GRACE_TICKS {
                self.pending = None;
                self.delete(touch);
            } else { self.pending = Some((touch, ticks + 1)); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::led::{self, Update};
    fn event(index: u8, pressed: bool) -> SurfaceEvent { SurfaceEvent { index, pressed, value: 90 } }

    #[test]
    fn tap_create_delete_and_drag_with_release_first_or_overlap() {
        settings::update(|s| { s.velocity_point_a = velocity::NO_POINT; s.velocity_point_b = velocity::NO_POINT; s.velocity_floor = 0; });
        let mut page = VelocityPage::new();
        page.on_enter();
        page.on_surface(event(82, true));
        page.on_surface(event(33, true));
        page.on_surface(event(33, false));
        page.on_surface(event(34, true)); // new point can also cross a pad gap
        page.on_surface(event(34, false));
        assert_eq!(velocity::point_to_pad(SelfPoint::first()), Some(34));
        page.on_surface(event(34, true));
        page.on_surface(event(34, false));
        page.on_surface(event(35, true)); // existing point, release before next press
        page.on_surface(event(35, false));
        assert_eq!(velocity::point_to_pad(SelfPoint::first()), Some(35));
        page.on_surface(event(65, true));
        page.on_surface(event(66, true)); // overlapping presses
        page.on_surface(event(66, false));
        assert_eq!(velocity::point_to_pad(SelfPoint::second()), Some(66));
        page.on_surface(event(35, true));
        page.on_surface(event(35, false));
        for _ in 0..=DRAG_GRACE_TICKS { page.on_tick(); }
        assert_eq!(SelfPoint::first(), velocity::NO_POINT);
        assert!(led::take_updates().contains(&Update::Set(81, 0xff8000)));
        page.on_surface(event(81, true));
        assert_eq!(page.page, 0);
    }

    #[test]
    fn floor_setting_keeps_existing_handles_above_the_new_floor() {
        settings::update(|s| {
            s.velocity_floor = 0;
            s.velocity_point_a = velocity::pad_to_point(23).unwrap();
            s.velocity_point_b = velocity::pad_to_point(66).unwrap();
        });
        let mut page = VelocityPage::new();
        page.on_enter();
        page.on_surface(event(83, true));
        page.on_surface(event(18, true)); // floor 48
        assert_eq!(velocity::point_to_pad(SelfPoint::first()), Some(43));
        assert_eq!(velocity::point_to_pad(SelfPoint::second()), Some(66));
        assert_eq!(settings::with(|s| s.velocity_floor), 7);
        page.on_surface(event(81, true));
        assert_eq!(page.page, 0);
    }

    #[test]
    fn crossing_handles_or_dragging_below_floor_never_deletes_a_point() {
        settings::update(|s| {
            s.velocity_floor = 5;
            s.velocity_point_a = velocity::pad_to_point(43).unwrap();
            s.velocity_point_b = velocity::pad_to_point(65).unwrap();
        });
        let mut page = VelocityPage::new();
        page.on_enter();
        page.on_surface(event(82, true));
        page.on_surface(event(43, true));
        page.on_surface(event(54, true)); // below handle B but larger x; allowed
        page.on_surface(event(54, false));
        assert_eq!(velocity::point_to_pad(SelfPoint::first()), Some(54));
        page.on_surface(event(54, true));
        page.on_surface(event(65, true)); // occupied by B: invalid move
        page.on_surface(event(54, false));
        for _ in 0..=DRAG_GRACE_TICKS { page.on_tick(); }
        assert_eq!(velocity::point_to_pad(SelfPoint::first()), Some(54));
        assert_eq!(velocity::point_to_pad(SelfPoint::second()), Some(65));
    }

    struct SelfPoint;
    impl SelfPoint {
        fn first() -> u8 { settings::with(|s| s.velocity_point_a) }
        fn second() -> u8 { settings::with(|s| s.velocity_point_b) }
    }
}

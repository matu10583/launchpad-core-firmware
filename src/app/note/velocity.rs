// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

/// Map the sensor sample at Note On into a usable 1..127 velocity.
/// The trigger/release thresholds are unchanged; only the output curve moves.
pub fn map(norm: u16, start: u16) -> u8 {
    if norm < start {
        return 1;
    }
    let delta = (norm - start) as u32;
    (1 + delta * 126 / 1100).min(127) as u8
}

#[cfg(test)]
mod tests {
    use super::map;

    #[test]
    fn light_to_strong_press_has_a_usable_range() {
        assert_eq!(map(112, 112), 1);
        assert_eq!(map(300, 112), 22);
        assert_eq!(map(600, 112), 56);
        assert_eq!(map(900, 112), 91);
        assert_eq!(map(1212, 112), 127);
        assert_eq!(map(4095, 112), 127);
    }

    #[test]
    fn selectable_floor_keeps_x_sensor_start_and_top_endpoint() {
        use crate::app::velocity::{self, NO_POINT};
        assert_eq!(velocity::tune(map(112, 112), [NO_POINT; 4], 0), 1);
        assert_eq!(velocity::tune(map(112, 112), [NO_POINT; 4], 5), 24);
        assert_eq!(velocity::tune(map(1212, 112), [NO_POINT; 4], 5), 127);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Device-independent output adjustment, after the existing sensor and curve mapping.

pub fn tune(value: u8, gain: u8, soft: u8) -> u8 {
    if value == 0 { return 0; }
    let value = value.min(127) as i32;
    let softness = soft.min(55) as i32 - 28;
    // Fade the low-end trim to zero at maximum velocity. Neither this nor
    // gain changes the hardware press/release detection or Note Off.
    let shaped = (value + softness * (127 - value) / 126).clamp(1, 127);
    let percent = 49 + 3 * gain.min(55) as i32; // 49%..214%, default 100%
    ((shaped * percent + 50) / 100).clamp(1, 127) as u8
}

pub fn encode_settings(enabled: u8, curve: u8, gain: u8, soft: u8) -> (u8, u8) {
    (0x80 | (gain.min(55) << 1) | u8::from(enabled != 0),
     (soft.min(55) << 2) | curve.min(2))
}

pub fn decode_settings(enabled: u8, curve: u8) -> (u8, u8, u8, u8) {
    if enabled & 0x80 != 0 {
        (enabled & 1, curve & 3, (enabled >> 1) & 0x3f, curve >> 2)
    } else {
        // Previous settings images contain only the toggle and 3-way curve.
        (enabled, curve, 17, 28)
    }
}

#[cfg(test)]
mod tests {
    use super::tune;

    #[test]
    fn neutral_is_identity_and_zero_stays_off() {
        for value in 0..=127 { assert_eq!(tune(value, 17, 28), value); }
        assert_eq!(tune(0, 55, 55), 0);
    }

    #[test]
    fn gain_and_softness_adjust_without_wrap_or_non_monotonic_steps() {
        assert!(tune(20, 45, 40) > tune(20, 17, 28));
        assert!(tune(20, 17, 10) < tune(20, 17, 28));
        assert_eq!(tune(127, 55, 55), 127);
        for gain in [0, 17, 55] {
            for soft in [0, 28, 55] {
                let mut prev = 0;
                for value in 1..=127 {
                    let next = tune(value, gain, soft);
                    assert!(next >= prev && next > 0);
                    prev = next;
                }
            }
        }
    }

    #[test]
    fn settings_bytes_preserve_old_images_and_roundtrip_fine_values() {
        assert_eq!(super::decode_settings(1, 2), (1, 2, 17, 28));
        for enabled in 0..=1 {
            for curve in 0..=2 {
                for gain in 0..=55 {
                    for soft in 0..=55 {
                        let (a, b) = super::encode_settings(enabled, curve, gain, soft);
                        assert_eq!(super::decode_settings(a, b), (enabled, curve, gain, soft));
                    }
                }
            }
        }
    }
}

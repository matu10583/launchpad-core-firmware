// SPDX-License-Identifier: GPL-3.0-only
//! Short-message framing shared by app-generated and USB-originated DIN TX.

pub fn message_len(status: u8) -> Option<usize> {
    match status {
        0x80..=0xbf | 0xe0..=0xef | 0xf2 => Some(3),
        0xc0..=0xdf | 0xf1 | 0xf3 => Some(2),
        0xf6 | 0xf8..=0xff => Some(1),
        _ => None,
    }
}

pub fn valid_message(data: &[u8]) -> bool {
    data.first().and_then(|&status| message_len(status)) == Some(data.len())
        && data[1..].iter().all(|&byte| byte < 0x80)
}

#[cfg(test)]
mod tests {
    use super::{message_len, valid_message};

    #[test]
    fn channel_voice_and_system_common_lengths() {
        assert_eq!(message_len(0xb0), Some(3));
        assert_eq!(message_len(0xc1), Some(2));
        assert_eq!(message_len(0xf2), Some(3));
        assert_eq!(message_len(0xf8), Some(1));
        assert_eq!(message_len(0xf0), None);
    }

    #[test]
    fn only_complete_short_midi_is_transmitted() {
        assert!(valid_message(&[0xb0, 64, 127]));
        assert!(valid_message(&[0xc1, 50]));
        assert!(valid_message(&[0xf8]));
        assert!(!valid_message(&[0xb0, 64]));
        assert!(!valid_message(&[0xb0, 64, 0x90]));
        assert!(!valid_message(&[0xf0, 1, 0xf7]));
        assert!(!valid_message(&[]));
    }
}

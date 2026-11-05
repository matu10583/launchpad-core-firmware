// SPDX-License-Identifier: GPL-3.0-only
//! X Note preferences in existing spare settings bits (no wire-size change).

pub struct NotePreferences {
    pub scale_mode: u8,
    pub scale: u8,
    pub root: u8,
    pub overlap: u8,
    pub channel: u8,
}

pub fn pack(brightness: u8, aftertouch_mode: u8, aftertouch_curve: u8,
            prefs: NotePreferences) -> (u8, u8, u8) {
    (brightness & 7 | (prefs.scale.min(15) << 3) | (prefs.scale_mode.min(1) << 7),
     aftertouch_mode & 3 | (prefs.root.min(11) << 2) | ((prefs.overlap.min(5) & 3) << 6),
     aftertouch_curve & 3 | 0x80 | ((prefs.overlap.min(5) >> 2) << 2)
         | (prefs.channel.min(15) << 3))
}

pub fn unpack(brightness: u8, aftertouch_mode: u8, aftertouch_curve: u8)
              -> Option<(u8, u8, u8, NotePreferences)> {
    if aftertouch_curve & 0x80 == 0 { return None; }
    Some((brightness & 7, aftertouch_mode & 3, aftertouch_curve & 3,
          NotePreferences {
              scale_mode: brightness >> 7,
              scale: brightness >> 3 & 15,
              root: aftertouch_mode >> 2 & 15,
              overlap: (aftertouch_mode >> 6 & 3) | ((aftertouch_curve >> 2 & 1) << 2),
              channel: aftertouch_curve >> 3 & 15,
          }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_images_keep_defaults_and_new_images_preserve_all_note_controls() {
        assert!(unpack(7, 2, 1).is_none());
        for scale in 0..16 {
            for root in 0..12 {
                for overlap in [0, 2, 3, 4, 5] {
                    for channel in 0..16 {
                        let (b, m, c) = pack(6, 2, 1, NotePreferences {
                            scale_mode: 1, scale, root, overlap, channel,
                        });
                        let (old_b, old_m, old_c, prefs) = unpack(b, m, c).unwrap();
                        assert_eq!((old_b, old_m, old_c), (6, 2, 1));
                        assert_eq!((prefs.scale_mode, prefs.scale, prefs.root,
                                    prefs.overlap, prefs.channel), (1, scale, root, overlap, channel));
                    }
                }
            }
        }
    }
}

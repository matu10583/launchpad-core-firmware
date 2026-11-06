// SPDX-License-Identifier: GPL-3.0-only
//! Launchpad X standalone chromatic/scale note positions.

pub const SCALE_COUNT: u8 = 16;

pub fn grid_slot(index: u8) -> Option<usize> {
    let row = index / 10;
    let col = index % 10;
    if !(1..=8).contains(&row) || !(1..=8).contains(&col) { return None; }
    Some(((row - 1) * 8 + col - 1) as usize)
}

pub fn intervals(scale: u8) -> &'static [u8] {
    match scale {
        0 => &[0, 2, 3, 5, 7, 8, 10], // natural minor
        1 => &[0, 2, 4, 5, 7, 9, 11], // major
        2 => &[0, 2, 3, 5, 7, 9, 10], // dorian
        3 => &[0, 1, 3, 5, 7, 8, 10], // phrygian
        4 => &[0, 2, 4, 5, 7, 9, 10], // mixolydian
        5 => &[0, 2, 3, 5, 7, 9, 11], // melodic minor (ascending)
        6 => &[0, 2, 3, 5, 7, 8, 11], // harmonic minor
        7 => &[0, 2, 3, 4, 5, 7, 9, 10], // bebop dorian
        8 => &[0, 3, 5, 6, 7, 10], // blues
        9 => &[0, 3, 5, 7, 10], // minor pentatonic
        10 => &[0, 2, 3, 6, 7, 8, 11], // Hungarian minor
        11 => &[0, 2, 3, 6, 7, 9, 10], // Ukrainian dorian
        12 => &[0, 1, 4, 6, 7, 9, 11], // Marva
        13 => &[0, 1, 3, 6, 7, 8, 11], // Todi
        14 => &[0, 2, 4, 6, 8, 10], // whole tone
        _ => &[0, 2, 3, 7, 8], // Hirajoshi
    }
}

pub fn note(index: u8, root: u8, scale: u8, overlap: u8, scale_mode: bool, shift: i16) -> Option<u8> {
    let row = index / 10;
    let col = index % 10;
    if !(1..=8).contains(&row) || !(1..=8).contains(&col) { return None; }
    let degrees = intervals(scale);
    let stride = if overlap == 0 {
        if scale_mode { degrees.len() as i16 } else { 8 }
    } else { overlap.clamp(2, 5) as i16 };
    let position = (row - 1) as i16 * stride + (col - 1) as i16;
    let interval = if scale_mode {
        position / degrees.len() as i16 * 12 + degrees[position as usize % degrees.len()] as i16
    } else { position };
    let pitch = 48 + root.min(11) as i16 + interval + shift;
    (0..=127).contains(&pitch).then_some(pitch as u8)
}

pub fn color(pitch: u8, root: u8, scale: u8, scale_mode: bool) -> u32 {
    let degree = (pitch as i16 - root as i16).rem_euclid(12) as u8;
    if degree == 0 { 0xff47ce }
    else if intervals(scale).contains(&degree) { 0x239fd1 }
    else if scale_mode { 0 } else { 0x181818 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_five_finger_chromatic_and_scale_positions() {
        assert_eq!(note(11, 0, 0, 5, false, 0), Some(48));
        assert_eq!(note(12, 0, 0, 5, false, 0), Some(49));
        assert_eq!(note(21, 0, 0, 5, false, 0), Some(53));
        assert_eq!(note(16, 0, 0, 5, false, 0), note(21, 0, 0, 5, false, 0));
        assert_eq!(note(32, 0, 0, 5, false, 0), Some(59));
        assert_eq!(note(12, 0, 0, 5, true, 0), Some(50));
        assert_eq!(note(11, 7, 1, 5, true, 0), Some(55));
        assert_eq!(note(11, 0, 0, 0, false, 0), Some(48));
        assert_eq!(note(21, 0, 0, 0, false, 0), Some(56));
        assert_eq!(note(21, 0, 0, 0, true, 0), Some(60));
        assert_eq!(color(49, 0, 0, false), 0x181818);
    }
}

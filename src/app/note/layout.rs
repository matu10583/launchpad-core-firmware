// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

/// Fixed layouts for the Launchpad X. Coordinates use 11 for the bottom-left
/// grid pad and 88 for the top-right grid pad.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Chromatic,
    Major,
    Classic,
}

impl Preset {
    pub fn from_slot(slot: u8) -> Option<Self> {
        match slot {
            0 => Some(Self::Chromatic),
            1 => Some(Self::Major),
            2 => Some(Self::Classic),
            _ => None,
        }
    }

    pub fn slot(self) -> u8 {
        match self {
            Self::Chromatic => 0,
            Self::Major => 1,
            Self::Classic => 2,
        }
    }
}

pub fn grid_slot(index: u8) -> Option<usize> {
    let row = index / 10;
    let col = index % 10;
    if !(1..=8).contains(&row) || !(1..=8).contains(&col) {
        return None;
    }
    Some(((row - 1) * 8 + col - 1) as usize)
}

pub fn note_for(preset: Preset, index: u8, transpose: i16) -> Option<u8> {
    grid_slot(index)?;
    let row = (index / 10 - 1) as i16;
    let col = (index % 10 - 1) as i16;
    let step = row * 4 + col % 4;
    let note = match preset {
        // Four semitones per row; the right half repeats one octave higher.
        Preset::Chromatic => 48 + step + (col / 4) * 12,
        // Four C-major scale degrees per row; right half is an octave higher.
        Preset::Major => {
            let degree = step + (col / 4) * 7;
            const SCALE: [i16; 7] = [0, 2, 4, 5, 7, 9, 11];
            48 + (degree / 7) * 12 + SCALE[(degree % 7) as usize]
        }
        // The original CoreFW Performance grid: 36..67 / 68..99.
        Preset::Classic => 36 + step + (col / 4) * 32,
    } + transpose;

    (0..=127).contains(&note).then_some(note as u8)
}

pub fn base_color(note: u8, transpose: i16) -> u32 {
    // Scale colors follow the transposed tonic (e.g. C# becomes do at +1).
    let degree = (note as i16 - transpose).rem_euclid(12);
    if degree == 0 {
        0xff47ce // C (do): pink
    } else if matches!(degree, 2 | 4 | 5 | 7 | 9 | 11) {
        0x239fd1 // Other C-major notes: blue
    } else {
        0x181818 // Chromatic notes outside the scale: dark grey
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromatic_four_step_rows_repeat_an_octave_on_the_right() {
        assert_eq!(note_for(Preset::Chromatic, 11, 0), Some(48));
        assert_eq!(note_for(Preset::Chromatic, 14, 0), Some(51));
        assert_eq!(note_for(Preset::Chromatic, 15, 0), Some(60));
        assert_eq!(note_for(Preset::Chromatic, 21, 0), Some(52));
        assert_eq!(note_for(Preset::Chromatic, 41, 0), Some(60));
        assert_eq!(note_for(Preset::Chromatic, 88, 0), Some(91));
    }

    #[test]
    fn major_and_classic_layouts_have_distinct_notes() {
        assert_eq!(Preset::from_slot(0).map(Preset::slot), Some(0));
        assert_eq!(Preset::from_slot(1).map(Preset::slot), Some(1));
        assert_eq!(Preset::from_slot(2).map(Preset::slot), Some(2));
        assert_eq!(Preset::from_slot(3).map(Preset::slot), None);
        assert_eq!(note_for(Preset::Major, 12, 0), Some(50));
        assert_eq!(note_for(Preset::Major, 15, 0), Some(60));
        assert_eq!(note_for(Preset::Classic, 11, 0), Some(36));
        assert_eq!(note_for(Preset::Classic, 15, 0), Some(68));
        assert_eq!(note_for(Preset::Classic, 88, 0), Some(99));
    }

    #[test]
    fn colors_follow_pitch_class_and_transposition() {
        let c = note_for(Preset::Chromatic, 11, 1).unwrap();
        assert_eq!(c, 49);
        assert_eq!(base_color(c, 1), 0xff47ce);
        assert_eq!(base_color(48, 1), 0x239fd1); // B relative to C#
        assert_eq!(base_color(50, 0), 0x239fd1);
        assert_eq!(base_color(49, 0), 0x181818);
        assert_eq!(note_for(Preset::Major, 11, 12), Some(60));
        assert_eq!(grid_slot(95), None);
        assert_eq!(note_for(Preset::Chromatic, 11, -49), None);
    }
}

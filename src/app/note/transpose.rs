// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

const MIN_TRANSPOSE: i16 = -24;
const MAX_TRANSPOSE: i16 = 12;

/// Keep the octave and semitone button histories independent so resetting
/// either pair never erases the contribution made by the other pair.
#[derive(Clone, Copy)]
pub struct Transpose {
    pub octaves: i8,
    pub semitones: i8,
}

impl Transpose {
    pub const fn new() -> Self {
        Self { octaves: 0, semitones: 0 }
    }

    pub fn total(self) -> i16 {
        self.octaves as i16 * 12 + self.semitones as i16
    }

    pub fn shifted(self, step: i16) -> Option<Self> {
        let mut next = self;
        match step {
            12 if self.octaves < 1 => next.octaves += 1,
            -12 if self.octaves > -2 => next.octaves -= 1,
            1 if self.semitones < 12 => next.semitones += 1,
            -1 if self.semitones > -24 => next.semitones -= 1,
            _ => return None,
        }
        (MIN_TRANSPOSE..=MAX_TRANSPOSE).contains(&next.total()).then_some(next)
    }

    pub fn without_octaves(self) -> Self {
        Self { octaves: 0, ..self }
    }

    pub fn without_semitones(self) -> Self {
        Self { semitones: 0, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::Transpose;

    #[test]
    fn each_pair_resets_only_its_own_offset() {
        let down_one_and_up_three = Transpose::new()
            .shifted(-12).unwrap()
            .shifted(1).unwrap()
            .shifted(1).unwrap()
            .shifted(1).unwrap();
        assert_eq!(down_one_and_up_three.total(), -9);
        assert_eq!(down_one_and_up_three.without_octaves().total(), 3);
        assert_eq!(down_one_and_up_three.without_semitones().total(), -12);
        assert_eq!(down_one_and_up_three.without_octaves().without_semitones().total(), 0);
    }

    #[test]
    fn keep_independent_components_within_the_original_total_range() {
        let up_one = Transpose::new().shifted(12).unwrap();
        assert!(up_one.shifted(1).is_none());
        let down_two = Transpose::new().shifted(-12).unwrap().shifted(-12).unwrap();
        assert!(down_two.shifted(-1).is_none());
        assert!(down_two.without_octaves().shifted(-1).is_some());
        let down_semis = Transpose::new().shifted(-1).unwrap();
        assert_eq!(down_semis.without_octaves().total(), -1);
        assert_eq!(down_semis.without_semitones().total(), 0);
    }
}

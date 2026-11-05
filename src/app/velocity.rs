// SPDX-License-Identifier: GPL-3.0-only
// Two movable Bezier handles on the 8x8 pad grid. 63 means "no handle":
// grid pad 81 is reserved for Back, leaving exactly 63 editable positions.
pub const NO_POINT: u8 = 63;
pub const FLOORS: [u8; 8] = [1, 4, 8, 12, 16, 24, 32, 48];

pub fn pad_to_point(index: u8) -> Option<u8> {
    let row = index / 10;
    let col = index % 10;
    if !(1..=8).contains(&row) || !(1..=8).contains(&col) || index == 81 { return None; }
    let slot = (row - 1) * 8 + col - 1;
    Some(if slot > 56 { slot - 1 } else { slot })
}

pub fn point_to_pad(point: u8) -> Option<u8> {
    if point >= NO_POINT { return None; }
    let slot = if point >= 56 { point + 1 } else { point };
    Some((slot / 8 + 1) * 10 + slot % 8 + 1)
}

pub fn floor(index: u8) -> u8 { FLOORS[index.min(7) as usize] }

// The two settings bytes are now two six-bit handle positions, a three-bit
// floor selection, and the enabled bit. No extra flash bytes are needed.
pub fn encode_settings(enabled: u8, floor_index: u8, first: u8, second: u8) -> (u8, u8) {
    let bits = (first.min(NO_POINT) as u16)
        | ((second.min(NO_POINT) as u16) << 6)
        | ((floor_index.min(7) as u16) << 12)
        | ((u16::from(enabled != 0)) << 15);
    (bits as u8, (bits >> 8) as u8)
}

pub fn decode_settings(low: u8, high: u8) -> (u8, u8, u8, u8) {
    let bits = u16::from_le_bytes([low, high]);
    ((bits >> 15) as u8, ((bits >> 12) & 7) as u8,
        (bits & 63) as u8, ((bits >> 6) & 63) as u8)
}

fn lerp(a: i32, b: i32, t: i32) -> i32 { a + (b - a) * t / 256 }

fn bezier(a: i32, b: i32, c: i32, d: i32, t: i32) -> i32 {
    let ab = lerp(a, b, t);
    let bc = lerp(b, c, t);
    let cd = lerp(c, d, t);
    lerp(lerp(ab, bc, t), lerp(bc, cd, t), t)
}

fn coord(point: u8, start: i32) -> Option<(i32, i32)> {
    let pad = point_to_pad(point)?;
    let x = (pad % 10 - 1) as i32 * 127 / 7;
    let y = (pad / 10 - 1) as i32 * 126 / 7 + 1;
    (y >= start).then_some((x, y))
}

fn handles(first: u8, second: u8, start: i32) -> ((i32, i32), (i32, i32)) {
    let left = (42, start + (127 - start) / 3);
    let right = (85, start + (127 - start) * 2 / 3);
    match (coord(first, start), coord(second, start)) {
        (Some(a), Some(b)) if a.0 <= b.0 && a.1 <= b.1 => (a, b),
        (Some(a), Some(b)) if b.0 <= a.0 && b.1 <= a.1 => (b, a),
        (Some(a), _) | (_, Some(a)) => {
            if a.0 <= 63 { (a, (right.0.max(a.0), right.1.max(a.1))) }
            else { ((left.0.min(a.0), left.1.min(a.1)), a) }
        }
        _ => (left, right),
    }
}

/// Preserve both endpoints; evaluate x(t) by binary search and then y(t).
/// This runs only on Note On (and eight times when drawing the setup graph).
pub fn tune(value: u8, first: u8, second: u8, floor_index: u8) -> u8 {
    if value == 0 { return 0; }
    let start = floor(floor_index) as i32;
    if value == 1 { return start as u8; }
    if value >= 127 { return 127; }
    let (a, b) = handles(first, second, start);
    let mut lo = 0;
    let mut hi = 256;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if bezier(0, a.0, b.0, 127, mid) < value as i32 { lo = mid; }
        else { hi = mid; }
    }
    bezier(start, a.1, b.1, 127, hi).clamp(start, 127) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_roundtrip_with_reserved_back_and_two_bytes_roundtrip() {
        assert_eq!(pad_to_point(81), None);
        assert_eq!(pad_to_point(0), None);
        for row in 1..=8 {
            for col in 1..=8 {
                let pad = row * 10 + col;
                if pad != 81 { assert_eq!(point_to_pad(pad_to_point(pad).unwrap()), Some(pad)); }
            }
        }
        for first in 0..=NO_POINT {
            let (low, high) = encode_settings(1, 5, first, NO_POINT);
            assert_eq!(decode_settings(low, high), (1, 5, first, NO_POINT));
        }
    }

    #[test]
    fn curves_keep_endpoints_and_never_reverse() {
        for first in [NO_POINT, 11, 22, 30, 40, 51, 62] {
            for second in [NO_POINT, 10, 23, 32, 44, 54, 61] {
                for floor_index in [0, 5, 7] {
                    let mut previous = 0;
                    for value in 1..=127 {
                        let next = tune(value, first, second, floor_index);
                        assert!(next >= previous, "{first} {second} {floor_index} {value}");
                        previous = next;
                    }
                    assert_eq!(tune(1, first, second, floor_index), floor(floor_index));
                    assert_eq!(tune(127, first, second, floor_index), 127);
                }
            }
        }
        assert_eq!(tune(0, NO_POINT, NO_POINT, 0), 0);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// Up to four editable curve anchors on the 8x8 pad grid. 63 means "no point":
// grid pad 81 is reserved for Back, leaving exactly 63 editable positions.
pub const NO_POINT: u8 = 63;
pub const MAX_POINTS: usize = 4;
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

// The original two settings bytes hold the first two six-bit positions,
// the floor selection, and enabled. Positions three and four are appended
// after the palette in the new settings image.
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
    // Edge columns remain editable, but never replace the fixed endpoints.
    let x = ((pad % 10 - 1) as i32 * 127 / 7).clamp(2, 126);
    let y = (pad / 10 - 1) as i32 * 126 / 7 + 1;
    (y >= start).then_some((x, y))
}

/// Monotone piecewise cubic Bezier through the selected points. Tangents at
/// each join come from neighboring secants; Bezier controls are clamped into
/// the segment so integer rounding cannot produce a velocity reversal.
pub fn tune(value: u8, points: [u8; MAX_POINTS], floor_index: u8) -> u8 {
    if value == 0 { return 0; }
    let start = floor(floor_index) as i32;
    if value == 1 { return start as u8; }
    if value >= 127 { return 127; }
    let mut anchors = [(0i32, 0i32); MAX_POINTS + 2];
    anchors[0] = (1, start);
    let mut len = 1;
    for point in points {
        if let Some((x, y)) = coord(point, start) {
            if x > anchors[len - 1].0 && y >= anchors[len - 1].1 {
                anchors[len] = (x, y);
                len += 1;
            }
        }
    }
    anchors[len] = (127, 127);
    len += 1;
    let x = value as i32;
    for i in 0..len - 1 {
        let (x0, y0) = anchors[i];
        let (x1, y1) = anchors[i + 1];
        if x > x1 { continue; }
        let width = x1 - x0;
        let height = y1 - y0;
        let slope = |a: usize, b: usize| {
            (anchors[b].1 - anchors[a].1) * 256 / (anchors[b].0 - anchors[a].0)
        };
        let tangent = |prev: Option<usize>, next: Option<usize>| {
            match (prev, next) {
                (Some(p), Some(n)) => {
                    let left = slope(p, p + 1);
                    let right = slope(n - 1, n);
                    if left == 0 || right == 0 { 0 }
                    else { ((left + right) / 2).min(2 * left.min(right)) }
                }
                _ => slope(i, i + 1),
            }
        };
        let m0 = tangent(i.checked_sub(1), Some(i + 1));
        let m1 = tangent(Some(i), (i + 2 < len).then_some(i + 2));
        let control_a = y0 + (m0 * width / 768).min(height / 2);
        let control_b = y1 - (m1 * width / 768).min(height / 2);
        let t = (x - x0) * 256 / width;
        return bezier(y0, control_a, control_b, y1, t).clamp(start, 127) as u8;
    }
    127
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
                        let next = tune(value, [first, second, NO_POINT, NO_POINT], floor_index);
                        assert!(next >= previous, "{first} {second} {floor_index} {value}");
                        previous = next;
                    }
                    assert_eq!(tune(1, [first, second, NO_POINT, NO_POINT], floor_index), floor(floor_index));
                    assert_eq!(tune(127, [first, second, NO_POINT, NO_POINT], floor_index), 127);
                }
            }
        }
        assert_eq!(tune(0, [NO_POINT; MAX_POINTS], 0), 0);
        for floor_index in [0, 5, 7] {
            let pads = [43u8, 54, 65, 76];
            let points = pads.map(|pad| pad_to_point(pad).unwrap());
            let mut previous = 0;
            for value in 1..=127 {
                let next = tune(value, points, floor_index);
                assert!(next >= previous);
                previous = next;
            }
            for pad in pads {
                let x = ((pad % 10 - 1) as u16 * 127 / 7) as u8;
                let y = (pad / 10 - 1) * 18 + 1;
                assert_eq!(tune(x, points, floor_index), y);
            }
        }
    }
}

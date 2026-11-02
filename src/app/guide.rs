// SPDX-License-Identifier: GPL-3.0-only
// Only a deliberately configured build may enable this optional SysEx protocol.
// Do not set this ID in public CI or publish a build containing a private ID.
const fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        b'a'..=b'f' => b - b'a' + 10,
        _ => panic!("CORE_GUIDE_SYSEX_ID must be two hexadecimal digits"),
    }
}
const fn configured_id() -> Option<u8> {
    match option_env!("CORE_GUIDE_SYSEX_ID") {
        None => None,
        Some(value) => {
            let bytes = value.as_bytes();
            if bytes.len() != 2 { panic!("CORE_GUIDE_SYSEX_ID must be two hexadecimal digits"); }
            let id = hex_digit(bytes[0]) * 16 + hex_digit(bytes[1]);
            if id == 0 || id >= 0x80 { panic!("CORE_GUIDE_SYSEX_ID must be a one-byte SysEx ID"); }
            Some(id)
        }
    }
}
// Public GitHub Actions builds must never embed an explicitly supplied ID.
const _: () = assert!(option_env!("GITHUB_ACTIONS").is_none() ||
                      option_env!("CORE_GUIDE_SYSEX_ID").is_none(),
                      "Configured guide ID cannot run in public CI");
#[cfg(not(test))]
const MANUFACTURER_ID: Option<u8> = configured_id();
// Host tests exercise framing even when the default firmware build disables it.
#[cfg(test)]
const MANUFACTURER_ID: Option<u8> = match configured_id() { Some(id) => Some(id), None => Some(1) };
const HEADER: [u8; 3] = [b'C', b'F', b'W'];
pub const MODEL_X: u8 = 0x0c;
pub const MODEL_PRO_MK3: u8 = 0x0e;
const VERSION: u8 = 1;
pub const LEASE_TICKS: u16 = 5_000; // Both supported hosts tick once per ms.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Query,
    Layout,
    Begin,
    Keepalive,
    End,
    Set { layer: u8, side: u8, pitch: u8, color: u8 },
}

pub fn decode(data: &[u8], model: u8) -> Option<Command> {
    let id = MANUFACTURER_ID?;
    if !matches!(model, MODEL_X | MODEL_PRO_MK3) { return None; }
    let start = 2 + HEADER.len() + 2;
    if data.len() < start + 2 || data.first() != Some(&0xf0)
        || data.get(1) != Some(&id)
        || !data[2..].starts_with(&HEADER)
        || data.get(start - 2) != Some(&model)
        || data.get(start - 1) != Some(&VERSION)
        || data.last() != Some(&0xf7)
        || data[1..data.len() - 1].iter().any(|&b| b > 127)
    {
        return None;
    }
    match &data[start..data.len() - 1] {
        [0x01] => Some(Command::Query),
        [0x02] => Some(Command::Layout),
        [0x10] => Some(Command::Begin),
        [0x11] => Some(Command::Keepalive),
        [0x12] => Some(Command::End),
        [0x20, layer @ 0..=1, side @ 0..=2, pitch, color] => Some(Command::Set {
            layer: *layer, side: *side, pitch: *pitch, color: *color,
        }),
        _ => None,
    }
}

pub fn reply(payload: &[u8], model: u8, out: &mut [u8]) -> Option<usize> {
    let id = MANUFACTURER_ID?;
    if !matches!(model, MODEL_X | MODEL_PRO_MK3) { return None; }
    let start = 2 + HEADER.len() + 2;
    let len = start + payload.len() + 1;
    if out.len() < len || payload.iter().any(|&b| b > 127) { return None; }
    out[0] = 0xf0;
    out[1] = id;
    let header_start = 2;
    out[header_start..header_start + HEADER.len()].copy_from_slice(&HEADER);
    out[start - 2] = model;
    out[start - 1] = VERSION;
    out[start..len - 1].copy_from_slice(payload);
    out[len - 1] = 0xf7;
    Some(len)
}

// Pro Mk3 Note/Custom share this read-only layout snapshot. Signed offset is
// encoded as two 7-bit bytes with an 8192 bias; X retains its v1 response.
pub fn pro_layout(layout: u8, root: u8, scale: u8, overlap: u8, offset: i16,
                  mut pitch: impl FnMut(u8) -> Option<u8>) -> [u8; 81] {
    let mut payload = [0u8; 81];
    let biased = (offset as i32 + 8192) as u16;
    payload[..7].copy_from_slice(&[
        0x42, layout, root, scale, overlap, (biased & 127) as u8, (biased >> 7) as u8,
    ]);
    for slot in 0..64 {
        let index = (slot / 8 + 1) as u8 * 10 + (slot % 8 + 1) as u8;
        if let Some(note) = pitch(index) {
            payload[7 + slot / 7] |= 1 << (slot % 7);
            payload[17 + slot] = note;
        }
    }
    payload
}

pub struct Guide {
    // [side: left/right][layer: preview/current][MIDI pitch]
    colors: [[[u8; 128]; 2]; 2],
    active: bool,
    remaining: u16,
}

impl Guide {
    pub const fn new() -> Self {
        Self { colors: [[[0; 128]; 2]; 2], active: false, remaining: 0 }
    }
    pub fn active(&self) -> bool { self.active }
    pub fn begin(&mut self) {
        self.colors = [[[0; 128]; 2]; 2];
        self.active = true;
        self.remaining = LEASE_TICKS;
    }
    pub fn renew(&mut self) {
        if self.active { self.remaining = LEASE_TICKS; }
    }
    pub fn end(&mut self) {
        self.colors = [[[0; 128]; 2]; 2];
        self.active = false;
        self.remaining = 0;
    }
    pub fn tick(&mut self) -> bool {
        if !self.active { return false; }
        self.remaining = self.remaining.saturating_sub(1);
        if self.remaining != 0 { return false; }
        self.end();
        true
    }
    pub fn set(&mut self, layer: u8, side: u8, pitch: u8, color: u8) -> bool {
        if !self.active || layer > 1 || side > 2 || pitch > 127 || color > 127 {
            return false;
        }
        for target in 0..2 {
            if side == target || side == 2 {
                self.colors[target as usize][layer as usize][pitch as usize] = color;
            }
        }
        self.renew();
        true
    }
    pub fn color(&self, side: usize, layer: usize, pitch: u8) -> u8 {
        if self.active && side < 2 && layer < 2 { self.colors[side][layer][pitch as usize] }
        else { 0 }
    }
    pub fn pad_color(&self, index: u8, pitch: Option<u8>, held: bool,
                     mut preferred_side_has_pitch: impl FnMut(usize, u8) -> bool) -> Option<u8> {
        let col = index % 10;
        if !self.active || !(1..=8).contains(&(index / 10)) || !(1..=8).contains(&col) {
            return None;
        }
        let pitch = pitch?;
        let side = usize::from(col > 4);
        // Current always wins over preview, including when a hand falls back
        // to the other half. Same-layer collisions favor the local hand.
        for layer in [1, 0] {
            if held && layer == 0 { break; }
            let local = self.color(side, layer, pitch);
            if local != 0 { return Some(local); }
            let other_side = 1 - side;
            let fallback = self.color(other_side, layer, pitch);
            if fallback != 0 && !preferred_side_has_pitch(other_side, pitch) {
                return Some(fallback);
            }
        }
        None
    }
}

// Recompute from the live mode layout, never from cached/host-supplied pad IDs.
pub fn side_has_pitch(side: usize, pitch: u8, mut note_for: impl FnMut(u8) -> Option<u8>) -> bool {
    if side > 1 { return false; }
    for row in 1..=8 {
        for col in 1..=4 {
            if note_for(row * 10 + side as u8 * 4 + col) == Some(pitch) { return true; }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_envelope_and_command_validation() {
        let set = [0xf0, MANUFACTURER_ID.unwrap(), 67, 70, 87, 12, 1, 0x20, 1, 2, 127, 127, 0xf7];
        assert_eq!(decode(&set, MODEL_X), Some(Command::Set { layer: 1, side: 2, pitch: 127, color: 127 }));
        let mut pro = set;
        pro[5] = MODEL_PRO_MK3;
        assert_eq!(decode(&pro, MODEL_PRO_MK3), decode(&set, MODEL_X));
        assert_eq!(decode(&pro, MODEL_X), None);
        for (at, value) in [(1, 0x7c), (2, 0), (5, 14), (6, 2), (8, 2), (9, 3), (10, 128)] {
            let mut bad = set;
            bad[at] = value;
            assert_eq!(decode(&bad, MODEL_X), None);
        }
        assert_eq!(decode(&set[..set.len() - 1], MODEL_X), None);
        let mut trailing = set.to_vec();
        trailing.push(0);
        assert_eq!(decode(&trailing, MODEL_X), None);
        for (command, expected) in [(1, Command::Query), (2, Command::Layout),
                                    (0x10, Command::Begin), (0x11, Command::Keepalive),
                                    (0x12, Command::End)] {
            assert_eq!(decode(&[&set[..7], &[command, 0xf7]].concat(), MODEL_X), Some(expected));
        }
        let mut response = [0; 12];
        assert_eq!(reply(&[0x41, 1], MODEL_X, &mut response), Some(10));
        let expected = [&set[..7], &[0x41, 1, 0xf7]].concat();
        assert_eq!(&response[..10], expected.as_slice());
    }

    #[test]
    fn side_layer_and_pitch_state_expire_or_clear_independently() {
        let mut state = Guide::new();
        assert!(!state.set(0, 0, 48, 13));
        state.begin();
        assert!(state.set(0, 0, 48, 13));
        assert!(state.set(1, 1, 48, 21));
        assert_eq!(state.color(0, 0, 48), 13);
        assert_eq!(state.color(1, 0, 48), 0);
        assert_eq!(state.color(1, 1, 48), 21);
        assert!(state.set(0, 2, 49, 9));
        assert!(state.set(0, 0, 49, 0));
        assert_eq!(state.color(0, 0, 49), 0);
        assert_eq!(state.color(1, 0, 49), 9);
        assert!(!state.set(0, 3, 49, 9));
        for _ in 0..LEASE_TICKS - 1 { assert!(!state.tick()); }
        assert!(state.tick());
        assert!(!state.active());
        assert_eq!(state.color(1, 1, 48), 0);
    }

    #[test]
    fn current_beats_preview_and_held_pad_only_hides_preview() {
        let mut state = Guide::new();
        state.begin();
        state.set(0, 0, 48, 13);
        state.set(1, 1, 48, 21);
        assert_eq!(state.pad_color(11, Some(48), false, |_, _| true), Some(13));
        assert_eq!(state.pad_color(11, Some(48), true, |_, _| true), None);
        assert_eq!(state.pad_color(15, Some(48), false, |_, _| true), Some(21));
        assert_eq!(state.pad_color(15, Some(48), true, |_, _| true), Some(21));
        assert_eq!(state.pad_color(15, Some(49), false, |_, _| true), None);
        assert_eq!(state.pad_color(91, Some(48), false, |_, _| true), None);
        assert_eq!(state.pad_color(11, None, false, |_, _| true), None);
    }

    #[test]
    fn pro_layout_has_consistent_validity_offset_and_model_framing() {
        let payload = pro_layout(2, 0, 0, 0, -12, |index| match index {
            11 => Some(0), 15 => Some(127), 88 => Some(72), _ => None,
        });
        assert_eq!(&payload[..7], &[0x42, 2, 0, 0, 0, 116, 63]);
        assert_eq!(payload[7], 0b0001_0001);
        assert_eq!(payload[16], 0b0000_0001);
        assert_eq!(payload[17], 0);
        assert_eq!(payload[21], 127);
        assert_eq!(payload[80], 72);
        let mut frame = [0u8; 96];
        assert_eq!(reply(&payload, MODEL_PRO_MK3, &mut frame), Some(89));
        assert_eq!(&frame[..8], &[0xf0, MANUFACTURER_ID.unwrap(), b'C', b'F', b'W', 0x0e, 1, 0x42]);
        assert_eq!(frame[88], 0xf7);
        assert!(reply(&payload, MODEL_X, &mut [0; 88]).is_none());
    }

    #[test]
    fn pro_custom_duplicate_notes_follow_transpose_and_side() {
        let pitch = |index: u8, transpose: i16| {
            let row = index / 10;
            let col = index % 10;
            if !(1..=8).contains(&row) || !(1..=8).contains(&col) { return None; }
            u8::try_from(48 + (row as i16 - 1) * 4 + (col as i16 - 1) % 4
                + (if col >= 5 { 24 } else { 0 }) + transpose).ok().filter(|&p| p <= 127)
        };
        let mut guide = Guide::new();
        guide.begin();
        guide.set(0, 1, 72, 21);
        let color = |guide: &Guide, index, transpose, held| guide.pad_color(
            index, pitch(index, transpose), held,
            |side, note| side_has_pitch(side, note, |pad| pitch(pad, transpose)));
        assert_eq!(color(&guide, 15, 0, false), Some(21));
        assert_eq!(color(&guide, 71, 0, false), None); // left half
        guide.set(1, 0, 72, 13);
        assert_eq!(color(&guide, 71, 0, false), Some(13));
        assert_eq!(color(&guide, 15, 12, false), None);
        assert_eq!(color(&guide, 41, 12, false), Some(13));
        guide.end();
        assert_eq!(color(&guide, 41, 12, false), None);
    }

    #[test]
    fn fallback_uses_opposite_half_only_when_preferred_half_has_no_pitch() {
        // MIDI 70 exists only on the left; when both sides have it, right
        // hand stays on the right and never duplicates its color on the left.
        let mut guide = Guide::new();
        guide.begin();
        guide.set(0, 1, 70, 21);
        assert_eq!(guide.pad_color(11, Some(70), false, |_, _| false), Some(21));
        assert_eq!(guide.pad_color(11, Some(70), false, |side, _| side == 1), None);
        assert_eq!(guide.pad_color(15, Some(70), false, |_, _| true), Some(21));
        guide.set(0, 0, 70, 13);
        assert_eq!(guide.pad_color(15, Some(70), false, |_, _| false), Some(21));
        guide.set(1, 1, 70, 31);
        assert_eq!(guide.pad_color(11, Some(70), false, |_, _| false), Some(31));
        guide.set(1, 0, 70, 42);
        assert_eq!(guide.pad_color(11, Some(70), false, |_, _| false), Some(42));
        assert_eq!(guide.pad_color(11, Some(70), true, |_, _| false), Some(42));
        guide.set(1, 0, 70, 0);
        assert_eq!(guide.pad_color(11, Some(70), true, |_, _| false), Some(31));
        guide.set(1, 1, 70, 0);
        assert_eq!(guide.pad_color(11, Some(70), true, |_, _| false), None);
        assert_eq!(guide.pad_color(11, Some(70), false, |_, _| false), Some(13));
        guide.end();
        assert_eq!(guide.pad_color(11, Some(70), false, |_, _| false), None);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
// PRIVATE ONE-OFF PROTOTYPE. Do not push this branch to a public repository.
// 0x7D must be replaced with an authorized ID before any public distribution.

// Replace only this ID with an owned 1- or 3-byte ID before any public release;
// the parser and response builder handle either length.
const MANUFACTURER_ID: &[u8] = &[0x7d];
const HEADER: [u8; 5] = [b'C', b'F', b'W', 0x0c, 1];
pub const LEASE_TICKS: u16 = 5_000; // Launchpad X host tick: 1 ms.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Query,
    Layout,
    Begin,
    Keepalive,
    End,
    Set { layer: u8, side: u8, pitch: u8, color: u8 },
}

pub fn decode(data: &[u8]) -> Option<Command> {
    let start = 1 + MANUFACTURER_ID.len() + HEADER.len();
    if data.len() < start + 2 || data.first() != Some(&0xf0)
        || !data[1..].starts_with(MANUFACTURER_ID)
        || !data[1 + MANUFACTURER_ID.len()..].starts_with(&HEADER)
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

pub fn reply(payload: &[u8], out: &mut [u8]) -> Option<usize> {
    let start = 1 + MANUFACTURER_ID.len() + HEADER.len();
    let len = start + payload.len() + 1;
    if out.len() < len || payload.iter().any(|&b| b > 127) { return None; }
    out[0] = 0xf0;
    out[1..1 + MANUFACTURER_ID.len()].copy_from_slice(MANUFACTURER_ID);
    out[1 + MANUFACTURER_ID.len()..start].copy_from_slice(&HEADER);
    out[start..len - 1].copy_from_slice(payload);
    out[len - 1] = 0xf7;
    Some(len)
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
    pub fn pad_color(&self, index: u8, pitch: Option<u8>, held: bool) -> Option<u8> {
        let col = index % 10;
        if !self.active || !(1..=8).contains(&(index / 10)) || !(1..=8).contains(&col) {
            return None;
        }
        let pitch = pitch?;
        let side = usize::from(col > 4);
        let current = self.color(side, 1, pitch);
        if current != 0 { return Some(current); }
        let preview = self.color(side, 0, pitch);
        if !held && preview != 0 { Some(preview) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_envelope_and_command_validation() {
        let set = [0xf0, 0x7d, 67, 70, 87, 12, 1, 0x20, 1, 2, 127, 127, 0xf7];
        assert_eq!(decode(&set), Some(Command::Set { layer: 1, side: 2, pitch: 127, color: 127 }));
        for (at, value) in [(1, 0x7c), (2, 0), (5, 14), (6, 2), (8, 2), (9, 3), (10, 128)] {
            let mut bad = set;
            bad[at] = value;
            assert_eq!(decode(&bad), None);
        }
        assert_eq!(decode(&set[..set.len() - 1]), None);
        let mut trailing = set.to_vec();
        trailing.push(0);
        assert_eq!(decode(&trailing), None);
        for (command, expected) in [(1, Command::Query), (2, Command::Layout),
                                    (0x10, Command::Begin), (0x11, Command::Keepalive),
                                    (0x12, Command::End)] {
            assert_eq!(decode(&[&set[..7], &[command, 0xf7]].concat()), Some(expected));
        }
        let mut response = [0; 12];
        assert_eq!(reply(&[0x41, 1], &mut response), Some(10));
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
        assert_eq!(state.pad_color(11, Some(48), false), Some(13));
        assert_eq!(state.pad_color(11, Some(48), true), None);
        assert_eq!(state.pad_color(15, Some(48), false), Some(21));
        assert_eq!(state.pad_color(15, Some(48), true), Some(21));
        assert_eq!(state.pad_color(15, Some(49), false), None);
        assert_eq!(state.pad_color(91, Some(48), false), None);
        assert_eq!(state.pad_color(11, None, false), None);
    }
}

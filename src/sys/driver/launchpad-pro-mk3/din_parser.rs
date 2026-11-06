// SPDX-License-Identifier: GPL-3.0-only
//! MIDI byte-stream framing for UART4 DIN In. No USB or hardware dependency.

pub struct Parser {
    running: u8,
    status: u8,
    data: [u8; 2],
    count: usize,
    sysex: bool,
}

impl Parser {
    pub const fn new() -> Self {
        Self { running: 0, status: 0, data: [0; 2], count: 0, sysex: false }
    }

    pub fn reset(&mut self) { *self = Self::new(); }

    /// Emits complete channel/system-common messages and real-time bytes.
    /// SysEx is deliberately not forwarded until streaming framing is added.
    pub fn push(&mut self, byte: u8, mut emit: impl FnMut(&[u8])) {
        if byte >= 0xf8 {
            emit(&[byte]);
            return;
        }
        if byte & 0x80 != 0 {
            self.count = 0;
            if byte == 0xf0 {
                self.running = 0;
                self.status = 0;
                self.sysex = true;
                return;
            }
            self.sysex = false;
            if byte == 0xf7 {
                self.running = 0;
                self.status = 0;
                return;
            }
            self.running = if byte < 0xf0 { byte } else { 0 };
            self.status = match byte {
                0x80..=0xef | 0xf1..=0xf3 => byte,
                0xf6 => { emit(&[byte]); 0 },
                _ => 0,
            };
            return;
        }
        if self.sysex { return; }
        if self.status == 0 { self.status = self.running; }
        let expected = match self.status {
            0xc0..=0xdf | 0xf1 | 0xf3 => 1,
            0x80..=0xbf | 0xe0..=0xef | 0xf2 => 2,
            _ => return,
        };
        self.data[self.count] = byte;
        self.count += 1;
        if self.count == expected {
            let message = [self.status, self.data[0], self.data[1]];
            emit(&message[..expected + 1]);
            self.count = 0;
            self.status = self.running;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;

    fn parse(input: &[u8]) -> Vec<Vec<u8>> {
        let mut parser = Parser::new();
        let mut output = Vec::new();
        for &byte in input { parser.push(byte, |message| output.push(message.to_vec())); }
        output
    }

    #[test]
    fn sustain_on_off_and_running_status() {
        assert_eq!(parse(&[0xb0, 64, 127, 64, 0]), vec![vec![0xb0, 64, 127], vec![0xb0, 64, 0]]);
    }

    #[test]
    fn realtime_does_not_interrupt_messages_or_sysex() {
        assert_eq!(parse(&[0x90, 60, 0xf8, 100, 0xf0, 1, 0xf8, 2, 0xf7, 60, 0x80, 60, 0]),
            vec![vec![0xf8], vec![0x90, 60, 100], vec![0xf8], vec![0x80, 60, 0]]);
    }

    #[test]
    fn one_byte_messages_and_system_common_clear_running_status() {
        assert_eq!(parse(&[0xc1, 55, 56, 0xf2, 1, 2, 57, 0xf6, 0xd1, 30]),
            vec![vec![0xc1, 55], vec![0xc1, 56], vec![0xf2, 1, 2], vec![0xf6], vec![0xd1, 30]]);
    }

    #[test]
    fn reset_and_interrupted_messages_drop_partial_data() {
        let mut parser = Parser::new();
        let mut out = Vec::new();
        for byte in [0xb0, 64, 0x90, 60] { parser.push(byte, |m| out.push(m.to_vec())); }
        parser.reset();
        for byte in [100, 0xb0, 64, 0] { parser.push(byte, |m| out.push(m.to_vec())); }
        assert_eq!(out, vec![vec![0xb0, 64, 0]]);
    }
}

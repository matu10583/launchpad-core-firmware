// SPDX-License-Identifier: GPL-3.0-only
//! Opt-in, read-only DIN pin discovery. NOT a MIDI receiver or pin assignment.
//! Never change GPIO modes, UARTs, M0, USB or flash here.
use crate::app::guide;
use crate::driver;
use crate::sys::midi::MidiPort;

// STM32F745 GPIO IDR registers. Six ports fit in one USB SysEx TX queue.
const IDR: [usize; 6] = [
    0x4002_0010, 0x4002_0410, 0x4002_0810,
    0x4002_0c10, 0x4002_1010, 0x4002_1410,
];
const RCC_AHB1ENR: usize = 0x4002_3830;
const WINDOW_MS: u64 = 40;

fn capture() -> [[u8; 16]; 6] {
    // Enabling port clocks does not change pin mux, pull or output state.
    unsafe {
        let rcc = RCC_AHB1ENR as *mut u32;
        core::ptr::write_volatile(rcc, core::ptr::read_volatile(rcc) | 0x3f);
    }
    let read = |port: usize| unsafe { core::ptr::read_volatile(port as *const u32) as u16 };
    let mut previous = IDR.map(read);
    let mut edges = [[0u8; 16]; 6];
    let start = embassy_time::Instant::now();
    while start.elapsed().as_millis() < WINDOW_MS {
        for (port, &address) in IDR.iter().enumerate() {
            let now = read(address);
            let mut changed = now ^ previous[port];
            previous[port] = now;
            while changed != 0 {
                let pin = changed.trailing_zeros() as usize;
                edges[port][pin] = edges[port][pin].saturating_add(1).min(127);
                changed &= changed - 1;
            }
        }
    }
    edges
}

/// F0 <guide ID> C F W 0E 01 30 F7 -> response payload 70 01 28
/// followed by 6x16 saturating edge counts for GPIO A..F, pins 0..15.
pub fn execute(data: &[u8]) -> bool {
    let mut request = [0u8; 16];
    let Some(len) = guide::reply(&[0x30], guide::MODEL_PRO_MK3, &mut request) else { return false };
    if data != &request[..len] { return false; }
    let edges = capture();
    let mut payload = [0u8; 3 + 6 * 16];
    payload[..3].copy_from_slice(&[0x70, 1, WINDOW_MS as u8]);
    for (port, pins) in edges.into_iter().enumerate() {
        payload[3 + port * 16..3 + (port + 1) * 16].copy_from_slice(&pins);
    }
    let mut response = [0u8; 112];
    if let Some(len) = guide::reply(&payload, guide::MODEL_PRO_MK3, &mut response) {
        driver::send_midi(MidiPort::Midi, &response[..len]);
    }
    true
}

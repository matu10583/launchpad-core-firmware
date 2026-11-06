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
const RESPONSE_LEN: usize = 2 + 3 + 2 + 3 + 6 * 16 + 1;
const CONFIG_RESPONSE_LEN: usize = 2 + 3 + 2 + 3 + 16 * 4 + 1;
const _: () = assert!(
    RESPONSE_LEN.div_ceil(3) <= crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
);
const _: () = assert!(
    RESPONSE_LEN.div_ceil(3) < crate::sys::driver::common::usb::queues::MIDI_TX_QUEUE_SIZE
);
const _: () = assert!(
    CONFIG_RESPONSE_LEN.div_ceil(3) <= crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
);

/// Request 31 <port A=0..F=5> returns 71 <port> <clock enabled> followed by
/// 16 tuples of (MODER, AFR, PUPDR, IDR), each value kept within MIDI's 7 bits.
/// Reads only; never changes pin configuration or the M0 UART.
fn config_snapshot(data: &[u8]) -> bool {
    for port in 0..6u8 {
        let mut request = [0u8; 16];
        let Some(len) = guide::reply(&[0x31, port], guide::MODEL_PRO_MK3, &mut request) else { return false };
        if data != &request[..len] { continue; }

        let clock = unsafe { core::ptr::read_volatile(RCC_AHB1ENR as *const u32) & (1 << port) != 0 };
        let mut payload = [0u8; 3 + 16 * 4];
        payload[..3].copy_from_slice(&[0x71, port, u8::from(clock)]);
        if clock {
            let base = 0x4002_0000usize + usize::from(port) * 0x400;
            let read = |offset: usize| unsafe { core::ptr::read_volatile((base + offset) as *const u32) };
            let moder = read(0x00);
            let pupdr = read(0x0c);
            let idr = read(0x10);
            let afr = [read(0x20), read(0x24)];
            for pin in 0..16 {
                let index = 3 + pin * 4;
                payload[index] = ((moder >> (pin * 2)) & 3) as u8;
                payload[index + 1] = ((afr[pin / 8] >> ((pin % 8) * 4)) & 15) as u8;
                payload[index + 2] = ((pupdr >> (pin * 2)) & 3) as u8;
                payload[index + 3] = ((idr >> pin) & 1) as u8;
            }
        }
        let mut response = [0u8; CONFIG_RESPONSE_LEN];
        if let Some(len) = guide::reply(&payload, guide::MODEL_PRO_MK3, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
        return true;
    }
    false
}

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
    if config_snapshot(data) { return true; }
    let mut request = [0u8; 16];
    let Some(len) = guide::reply(&[0x30], guide::MODEL_PRO_MK3, &mut request) else { return false };
    if data != &request[..len] { return false; }
    let edges = capture();
    let mut payload = [0u8; 3 + 6 * 16];
    payload[..3].copy_from_slice(&[0x70, 1, WINDOW_MS as u8]);
    for (port, pins) in edges.into_iter().enumerate() {
        payload[3 + port * 16..3 + (port + 1) * 16].copy_from_slice(&pins);
    }
    let mut response = [0u8; RESPONSE_LEN];
    if let Some(len) = guide::reply(&payload, guide::MODEL_PRO_MK3, &mut response) {
        driver::send_midi(MidiPort::Midi, &response[..len]);
    }
    true
}

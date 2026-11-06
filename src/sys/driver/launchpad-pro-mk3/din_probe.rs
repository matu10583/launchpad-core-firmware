// SPDX-License-Identifier: GPL-3.0-only
//! Opt-in DIN diagnostics; 0x33 temporarily samples the stock MIDI0 RX path.
//! Never change UART5/M0, TX pins, USB or flash here.
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
const UART_RESPONSE_LEN: usize = 2 + 3 + 2 + 2 + 1 + 4 * 5 + 1;
const DIN_RX_CAPACITY: usize = 32;
const DIN_RESPONSE_LEN: usize = 2 + 3 + 2 + 5 + DIN_RX_CAPACITY * 2 + 1;
const _: () = assert!(
    RESPONSE_LEN.div_ceil(3) <= crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
);
const _: () = assert!(
    RESPONSE_LEN.div_ceil(3) < crate::sys::driver::common::usb::queues::MIDI_TX_QUEUE_SIZE
);
const _: () = assert!(
    CONFIG_RESPONSE_LEN.div_ceil(3) <= crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
);
const _: () = assert!(
    UART_RESPONSE_LEN.div_ceil(3) <= crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
);
const _: () = assert!(
    DIN_RESPONSE_LEN.div_ceil(3) <= crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
);
const _: () = assert!(
    DIN_RESPONSE_LEN.div_ceil(3) < crate::sys::driver::common::usb::queues::MIDI_TX_QUEUE_SIZE
);

// STM32F745 UART4_RX PC11 AF8, as identified in the stock MIDI0Rx GPIO
// configuration. 54 MHz APB1 / 31,250 = 1,728 (16x oversampling).
const UART4: usize = 0x4000_4c00;
const RCC_APB1ENR: usize = 0x4002_3840;
const UART4_EN: u32 = 1 << 19;
const GPIOC: usize = 0x4002_0800;
const PC11_SHIFT: u32 = 22;
const PC11_AF_SHIFT: u32 = 12;

fn din_rx_snapshot(data: &[u8]) -> bool {
    let mut request = [0u8; 16];
    let Some(len) = guide::reply(&[0x33], guide::MODEL_PRO_MK3, &mut request) else { return false };
    if data != &request[..len] { return false; }

    let read = |addr: usize| unsafe { core::ptr::read_volatile(addr as *const u32) };
    let write = |addr: usize, value| unsafe { core::ptr::write_volatile(addr as *mut u32, value) };
    let previous_clock = read(RCC_APB1ENR);
    // Do not interfere with any other firmware that may have enabled UART4.
    let mut payload = [0u8; 5 + DIN_RX_CAPACITY];
    payload[0] = 0x73;
    if previous_clock & UART4_EN != 0 {
        payload[1] = 1; // busy; all other fields zero
    } else {
        let ahb_clock = read(RCC_AHB1ENR);
        write(RCC_AHB1ENR, ahb_clock | (1 << 2));
        let moder = read(GPIOC);
        let pupdr = read(GPIOC + 0x0c);
        let afrh = read(GPIOC + 0x24);

        // Do not enable TX. Configure only PC11 for the duration of the
        // capture; RX-only UART4 never forwards received bytes to USB.
        write(GPIOC + 0x24, (afrh & !(0xf << PC11_AF_SHIFT)) | (8 << PC11_AF_SHIFT));
        write(GPIOC + 0x0c, (pupdr & !(3 << PC11_SHIFT)) | (1 << PC11_SHIFT));
        write(GPIOC, (moder & !(3 << PC11_SHIFT)) | (2 << PC11_SHIFT));
        write(RCC_APB1ENR, previous_clock | UART4_EN);
        let _ = read(RCC_APB1ENR);
        write(UART4, 0);
        write(UART4 + 0x04, 0); // CR2: no RX inversion
        write(UART4 + 0x08, 0); // CR3: no DMA
        write(UART4 + 0x0c, 1728); // BRR: 31,250 at APB1=54MHz
        write(UART4 + 0x18, 1 << 3); // RQR.RXFRQ: discard a stale byte
        write(UART4 + 0x20, 0x0f); // clear initial PE/FE/NF/ORE
        write(UART4, 1 | 1 << 2); // UE | RE; TE stays off

        let start = embassy_time::Instant::now();
        while start.elapsed().as_millis() < WINDOW_MS {
            let isr = read(UART4 + 0x1c);
            if isr & (1 << 1) != 0 { payload[3] = payload[3].saturating_add(1).min(127); }
            if isr & (1 << 3) != 0 { payload[4] = payload[4].saturating_add(1).min(127); }
            if isr & (1 << 5) != 0 {
                let byte = read(UART4 + 0x24) as u8;
                if payload[2] < DIN_RX_CAPACITY as u8 {
                    payload[5 + payload[2] as usize] = byte;
                    payload[2] += 1;
                } else {
                    // Count discarded bytes, including non-7-bit status bytes.
                    payload[1] = 2;
                }
            }
            if isr & 0x0f != 0 { write(UART4 + 0x20, isr & 0x0f); }
        }
        write(UART4, 0);
        write(RCC_APB1ENR, previous_clock);
        write(GPIOC, (read(GPIOC) & !(3 << PC11_SHIFT)) | (moder & (3 << PC11_SHIFT)));
        write(GPIOC + 0x0c, (read(GPIOC + 0x0c) & !(3 << PC11_SHIFT)) | (pupdr & (3 << PC11_SHIFT)));
        write(GPIOC + 0x24, (read(GPIOC + 0x24) & !(0xf << PC11_AF_SHIFT)) | (afrh & (0xf << PC11_AF_SHIFT)));
        write(RCC_AHB1ENR, ahb_clock);
    }

    // MIDI SysEx cannot contain raw status bytes. Split each sampled byte
    // into two 7-bit-safe nibbles; 32 bytes + header fits in one TX batch.
    let count = payload[2] as usize;
    let mut encoded = [0u8; 5 + DIN_RX_CAPACITY * 2];
    encoded[..5].copy_from_slice(&payload[..5]);
    for (index, &byte) in payload[5..5 + count].iter().enumerate() {
        encoded[5 + index * 2] = byte >> 4;
        encoded[6 + index * 2] = byte & 15;
    }
    let mut response = [0u8; 2 + 3 + 2 + 5 + DIN_RX_CAPACITY * 2 + 1];
    if let Some(len) = guide::reply(&encoded[..5 + count * 2], guide::MODEL_PRO_MK3, &mut response) {
        driver::send_midi(MidiPort::Midi, &response[..len]);
    }
    true
}

/// Explicitly set PA7's electrical level; stock startup=Low. This does not
/// label either level as OUT or THRU until that relation is verified.
fn thru_pin_level(data: &[u8]) -> bool {
    for level in 0..=1u8 {
        let mut request = [0u8; 16];
        let Some(len) = guide::reply(&[0x34, level], guide::MODEL_PRO_MK3, &mut request) else { return false };
        if data != &request[..len] { continue; }
        super::din_tx::set_thru_pin_high(level == 1);
        let mut response = [0u8; 16];
        if let Some(len) = guide::reply(&[0x74, level], guide::MODEL_PRO_MK3, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
        return true;
    }
    false
}

// USART1, UART7, UART8 are the AF candidates in the GPIO snapshot. UART5
// is the known M0 link, read as a reference. Never read RDR: it consumes data.
const UARTS: [(usize, usize, u32); 4] = [
    (0x4001_1000, 0x4002_3844, 1 << 4),  // USART1, APB2ENR
    (0x4000_7800, 0x4002_3840, 1 << 30), // UART7, APB1ENR
    (0x4000_7c00, 0x4002_3840, 1 << 31), // UART8, APB1ENR
    (0x4000_5000, 0x4002_3840, 1 << 20), // UART5 (M0), APB1ENR
];

/// Request 32 <0=USART1, 1=UART7, 2=UART8, 3=UART5/M0> returns
/// 72 <index> <clock> plus CR1/CR3/BRR/ISR in little-endian 7-bit groups.
fn uart_snapshot(data: &[u8]) -> bool {
    for (index, &(base, rcc, mask)) in UARTS.iter().enumerate() {
        let mut request = [0u8; 16];
        let Some(len) = guide::reply(&[0x32, index as u8], guide::MODEL_PRO_MK3, &mut request) else { return false };
        if data != &request[..len] { continue; }

        let clock = unsafe { core::ptr::read_volatile(rcc as *const u32) & mask != 0 };
        let mut payload = [0u8; 3 + 4 * 5];
        payload[..3].copy_from_slice(&[0x72, index as u8, u8::from(clock)]);
        if clock {
            for (register, offset) in [0x00, 0x08, 0x0c, 0x1c].into_iter().enumerate() {
                let mut value = unsafe { core::ptr::read_volatile((base + offset) as *const u32) };
                for byte in &mut payload[3 + register * 5..3 + (register + 1) * 5] {
                    *byte = (value & 0x7f) as u8;
                    value >>= 7;
                }
            }
        }
        let mut response = [0u8; UART_RESPONSE_LEN];
        if let Some(len) = guide::reply(&payload, guide::MODEL_PRO_MK3, &mut response) {
            driver::send_midi(MidiPort::Midi, &response[..len]);
        }
        return true;
    }
    false
}

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
    if thru_pin_level(data) { return true; }
    if din_rx_snapshot(data) { return true; }
    if config_snapshot(data) { return true; }
    if uart_snapshot(data) { return true; }
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

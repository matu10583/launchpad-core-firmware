// SPDX-License-Identifier: GPL-3.0-only
//! Pro Mk3 MIDI0/MIDI1 outputs. PA7 uses the stock firmware's initial Low
//! state; its OUT/THRU polarity is unverified. Switching is opt-in only.

use core::cell::RefCell;
use cortex_m::interrupt::{self, Mutex};

use crate::app::MidiEvent;
use crate::sys::driver::common::usb::queues::Ring;
use crate::sys::midi::MidiPort;
use super::din_tx_format::{message_len, valid_message};

const UART4: usize = 0x4000_4c00;
const UART7: usize = 0x4000_7800;
const GPIOA: usize = 0x4002_0000;
const GPIOE: usize = 0x4002_1000;
const RCC_AHB1ENR: usize = 0x4002_3830;
const RCC_APB1RSTR: usize = 0x4002_3820;
const RCC_APB1ENR: usize = 0x4002_3840;
const UART7_BIT: u32 = 1 << 30;
const TXE: u32 = 1 << 7;
const TXEIE: u32 = 1 << 7;

struct Outputs {
    out1: Ring<u8, 257>,
    out2: Ring<u8, 257>,
}

static TX: Mutex<RefCell<Outputs>> = Mutex::new(RefCell::new(Outputs {
    out1: Ring::new(0),
    out2: Ring::new(0),
}));

#[inline(always)]
fn read(addr: usize) -> u32 { unsafe { core::ptr::read_volatile(addr as *const u32) } }
#[inline(always)]
fn write(addr: usize, value: u32) { unsafe { core::ptr::write_volatile(addr as *mut u32, value) } }

pub fn init() {
    // UART4 RX is already configured by din_rx::init(). Never reset it.
    write(RCC_AHB1ENR, read(RCC_AHB1ENR) | 1 | (1 << 4)); // GPIOA/E
    write(RCC_APB1ENR, read(RCC_APB1ENR) | UART7_BIT);
    let _ = read(RCC_APB1ENR);
    write(RCC_APB1RSTR, read(RCC_APB1RSTR) | UART7_BIT);
    write(RCC_APB1RSTR, read(RCC_APB1RSTR) & !UART7_BIT);

    // PA0=UART4_TX AF8; PE8=UART7_TX AF8, both push-pull and pull-up.
    write(GPIOA + 0x20, (read(GPIOA + 0x20) & !0xf) | 8);
    write(GPIOA + 0x0c, (read(GPIOA + 0x0c) & !3) | 1);
    write(GPIOA + 0x04, read(GPIOA + 0x04) & !1);
    write(GPIOA + 0x08, read(GPIOA + 0x08) | 3); // very high speed
    write(GPIOA, (read(GPIOA) & !3) | 2);

    write(GPIOE + 0x24, (read(GPIOE + 0x24) & !0xf) | 8);
    write(GPIOE + 0x0c, (read(GPIOE + 0x0c) & !(3 << 16)) | (1 << 16));
    write(GPIOE + 0x04, read(GPIOE + 0x04) & !(1 << 8));
    write(GPIOE + 0x08, read(GPIOE + 0x08) | (3 << 16));
    write(GPIOE, (read(GPIOE) & !(3 << 16)) | (2 << 16));

    // Stock MIDI1ThruMode default is Low. Its electrical direction is not
    // established: do not offer an unverified HIGH=THRU/OUT setting.
    write(GPIOA + 0x18, 1 << (7 + 16)); // BSRR reset before enabling output
    write(GPIOA + 0x04, read(GPIOA + 0x04) & !(1 << 7));
    write(GPIOA + 0x0c, (read(GPIOA + 0x0c) & !(3 << 14)) | (1 << 14));
    write(GPIOA, (read(GPIOA) & !(3 << 14)) | (1 << 14));

    // Both stock UART configurations enable CR2.TXINV. RXINV stays clear.
    // din_rx::init() programmed UART4 TXINV while UE was still disabled.
    write(UART4, read(UART4) | (1 << 3)); // TE, keep UE/RE/RXNEIE
    write(UART7, 0);
    write(UART7 + 0x04, 1 << 17);
    write(UART7 + 0x08, 0);
    write(UART7 + 0x0c, 1728); // APB1=54MHz / 31,250bps; OVER8=0
    write(UART7 + 0x20, 0x0f);
    cortex_m::peripheral::NVIC::unpend(stm32_metapac::Interrupt::UART7);
    write(UART7, 1 | (1 << 3)); // UE | TE; TXEIE only when queued
    unsafe { cortex_m::peripheral::NVIC::unmask(stm32_metapac::Interrupt::UART7); }
}

/// One complete short MIDI message; SysEx and malformed messages are not
/// forwarded. The two rings must both have room before either is modified.
pub fn enqueue(data: &[u8]) -> Result<(), ()> {
    if !valid_message(data) { return Err(()); }
    interrupt::free(|cs| {
        let mut tx = TX.borrow(cs).borrow_mut();
        if tx.out1.free_space() < data.len() || tx.out2.free_space() < data.len() { return Err(()); }
        for &byte in data {
            let _ = tx.out1.push(byte);
            let _ = tx.out2.push(byte);
        }
        write(UART4, read(UART4) | TXEIE);
        write(UART7, read(UART7) | TXEIE);
        Ok(())
    })
}

pub fn enqueue_usb_event(event: MidiEvent) {
    if event.port != MidiPort::Midi { return; }
    let data = [event.status, event.data1, event.data2];
    let Some(len) = message_len(event.status) else { return };
    let _ = enqueue(&data[..len]);
}

/// Physical MIDI1ThruMode pin level, NOT a claim about which level is OUT.
/// Boot always selects the stock firmware's initial Low state.
pub fn set_thru_pin_high(high: bool) {
    write(GPIOA + 0x18, if high { 1 << 7 } else { 1 << (7 + 16) });
}

pub fn uart4_tx_interrupt(isr: u32) {
    if isr & TXE == 0 || read(UART4) & TXEIE == 0 { return; }
    interrupt::free(|cs| {
        let mut tx = TX.borrow(cs).borrow_mut();
        if let Some(byte) = tx.out1.pop() { write(UART4 + 0x28, u32::from(byte)); }
        else { write(UART4, read(UART4) & !TXEIE); }
    });
}

#[unsafe(export_name = "UART7")]
pub extern "C" fn uart7_handler() {
    if read(UART7 + 0x1c) & TXE == 0 || read(UART7) & TXEIE == 0 { return; }
    interrupt::free(|cs| {
        let mut tx = TX.borrow(cs).borrow_mut();
        if let Some(byte) = tx.out2.pop() { write(UART7 + 0x28, u32::from(byte)); }
        else { write(UART7, read(UART7) & !TXEIE); }
    });
}

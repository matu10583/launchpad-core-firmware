// SPDX-License-Identifier: GPL-3.0-only
//! Continuous Pro Mk3 MIDI0 DIN In (UART4 RX / PC11) to USB MIDI port.
//! UART5 is reserved for the M0; no DIN TX or Out/Thru pins are configured.

use core::cell::RefCell;
use cortex_m::interrupt::{self, Mutex};

use super::din_parser::Parser;
use crate::sys::driver::common::usb::queues::Ring;
use crate::sys::midi::MidiPort;

const UART4: usize = 0x4000_4c00;
const GPIOC: usize = 0x4002_0800;
const RCC_AHB1ENR: usize = 0x4002_3830;
const RCC_APB1RSTR: usize = 0x4002_3820;
const RCC_APB1ENR: usize = 0x4002_3840;
const UART4_BIT: u32 = 1 << 19;
const PC11_SHIFT: u32 = 22;
const PC11_AF_SHIFT: u32 = 12;
const RXNE: u32 = 1 << 5;
const UART_ERRORS: u32 = 0x0f; // PE, FE, NF, ORE

struct Receiver {
    bytes: Ring<u8, 257>, // 256 bytes; >40ms of full-rate DIN traffic
    lost: bool,
}

static RX: Mutex<RefCell<Receiver>> = Mutex::new(RefCell::new(Receiver {
    bytes: Ring::new(0),
    lost: false,
}));

#[inline(always)]
fn read(addr: usize) -> u32 { unsafe { core::ptr::read_volatile(addr as *const u32) } }
#[inline(always)]
fn write(addr: usize, value: u32) { unsafe { core::ptr::write_volatile(addr as *mut u32, value) } }

pub fn init() {
    write(RCC_AHB1ENR, read(RCC_AHB1ENR) | (1 << 2)); // GPIOC
    write(RCC_APB1ENR, read(RCC_APB1ENR) | UART4_BIT);
    let _ = read(RCC_APB1ENR);
    // Reset only UART4; bootloader state must not leak into the receiver.
    write(RCC_APB1RSTR, read(RCC_APB1RSTR) | UART4_BIT);
    write(RCC_APB1RSTR, read(RCC_APB1RSTR) & !UART4_BIT);

    write(GPIOC + 0x24, (read(GPIOC + 0x24) & !(0xf << PC11_AF_SHIFT)) | (8 << PC11_AF_SHIFT));
    write(GPIOC + 0x0c, (read(GPIOC + 0x0c) & !(3 << PC11_SHIFT)) | (1 << PC11_SHIFT));
    write(GPIOC, (read(GPIOC) & !(3 << PC11_SHIFT)) | (2 << PC11_SHIFT));

    write(UART4, 0);
    write(UART4 + 0x04, 0); // CR2: no RX inversion, 1 stop bit
    write(UART4 + 0x08, 0); // CR3: no DMA
    write(UART4 + 0x0c, 1728); // APB1=54MHz / 31,250bps; OVER8=0
    write(UART4 + 0x18, 1 << 3); // discard stale RX data
    write(UART4 + 0x20, UART_ERRORS);
    cortex_m::peripheral::NVIC::unpend(stm32_metapac::Interrupt::UART4);
    write(UART4, 1 | (1 << 2) | (1 << 5)); // UE | RE | RXNEIE; TE=0
    unsafe { cortex_m::peripheral::NVIC::unmask(stm32_metapac::Interrupt::UART4); }
}

#[unsafe(export_name = "UART4")]
pub extern "C" fn uart4_handler() {
    let isr = read(UART4 + 0x1c);
    let byte = if isr & RXNE != 0 { Some(read(UART4 + 0x24) as u8) } else { None };
    if isr & UART_ERRORS != 0 { write(UART4 + 0x20, isr & UART_ERRORS); }
    interrupt::free(|cs| {
        let mut receiver = RX.borrow(cs).borrow_mut();
        if isr & UART_ERRORS != 0 { receiver.lost = true; }
        else if let Some(byte) = byte {
            if !receiver.bytes.push(byte) { receiver.lost = true; }
        }
    });
}

pub fn poll(parser: &mut Parser) {
    // Keep each main-loop iteration bounded even under sustained MIDI traffic.
    for _ in 0..256 {
        let next = interrupt::free(|cs| {
            let mut receiver = RX.borrow(cs).borrow_mut();
            if receiver.lost {
                receiver.lost = false;
                while receiver.bytes.pop().is_some() {}
                Some(Err(()))
            } else {
                receiver.bytes.pop().map(Ok)
            }
        });
        match next {
            None => break,
            Some(Err(())) => parser.reset(),
            Some(Ok(byte)) => parser.push(byte, |message| {
                let _ = super::usb::enqueue_tx_message(MidiPort::Midi.as_cable(), message);
            }),
        }
    }
}

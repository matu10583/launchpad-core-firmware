// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister
// Copyright (C) 2026 ZephyrCodesStuff

use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use embassy_stm32::interrupt::{self, InterruptExt};
use stm32_metapac as pac;

use super::grid::Grid;

const TIM1_PSC_1MHZ: u16 = 84 - 1;
const INITIAL_ARR_US: u16 = 32;
const MIN_IRQ_INTERVAL_US: u64 = 24;

static GRID: AtomicPtr<Grid> = AtomicPtr::new(ptr::null_mut());
static DRIVE_PHASE: AtomicBool = AtomicBool::new(false);
static FRAME_COMPLETE: AtomicBool = AtomicBool::new(false);

pub fn start(grid: *mut Grid) {
    GRID.store(grid, Ordering::Release);
    DRIVE_PHASE.store(false, Ordering::Relaxed);
    FRAME_COMPLETE.store(false, Ordering::Relaxed);

    critical_section::with(|_cs| {
        pac::RCC.apb2enr().modify(|w| w.set_tim1en(true));
    });

    pac::TIM1.cr1().modify(|w| {
        w.set_cen(false);
        w.set_arpe(false);
    });
    pac::TIM1.psc().write(|w| *w = TIM1_PSC_1MHZ);
    pac::TIM1
        .arr()
        .write_value(pac::timer::regs::ArrCore(INITIAL_ARR_US as u32));
    pac::TIM1.cnt().write_value(pac::timer::regs::CntCore(0));
    pac::TIM1.egr().write(|w| w.set_ug(true));
    pac::TIM1.sr().write(|w| w.set_uif(false));
    pac::TIM1.dier().modify(|w| w.set_uie(true));

    interrupt::TIM1_UP_TIM10.unpend();
    unsafe {
        interrupt::TIM1_UP_TIM10.enable();
    }

    pac::TIM1.cr1().modify(|w| {
        w.set_arpe(false);
        w.set_cen(true);
    });
}

pub fn take_frame_complete() -> bool {
    FRAME_COMPLETE.swap(false, Ordering::AcqRel)
}

#[cortex_m_rt::interrupt]
fn TIM1_UP_TIM10() {
    if !pac::TIM1.sr().read().uif() {
        return;
    }
    pac::TIM1.sr().write(|w| w.set_uif(false));
    let _ = pac::TIM1.sr().read();

    let grid = GRID.load(Ordering::Relaxed);
    if grid.is_null() {
        return;
    }

    let grid = unsafe { &mut *grid };

    if !DRIVE_PHASE.load(Ordering::Relaxed) {
        let arr = timer_arr_from_us(grid.prepare_delay_us()) as u32;
        pac::TIM1.arr().write_value(pac::timer::regs::ArrCore(arr));
        pac::TIM1.cnt().write_value(pac::timer::regs::CntCore(0));
        grid.prepare_phase();
        DRIVE_PHASE.store(true, Ordering::Relaxed);
        return;
    }

    let arr = timer_arr_from_us(grid.drive_delay_us()) as u32;
    pac::TIM1.arr().write_value(pac::timer::regs::ArrCore(arr));
    pac::TIM1.cnt().write_value(pac::timer::regs::CntCore(0));

    grid.drive_phase();
    grid.advance_slot();

    if grid.frame_complete() {
        FRAME_COMPLETE.store(true, Ordering::Release);
    }

    DRIVE_PHASE.store(false, Ordering::Relaxed);
}

fn timer_arr_from_us(us: u64) -> u16 {
    us.clamp(MIN_IRQ_INTERVAL_US, u16::MAX as u64) as u16
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister
// Copyright (C) 2026 ZephyrCodesStuff

use stm32_metapac as pac;

// ----------------------------
// Hardware Definitions
// ----------------------------

/// Size of the physical 10x10 pad layout mapping table.
const LP_LED_COUNT: usize = 100;

/// Total 8x8 FSR pressure sensors
const LP_GRID_SENSOR_COUNT: usize = 64;

/// Total digital side/top buttons (16 buttons).
const LP_SIMPLE_BTN_COUNT: usize = 16;

/// Number of analog ADC DMA channels sampled per multiplexer bank.
const LP_ADC_CHANNELS: usize = 8;

/// Size of the lock-free SPSC event ring buffer queue for grid press/release events.
const LP_GRID_QUEUE_LEN: usize = 64;

// ----------------------------
// DSP, Calibration & Sensitivity Tuning Thresholds
// ----------------------------

/// Minimum pressure required to trigger a Note On.
///
/// Resting fingers or light touches below this value are ignored.
const LP_PRESS_START_NORM: u16 = 300;

/// Pressure threshold to trigger a Note Off.
///
/// Using a lower release threshold than start threshold creates hysteresis,
/// preventing rapid note flickering when a pad is lightly held.
const LP_PRESS_RELEASE_NORM: u16 = 8;

/// Debounce sample counter required before confirming a press.
const LP_PRESS_ON_COUNT: u8 = 1;

/// Debounce sample counter required before confirming a release.
const LP_RELEASE_COUNT: u8 = 8;

// ----------------------------
// Polyphonic Aftertouch Filtering
// ----------------------------

/// Pressure threshold before Polyphonic Aftertouch MIDI messages start sending.
const LP_AFTER_START_NORM: u16 = 601;

/// Minimum pressure change required before sending a new Aftertouch message (prevents spamming the USB MIDI bus with minor noise).
const LP_AFTER_DELTA_THR: u8 = 3;

/// Minimum clock ticks between consecutive Aftertouch events per pad.
const LP_AFTER_COOLDOWN: u8 = 2;

/// Ticks immediately following a press during which release events are suppressed.
const LP_RELEASE_HOLDOFF: u8 = 10;

// ----------------------------
// FSR (Force-Sensitive Resistors) Dynamic Recalibration
// ----------------------------

/// Force-Sensitive Resistors drift due to ambient temperature and mechanical relaxation.
///
/// If pad force is ≤ 96, the algorithm slowly recalibrates the zero-pressure baseline (grid_base).
///
/// When pressed harder (> 96), baseline tracking freezes so presses don't corrupt the zero point.
const LP_BASELINE_GUARD: u16 = 96;

// ----------------------------
// Non-Linear Velocity & Pressure Curves
// ----------------------------

// Coefficients used to map raw 12-bit ADC readings (0..4095) into 7-bit MIDI velocity (1..127)
// using a logarithmic/curved response matching human finger dynamics.
const LP_AFTER_FLOOR_16: u32 = 0x2222;
const LP_VELOCITY_GAIN_NUM: u32 = 140;

const PAD_SCAN_MAP: [u8; LP_LED_COUNT] = [
    0xff, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0xff, 0xff, 0x00, 0x08, 0x10, 0x18, 0x01,
    0x09, 0x11, 0x19, 0x88, 0xff, 0x20, 0x28, 0x30, 0x38, 0x21, 0x29, 0x31, 0x39, 0x89, 0xff, 0x02,
    0x0a, 0x12, 0x1a, 0x03, 0x0b, 0x13, 0x1b, 0x8a, 0xff, 0x22, 0x2a, 0x32, 0x3a, 0x23, 0x2b, 0x33,
    0x3b, 0x8b, 0xff, 0x04, 0x0c, 0x14, 0x1c, 0x05, 0x0d, 0x15, 0x1d, 0x8c, 0xff, 0x24, 0x2c, 0x34,
    0x3c, 0x25, 0x2d, 0x35, 0x3d, 0x8d, 0xff, 0x06, 0x0e, 0x16, 0x1e, 0x07, 0x0f, 0x17, 0x1f, 0x8e,
    0xff, 0x26, 0x2e, 0x36, 0x3e, 0x27, 0x2f, 0x37, 0x3f, 0x8f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff,
];

#[derive(Clone, Copy)]
pub enum GridEvent {
    Press { note: u8, value: u8 },
    Release { note: u8 },
    Aftertouch { note: u8, value: u8 },
}

pub struct Inputs {
    side_hist: [u16; 4],
    side_stable: u16,
    adc_dma: [u16; LP_ADC_CHANNELS],
    pressure_capture_bank: u8,
    pressure_capture_active: bool,
    pressure_raw: [[u16; LP_ADC_CHANNELS]; 8],
    pressure_dirty_mask: u8,
    grid_base: [u16; LP_GRID_SENSOR_COUNT],
    grid_filt: [u16; LP_GRID_SENSOR_COUNT],
    grid_hist: [[u16; 3]; LP_GRID_SENSOR_COUNT],
    grid_hist_pos: [u8; LP_GRID_SENSOR_COUNT],
    grid_hist_count: [u8; LP_GRID_SENSOR_COUNT],
    grid_ready: [bool; LP_GRID_SENSOR_COUNT],
    grid_pressed: [bool; LP_GRID_SENSOR_COUNT],
    grid_last_at: [u8; LP_GRID_SENSOR_COUNT],
    grid_on_count: [u8; LP_GRID_SENSOR_COUNT],
    grid_off_count: [u8; LP_GRID_SENSOR_COUNT],
    grid_release_holdoff: [u8; LP_GRID_SENSOR_COUNT],
    grid_after_cooldown: [u8; LP_GRID_SENSOR_COUNT],
    sensor_to_note: [u8; LP_GRID_SENSOR_COUNT],
    simple_to_note: [u8; LP_SIMPLE_BTN_COUNT],
    queue: [Option<GridEvent>; LP_GRID_QUEUE_LEN],
    q_head: u8,
    q_tail: u8,
}

impl Inputs {
    pub fn new() -> Self {
        let mut this = Self {
            side_hist: [0; 4],
            side_stable: 0,
            adc_dma: [0; LP_ADC_CHANNELS],
            pressure_capture_bank: 0xff,
            pressure_capture_active: false,
            pressure_raw: [[0; LP_ADC_CHANNELS]; 8],
            pressure_dirty_mask: 0,
            grid_base: [0; LP_GRID_SENSOR_COUNT],
            grid_filt: [0; LP_GRID_SENSOR_COUNT],
            grid_hist: [[0; 3]; LP_GRID_SENSOR_COUNT],
            grid_hist_pos: [0; LP_GRID_SENSOR_COUNT],
            grid_hist_count: [0; LP_GRID_SENSOR_COUNT],
            grid_ready: [false; LP_GRID_SENSOR_COUNT],
            grid_pressed: [false; LP_GRID_SENSOR_COUNT],
            grid_last_at: [0; LP_GRID_SENSOR_COUNT],
            grid_on_count: [0; LP_GRID_SENSOR_COUNT],
            grid_off_count: [0; LP_GRID_SENSOR_COUNT],
            grid_release_holdoff: [0; LP_GRID_SENSOR_COUNT],
            grid_after_cooldown: [0; LP_GRID_SENSOR_COUNT],
            sensor_to_note: [0xff; LP_GRID_SENSOR_COUNT],
            simple_to_note: [0xff; LP_SIMPLE_BTN_COUNT],
            queue: [None; LP_GRID_QUEUE_LEN],
            q_head: 0,
            q_tail: 0,
        };

        for (index, &map) in PAD_SCAN_MAP.iter().enumerate() {
            let note = idx_to_yx(index as u8);
            if (map as usize) < LP_GRID_SENSOR_COUNT {
                this.sensor_to_note[map as usize] = note;
            } else if (map & 0xf0) == 0x80 {
                this.simple_to_note[(map & 0x0f) as usize] = note;
            }
        }

        this
    }

    pub fn init_hardware(&mut self) {
        critical_section::with(|_cs| {
            pac::RCC.ahb1enr().modify(|w| {
                w.set_gpioaen(true);
                w.set_dma2en(true);
            });
            pac::RCC.apb2enr().modify(|w| w.set_adc1en(true));
        });

        for pin in 0..8 {
            pac::GPIOA
                .moder()
                .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::ANALOG));
            pac::GPIOA
                .pupdr()
                .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::FLOATING));
        }

        pac::DMA2.st(0).cr().modify(|w| w.set_en(false));
        while pac::DMA2.st(0).cr().read().en() {}

        clear_dma2_stream0_flags();

        pac::DMA2
            .st(0)
            .par()
            .write_value(pac::ADC1.dr().as_ptr() as u32);
        pac::DMA2
            .st(0)
            .m0ar()
            .write_value(self.adc_dma.as_mut_ptr() as u32);
        pac::DMA2
            .st(0)
            .ndtr()
            .write_value(pac::dma::regs::Ndtr(LP_ADC_CHANNELS as u32));
        pac::DMA2.st(0).fcr().write(|_| {});
        configure_dma2_stream0_cr();

        pac::ADC1_COMMON
            .ccr()
            .modify(|w| w.set_adcpre(pac::adccommon::vals::Adcpre::DIV4));
        pac::ADC1.cr1().write(|w| w.set_scan(true));
        pac::ADC1.cr2().write(|w| {
            w.set_dma(true);
            w.set_dds(pac::adc::vals::Dds::CONTINUOUS);
            w.set_eocs(pac::adc::vals::Eocs::EACH_CONVERSION);
        });
        pac::ADC1.smpr2().write(|w| {
            for ch in 0..8 {
                w.set_smp(ch, pac::adc::vals::SampleTime::CYCLES28);
            }
        });
        pac::ADC1.sqr1().write(|w| w.set_l(7));
        pac::ADC1.sqr2().write(|w| {
            w.set_sq(0, 6); // SQ7 = ch6
            w.set_sq(1, 7); // SQ8 = ch7
        });
        pac::ADC1.sqr3().write(|w| {
            w.set_sq(0, 0); // SQ1 = ch0
            w.set_sq(1, 1); // SQ2 = ch1
            w.set_sq(2, 2); // SQ3 = ch2
            w.set_sq(3, 3); // SQ4 = ch3
            w.set_sq(4, 4); // SQ5 = ch4
            w.set_sq(5, 5); // SQ6 = ch5
        });
        pac::ADC1.cr2().modify(|w| w.set_adon(true));
    }

    pub fn capture_side(&mut self, group: u8, row: u8, sample: u16) {
        if group >= 4 || row >= 4 {
            return;
        }

        let mut value = self.side_hist[group as usize];
        for col in 0..4 {
            let pressed = (sample & (1 << (col * 4))) != 0;
            let bit = col * 4 + row as usize;
            let mask = 1u16 << bit;
            if pressed {
                value |= mask;
            } else {
                value &= !mask;
            }
        }
        self.side_hist[group as usize] = value;
    }

    pub fn start_pressure_capture(&mut self, bank: u8) {
        if bank >= 8 {
            return;
        }

        if self.pressure_capture_active {
            pac::DMA2.st(0).cr().modify(|w| w.set_en(false));
            while pac::DMA2.st(0).cr().read().en() {}
        }

        clear_dma2_stream0_flags();
        pac::DMA2
            .st(0)
            .m0ar()
            .write_value(self.adc_dma.as_mut_ptr() as u32);
        pac::DMA2
            .st(0)
            .ndtr()
            .write_value(pac::dma::regs::Ndtr(LP_ADC_CHANNELS as u32));
        pac::ADC1.sr().write(|_| {});

        self.pressure_capture_bank = bank;
        self.pressure_capture_active = true;

        pac::DMA2.st(0).cr().modify(|w| w.set_en(true));
        pac::ADC1.cr2().modify(|w| w.set_swstart(true));
    }

    pub fn finish_pressure_capture(&mut self, bank: u8) -> bool {
        if bank >= 8 || !self.pressure_capture_active || self.pressure_capture_bank != bank {
            return true;
        }

        let lisr = pac::DMA2.isr(0).read();
        if !lisr.tcif(0) {
            if lisr.feif(0) || lisr.dmeif(0) || lisr.teif(0) {
                pac::DMA2.st(0).cr().modify(|w| w.set_en(false));
                while pac::DMA2.st(0).cr().read().en() {}
                clear_dma2_stream0_flags();
                self.pressure_capture_bank = 0xff;
                self.pressure_capture_active = false;
                return true;
            }
            return false;
        }

        clear_dma2_stream0_flags();

        for ch in 0..LP_ADC_CHANNELS {
            self.pressure_raw[bank as usize][ch] = self.adc_dma[ch] & 0x0fff;
        }

        self.pressure_capture_bank = 0xff;
        self.pressure_capture_active = false;
        self.pressure_dirty_mask |= 1 << bank;
        true
    }

    pub fn service(&mut self) {
        self.service_pressure_foreground();
        self.service_side_buttons_foreground();
    }

    pub fn pop_event(&mut self) -> Option<GridEvent> {
        if self.q_tail == self.q_head {
            return None;
        }

        let event = self.queue[self.q_tail as usize].take();
        self.q_tail = (self.q_tail + 1) % LP_GRID_QUEUE_LEN as u8;
        event
    }

    fn service_pressure_foreground(&mut self) {
        while self.pressure_dirty_mask != 0 {
            let bank = self.pressure_dirty_mask.trailing_zeros() as usize;
            self.pressure_dirty_mask &= !(1 << bank);

            let base_sensor = bank * LP_ADC_CHANNELS;
            for ch in 0..LP_ADC_CHANNELS {
                self.update_grid_sensor(base_sensor + ch, self.pressure_raw[bank][ch]);
            }
        }
    }

    fn service_side_buttons_foreground(&mut self) {
        let mut new_stable = 0u16;

        for bit in 0..LP_SIMPLE_BTN_COUNT {
            let mask = 1u16 << bit;
            let mut sum = 0;
            for sample in self.side_hist {
                if sample & mask != 0 {
                    sum += 1;
                }
            }

            if sum > 1 {
                new_stable |= mask;
            }
        }

        let changed = new_stable ^ self.side_stable;
        if changed == 0 {
            return;
        }

        for bit in 0..LP_SIMPLE_BTN_COUNT {
            let mask = 1u16 << bit;
            if changed & mask == 0 {
                continue;
            }

            let note = self.simple_to_note[bit];
            if note == 0xff {
                continue;
            }

            if new_stable & mask != 0 {
                self.queue_event(GridEvent::Press { note, value: 127 });
            } else {
                self.queue_event(GridEvent::Release { note });
            }
        }

        self.side_stable = new_stable;
    }

    fn update_grid_sensor(&mut self, sensor: usize, raw: u16) {
        if sensor >= LP_GRID_SENSOR_COUNT {
            return;
        }

        let pos = self.grid_hist_pos[sensor] as usize;
        self.grid_hist[sensor][pos] = raw;
        self.grid_hist_pos[sensor] = ((pos + 1) % 3) as u8;
        self.grid_hist_count[sensor] = self.grid_hist_count[sensor].saturating_add(1).min(3);

        let mut sample = raw;
        if self.grid_hist_count[sensor] >= 3 {
            sample = median3(
                self.grid_hist[sensor][0],
                self.grid_hist[sensor][1],
                self.grid_hist[sensor][2],
            );
        }

        let norm = normalize_raw_to_norm(sample);
        let mut filt = self.grid_filt[sensor];
        if filt == 0 {
            filt = norm;
        } else if norm >= filt {
            filt = ((filt as u32 + norm as u32 + 1) >> 1) as u16;
        } else {
            filt = (smlabb(filt as u32, 7, norm as u32 + 4) >> 3) as u16;
        }
        self.grid_filt[sensor] = filt;

        if !self.grid_ready[sensor] {
            self.grid_base[sensor] = filt;
            self.grid_ready[sensor] = true;
            return;
        }

        let mut base = self.grid_base[sensor];
        if filt > base {
            if filt - base < LP_BASELINE_GUARD && !self.grid_pressed[sensor] {
                base = (smlabb(base as u32, 31, filt as u32 + 16) >> 5) as u16;
            }
        } else if !self.grid_pressed[sensor] {
            base = (smlabb(base as u32, 31, filt as u32 + 16) >> 5) as u16;
        }
        self.grid_base[sensor] = base;

        let delta = filt.saturating_sub(base);
        let velocity = norm_to_velocity(norm);
        let pressure = norm_to_aftertouch(norm);

        if !self.grid_pressed[sensor] {
            if delta >= LP_PRESS_START_NORM {
                self.grid_on_count[sensor] = self.grid_on_count[sensor].saturating_add(1);
                if self.grid_on_count[sensor] >= LP_PRESS_ON_COUNT {
                    self.grid_pressed[sensor] = true;
                    self.grid_on_count[sensor] = 0;
                    self.grid_off_count[sensor] = 0;
                    self.grid_release_holdoff[sensor] = LP_RELEASE_HOLDOFF;
                    self.grid_after_cooldown[sensor] = 0;
                    self.grid_last_at[sensor] = pressure;
                    self.queue_grid_event(GridEventKind::Press, sensor, velocity);
                }
            } else {
                self.grid_on_count[sensor] = 0;
            }
            return;
        }

        self.grid_release_holdoff[sensor] = self.grid_release_holdoff[sensor].saturating_sub(1);

        if self.grid_after_cooldown[sensor] != 0 {
            self.grid_after_cooldown[sensor] -= 1;
        } else {
            let prev = self.grid_last_at[sensor];
            let diff = pressure.abs_diff(prev);
            if diff >= LP_AFTER_DELTA_THR {
                self.grid_last_at[sensor] = pressure;
                self.grid_after_cooldown[sensor] = LP_AFTER_COOLDOWN;
                self.queue_grid_event(GridEventKind::Aftertouch, sensor, pressure);
            }
        }

        if delta <= LP_PRESS_RELEASE_NORM {
            self.grid_off_count[sensor] = self.grid_off_count[sensor].saturating_add(1);
            if self.grid_off_count[sensor] >= LP_RELEASE_COUNT
                && self.grid_release_holdoff[sensor] == 0
            {
                self.grid_pressed[sensor] = false;
                self.grid_off_count[sensor] = 0;
                self.grid_release_holdoff[sensor] = 0;
                self.grid_after_cooldown[sensor] = 0;
                self.grid_last_at[sensor] = 0;
                self.grid_base[sensor] = filt;
                self.queue_grid_event(GridEventKind::Release, sensor, 0);
            }
            return;
        }

        self.grid_off_count[sensor] = 0;
    }

    fn queue_grid_event(&mut self, kind: GridEventKind, sensor: usize, value: u8) {
        if sensor >= LP_GRID_SENSOR_COUNT {
            return;
        }

        let note = self.sensor_to_note[sensor];
        if note == 0xff {
            return;
        }

        match kind {
            GridEventKind::Press => self.queue_event(GridEvent::Press { note, value }),
            GridEventKind::Release => self.queue_event(GridEvent::Release { note }),
            GridEventKind::Aftertouch => self.queue_event(GridEvent::Aftertouch { note, value }),
        }
    }

    fn queue_event(&mut self, event: GridEvent) {
        let next = (self.q_head + 1) % LP_GRID_QUEUE_LEN as u8;
        if next == self.q_tail {
            return;
        }

        self.queue[self.q_head as usize] = Some(event);
        self.q_head = next;
    }
}

enum GridEventKind {
    Press,
    Release,
    Aftertouch,
}

fn idx_to_yx(index: u8) -> u8 {
    (9 - (index / 10)) * 10 + (index % 10)
}

#[inline(always)]
fn normalize_raw_to_norm(raw: u16) -> u16 {
    if raw < 0x0097 {
        return 0;
    }

    let val = (((raw - 0x0096) as u32) << 12) / 0x0d16;
    usat12(val)
}

#[inline(always)]
fn usat12(val: u32) -> u16 {
    let res: u32;
    unsafe {
        core::arch::asm!(
            "usat {0}, #12, {1}",
            out(reg) res,
            in(reg) val,
            options(nomem, nostack, preserves_flags)
        );
    }
    res as u16
}

#[inline(always)]
fn usat7(val: u32) -> u8 {
    let res: u32;
    unsafe {
        core::arch::asm!(
            "usat {0}, #7, {1}",
            out(reg) res,
            in(reg) val,
            options(nomem, nostack, preserves_flags)
        );
    }
    res as u8
}

#[inline(always)]
fn norm_to_velocity(norm: u16) -> u8 {
    if norm <= LP_PRESS_START_NORM {
        return 1;
    }

    let value = (((norm - LP_PRESS_START_NORM) as u32) * LP_VELOCITY_GAIN_NUM) / 0x0ed4;
    usat7(value).max(1)
}

#[inline(always)]
fn norm_to_aftertouch(norm: u16) -> u8 {
    if norm <= LP_AFTER_START_NORM {
        return 0;
    }

    let scaled16 = if norm >= 0x0fff {
        0xffff
    } else {
        (((norm - 0x0258) as u32) << 16) / 0x0da7
    };

    let stretched16 = ((0x9000 * scaled16) >> 15).min(0xffff);
    let out16 = if stretched16 > LP_AFTER_FLOOR_16 {
        ((stretched16 - LP_AFTER_FLOOR_16) << 16) / (0x10000 - LP_AFTER_FLOOR_16)
    } else {
        0
    };

    usat7(out16 >> 9)
}
#[inline(always)]
fn smlabb(x: u32, y: u32, acc: u32) -> u32 {
    let res: u32;
    unsafe {
        core::arch::asm!(
            "smlabb {0}, {1}, {2}, {3}",
            out(reg) res,
            in(reg) x,
            in(reg) y,
            in(reg) acc,
            options(nomem, nostack, preserves_flags)
        );
    }
    res
}

#[inline(always)]
fn median3(a: u16, b: u16, c: u16) -> u16 {
    a.max(b.min(c)).min(b.max(c))
}

fn clear_dma2_stream0_flags() {
    pac::DMA2.ifcr(0).write(|w| {
        w.set_feif(0, true);
        w.set_dmeif(0, true);
        w.set_teif(0, true);
        w.set_htif(0, true);
        w.set_tcif(0, true);
    });
}

fn configure_dma2_stream0_cr() {
    pac::DMA2.st(0).cr().write(|w| {
        w.set_pl(pac::dma::vals::Pl::HIGH);
        w.set_chsel(0);
        w.set_msize(pac::dma::vals::Size::BITS16);
        w.set_psize(pac::dma::vals::Size::BITS16);
        w.set_minc(true);
        w.set_pinc(false);
        w.set_dir(pac::dma::vals::Dir::PERIPHERAL_TO_MEMORY);
    });
}

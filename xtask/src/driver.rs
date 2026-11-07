use crate::sys::midi::MidiPort;

std::thread_local! {
    static SENT: std::cell::RefCell<Vec<(MidiPort, Vec<u8>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn send_midi(port: MidiPort, data: &[u8]) {
    SENT.with(|sent| sent.borrow_mut().push((port, data.to_vec())));
}

pub fn take_midi() -> Vec<(MidiPort, Vec<u8>)> {
    SENT.with(|sent| std::mem::take(&mut *sent.borrow_mut()))
}
pub fn highspeed_leds_enabled() -> bool { false }
pub fn brightness() -> u8 { crate::sys::settings::with(|s| s.brightness) }
pub fn set_brightness(value: u8) { crate::sys::settings::update(|s| s.brightness = value); }
pub fn device_id() -> u8 { if cfg!(feature = "launchpad-x") { 0x0c } else { 0x0e } }
pub fn set_rgb_led(_index: u8, _r: u8, _g: u8, _b: u8) {} // M0 LED transport is not simulated.

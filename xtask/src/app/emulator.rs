//! Drive the firmware AppHost (including mode routing and Setup), not a UI copy.
//! Hardware USB queues, sensors, M0 and flash are deliberately outside this boundary.
use super::{host::AppHost, AftertouchEvent, AppId, MidiEvent, SurfaceEvent};
use crate::sys::{led::{self, Color, Update}, midi::MidiPort};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Device { X, ProMk3 }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode { Note, Custom, Setup, Daw, Performance, Programmer, PaletteEditor, Boot }

impl Mode {
    fn app(self) -> AppId {
        match self {
            Self::Note => AppId::Note,
            Self::Custom => {
                #[cfg(feature = "launchpad-x")]
                { AppId::XCustom }
                #[cfg(feature = "launchpad-pro-mk3")]
                { AppId::CustomNote }
            }
            Self::Setup => AppId::Setup,
            Self::Daw => AppId::Daw,
            Self::Performance => AppId::Performance,
            Self::Programmer => AppId::Programmer,
            Self::PaletteEditor => AppId::PaletteEditor,
            Self::Boot => AppId::Boot,
        }
    }
    fn from_app(app: AppId) -> Self {
        match app {
            AppId::Note => Self::Note,
            #[cfg(feature = "launchpad-x")]
            AppId::XCustom => Self::Custom,
            #[cfg(feature = "launchpad-pro-mk3")]
            AppId::CustomNote => Self::Custom,
            AppId::Setup => Self::Setup,
            AppId::Daw => Self::Daw,
            AppId::Performance => Self::Performance,
            AppId::Programmer => Self::Programmer,
            AppId::PaletteEditor => Self::PaletteEditor,
            AppId::Boot => Self::Boot,
        }
    }
}

pub enum Input {
    Surface { pad: u8, pressed: bool, velocity: u8 },
    Aftertouch { pad: u8, value: u8 },
    Midi(MidiEvent),
    Sysex { port: MidiPort, bytes: Vec<u8> },
    AdvanceMs(u16),
    Switch(Mode),
}

#[derive(Debug)]
pub struct Observation {
    pub midi: Vec<(MidiPort, Vec<u8>)>,
    pub leds: [Color; 64],
    pub surface: [Color; 100],
    pub updates: Vec<Update>,
}

impl Observation {
    pub fn pad(&self, index: u8) -> Color {
        let row = index / 10;
        let col = index % 10;
        assert!((1..=8).contains(&row) && (1..=8).contains(&col));
        self.leds[((row - 1) * 8 + col - 1) as usize]
    }
}

pub struct Emulator {
    host: AppHost,
}

impl Emulator {
    pub fn mode(&self) -> Mode { Mode::from_app(self.host.current) }

    pub fn new(device: Device, mode: Mode) -> Self {
        assert_eq!(device, if cfg!(feature = "launchpad-x") { Device::X } else { Device::ProMk3 },
            "build xtask with the matching launchpad-x or launchpad-pro-mk3 feature");
        crate::sys::settings::reset();
        crate::sys::rotation::set(crate::sys::rotation::Rotation::Default);
        led::reset();
        crate::driver::take_midi();
        let mut host = AppHost::new(mode.app());
        host.init();
        led::take_updates();
        Self { host }
    }

    pub fn step(&mut self, input: Input) -> Observation {
        match input {
            Input::Surface { pad, pressed, velocity } => {
                let event = SurfaceEvent { index: pad, pressed, value: velocity };
                self.host.route_surface_event(event);
            }
            Input::Midi(event) => self.host.route_midi_event(event),
            Input::Aftertouch { pad, value } => self.host.route_aftertouch_event(AftertouchEvent { index: pad, value }),
            Input::Sysex { port, bytes } => self.host.receive_sysex(port, &bytes),
            Input::AdvanceMs(ticks) => for _ in 0..ticks { self.host.route_tick_event(); },
            Input::Switch(mode) => self.host.switch(mode.app()), // explicit diagnostic shortcut
        }
        Observation {
            midi: crate::driver::take_midi(),
            leds: std::array::from_fn(|slot| {
                let pad = (slot / 8 + 1) as u8 * 10 + (slot % 8 + 1) as u8;
                led::color(pad)
            }),
            updates: led::take_updates(),
            surface: std::array::from_fn(|index| led::color(index as u8)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::guide;

    fn sysex(model: u8, payload: &[u8], port: MidiPort) -> Input {
        let mut frame = [0; 96];
        let len = guide::reply(payload, model, &mut frame).unwrap();
        Input::Sysex { port, bytes: frame[..len].to_vec() }
    }

    #[cfg(feature = "launchpad-x")]
    #[test]
    fn x_duplicate_notes_and_mode_exit_have_observable_led_and_midi_traces() {
        let mut emu = Emulator::new(Device::X, Mode::Custom);
        let on = emu.step(Input::Surface { pad: 15, pressed: true, velocity: 70 });
        assert_eq!(on.midi, [(MidiPort::Midi, vec![0x90, 72, 127])]);
        assert_eq!(on.pad(15), Color::Rgb(0xffffff));
        let duplicate = emu.step(Input::Surface { pad: 71, pressed: true, velocity: 70 });
        assert!(duplicate.midi.is_empty());
        let first_off = emu.step(Input::Surface { pad: 15, pressed: false, velocity: 0 });
        assert!(first_off.midi.is_empty());
        let final_off = emu.step(Input::Switch(Mode::Note));
        assert_eq!(final_off.midi, [(MidiPort::Midi, vec![0x90, 72, 0])]);
        assert!(final_off.updates.contains(&Update::Clear));
    }

    #[test]
    fn both_models_answer_layout_only_on_performance_midi_port() {
        #[cfg(feature = "launchpad-x")]
        let models = [(Device::X, guide::MODEL_X, 85)];
        #[cfg(feature = "launchpad-pro-mk3")]
        let models = [(Device::ProMk3, guide::MODEL_PRO_MK3, 89)];
        for (device, model, bytes) in models {
            for mode in [Mode::Note, Mode::Custom] {
                let mut emu = Emulator::new(device, mode);
                assert!(emu.step(sysex(model, &[2], MidiPort::Daw)).midi.is_empty());
                let response = emu.step(sysex(model, &[2], MidiPort::Midi));
                assert_eq!(response.midi.len(), 1);
                assert_eq!(response.midi[0].0, MidiPort::Midi);
                assert_eq!(response.midi[0].1.len(), bytes);
                assert_eq!(response.midi[0].1[7], 0x42);
                assert_eq!(response.midi[0].1[5], model);
                assert!(emu.step(sysex(if model == guide::MODEL_X { guide::MODEL_PRO_MK3 }
                    else { guide::MODEL_X }, &[2], MidiPort::Midi)).midi.is_empty());
            }
        }
    }

    #[cfg(feature = "launchpad-pro-mk3")]
    #[test]
    fn pro_custom_performance_midi_still_leaves_on_midi_port() {
        let mut emu = Emulator::new(Device::ProMk3, Mode::Custom);
        let on = emu.step(Input::Surface { pad: 11, pressed: true, velocity: 90 });
        let off = emu.step(Input::Surface { pad: 11, pressed: false, velocity: 0 });
        assert_eq!(on.midi, [(MidiPort::Midi, vec![0x90, 48, 127])]);
        assert_eq!(off.midi, [(MidiPort::Midi, vec![0x90, 48, 0])]);
    }

    #[test]
    fn virtual_clock_expires_guide_without_sleep_or_hardware() {
        #[cfg(feature = "launchpad-x")]
        let models = [(Device::X, guide::MODEL_X)];
        #[cfg(feature = "launchpad-pro-mk3")]
        let models = [(Device::ProMk3, guide::MODEL_PRO_MK3)];
        for (device, model) in models {
            let mut emu = Emulator::new(device, Mode::Custom);
            let begun = emu.step(sysex(model, &[0x10], MidiPort::Midi));
            assert_eq!(&begun.midi[0].1[7..], &[0x50, 0x10, 1, 0xf7]);
            let colored = emu.step(sysex(model, &[0x20, 0, 0, 48, 13], MidiPort::Midi));
            assert_eq!(colored.pad(11), Color::Palette(13));
            assert!(colored.midi.is_empty());
            assert_eq!(emu.step(Input::AdvanceMs(4_999)).pad(11), Color::Palette(13));
            let expired = emu.step(Input::AdvanceMs(1));
            assert_ne!(expired.pad(11), Color::Palette(13));
        }
    }

    fn press(emu: &mut Emulator, pad: u8) -> Observation {
        emu.step(Input::Surface { pad, pressed: true, velocity: 90 })
    }
    fn release(emu: &mut Emulator, pad: u8) -> Observation {
        emu.step(Input::Surface { pad, pressed: false, velocity: 0 })
    }

    #[cfg(feature = "launchpad-x")]
    #[test]
    fn x_physical_modes_setup_hold_and_selected_daw_port() {
        let mut emu = Emulator::new(Device::X, Mode::Note);
        press(&mut emu, 97);
        assert_eq!(emu.mode(), Mode::Custom);
        press(&mut emu, 15);
        let off = press(&mut emu, 96);
        assert_eq!(emu.mode(), Mode::Note);
        assert!(off.midi.iter().any(|(port, bytes)| *port == MidiPort::Midi && bytes.last() == Some(&0)));
        release(&mut emu, 96);
        press(&mut emu, 95);
        emu.step(Input::AdvanceMs(499));
        assert_eq!(emu.mode(), Mode::Note);
        emu.step(Input::AdvanceMs(1));
        assert_eq!(emu.mode(), Mode::Setup);
        release(&mut emu, 95);
        press(&mut emu, 12); // Setup selects DAW, but does not switch yet.
        assert_eq!(emu.mode(), Mode::Setup);
        press(&mut emu, 95);
        assert_eq!(emu.mode(), Mode::Daw);
        assert!(emu.step(sysex(guide::MODEL_X, &[2], MidiPort::Midi)).midi.is_empty());
    }

    #[cfg(feature = "launchpad-pro-mk3")]
    #[test]
    fn pro_physical_setup_selection_and_daw_port_isolation() {
        let mut emu = Emulator::new(Device::ProMk3, Mode::Note);
        press(&mut emu, 96);
        assert_eq!(emu.mode(), Mode::Custom);
        press(&mut emu, 0);
        assert_eq!(emu.mode(), Mode::Setup);
        assert!(emu.step(sysex(guide::MODEL_PRO_MK3, &[2], MidiPort::Midi)).midi.is_empty());
        press(&mut emu, 24); // Setup selects Note
        press(&mut emu, 0);
        assert_eq!(emu.mode(), Mode::Note);
        press(&mut emu, 93); // DAW hardware button
        assert_eq!(emu.mode(), Mode::Daw);
        let ignored = emu.step(Input::Midi(MidiEvent { port: MidiPort::Midi, status: 0x90, data1: 11, data2: 127 }));
        assert!(ignored.midi.is_empty());
    }

    #[test]
    fn setup_rotation_uses_firmware_raw_buttons_and_grid_mapping() {
        let device = if cfg!(feature = "launchpad-x") { Device::X } else { Device::ProMk3 };
        let mut emu = Emulator::new(device, Mode::Setup);
        press(&mut emu, 79); // LED/rotation page
        let left = if cfg!(feature = "launchpad-x") { 93 } else { 91 };
        press(&mut emu, left);
        assert_eq!(crate::sys::rotation::get() as u8, crate::sys::rotation::Rotation::Left as u8);
        assert_ne!(crate::sys::rotation::to_canonical_grid_only(11), 11);
        // Edge tab 89 remains physical even while the main grid is rotated.
        press(&mut emu, 89);
        assert_eq!(emu.mode(), Mode::Setup);
        let select = if cfg!(feature = "launchpad-x") { 14 } else { 24 };
        press(&mut emu, crate::sys::rotation::to_raw_grid_only(select));
        let exit = if cfg!(feature = "launchpad-x") { 95 } else { 0 };
        press(&mut emu, exit);
        assert_eq!(emu.mode(), Mode::Note);
    }
}

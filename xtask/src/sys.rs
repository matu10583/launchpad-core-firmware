pub mod midi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum MidiPort { Midi, Daw }
}

pub mod led {
    pub fn clear() {}
    pub fn set(_index: u8, _color: u32) {}
    pub fn set_raw(_index: u8, _color: u32) {}
    pub fn novation(_index: u8, _color: u8) {}
}

pub mod settings {
    use std::cell::RefCell;

    pub const VELOCITY_FINE_STEPS: u8 = 56;
    pub struct Settings {
        pub velocity_enabled: u8,
        pub velocity_curve: u8,
        pub velocity_gain: u8,
        pub velocity_soft: u8,
        pub aftertouch_mode: u8,
    }

    std::thread_local! {
        static SETTINGS: RefCell<Settings> = const { RefCell::new(Settings {
            velocity_enabled: 0, velocity_curve: 1, velocity_gain: 17,
            velocity_soft: 28, aftertouch_mode: 0,
        }) };
    }

    pub fn with<T>(f: impl FnOnce(&Settings) -> T) -> T {
        SETTINGS.with(|s| f(&s.borrow()))
    }

    pub fn update(f: impl FnOnce(&mut Settings)) {
        SETTINGS.with(|s| f(&mut s.borrow_mut()));
    }
}

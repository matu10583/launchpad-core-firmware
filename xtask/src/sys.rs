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
    pub struct Settings {
        pub velocity_enabled: u8,
        pub velocity_curve: u8,
        pub aftertouch_mode: u8,
    }

    pub fn with<T>(f: impl FnOnce(&Settings) -> T) -> T {
        f(&Settings { velocity_enabled: 0, velocity_curve: 0, aftertouch_mode: 0 })
    }
}

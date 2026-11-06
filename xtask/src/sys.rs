pub mod midi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum MidiPort { Midi, Daw }
}

pub mod led {
    #[cfg(test)]
    #[derive(Debug, PartialEq, Eq)]
    pub enum Update { Clear, Set(u8, u32), Raw(u8, u32), Novation(u8, u8) }

    #[cfg(test)]
    std::thread_local! {
        static UPDATES: std::cell::RefCell<Vec<Update>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[cfg(test)]
    pub fn take_updates() -> Vec<Update> {
        UPDATES.with(|updates| std::mem::take(&mut *updates.borrow_mut()))
    }

    pub fn clear() {
        #[cfg(test)]
        UPDATES.with(|updates| updates.borrow_mut().push(Update::Clear));
    }
    pub fn set(index: u8, color: u32) {
        #[cfg(test)]
        UPDATES.with(|updates| updates.borrow_mut().push(Update::Set(index, color)));
        #[cfg(not(test))]
        let _ = (index, color);
    }
    pub fn set_raw(index: u8, color: u32) {
        #[cfg(test)]
        UPDATES.with(|updates| updates.borrow_mut().push(Update::Raw(index, color)));
        #[cfg(not(test))]
        let _ = (index, color);
    }
    pub fn pulse_raw(_index: u8, _color: u32) {}
    pub fn novation(index: u8, color: u8) {
        #[cfg(test)]
        UPDATES.with(|updates| updates.borrow_mut().push(Update::Novation(index, color)));
        #[cfg(not(test))]
        let _ = (index, color);
    }
}

pub mod settings {
    use std::cell::RefCell;

    pub const VELOCITY_FINE_STEPS: u8 = 56;
    pub struct Settings {
        pub velocity_enabled: u8,
        pub velocity_curve: u8,
        pub velocity_gain: u8,
        pub velocity_soft: u8,
        pub x_note_scale_mode: u8,
        pub x_note_scale: u8,
        pub x_note_root: u8,
        pub x_note_overlap: u8,
        pub x_note_channel: u8,
        pub aftertouch_mode: u8,
    }

    std::thread_local! {
        static SETTINGS: RefCell<Settings> = const { RefCell::new(Settings {
            velocity_enabled: 0, velocity_curve: 1, velocity_gain: 17,
            velocity_soft: 28, aftertouch_mode: 0,
            x_note_scale_mode: 0, x_note_scale: 0, x_note_root: 0,
            x_note_overlap: 5, x_note_channel: 0,
        }) };
    }

    pub fn with<T>(f: impl FnOnce(&Settings) -> T) -> T {
        SETTINGS.with(|s| f(&s.borrow()))
    }

    pub fn update(f: impl FnOnce(&mut Settings)) {
        SETTINGS.with(|s| f(&mut s.borrow_mut()));
    }

    pub fn save() {}
}

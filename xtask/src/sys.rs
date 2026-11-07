pub mod midi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum MidiPort { Midi, Daw, Din }
}

pub mod led {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Color { Rgb(u32), Palette(u8) }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Update { Clear, Set(u8, u32), Raw(u8, u32), Novation(u8, u8) }

    std::thread_local! {
        static UPDATES: std::cell::RefCell<Vec<Update>> = const { std::cell::RefCell::new(Vec::new()) };
        static STATE: std::cell::RefCell<[Color; 100]> = const { std::cell::RefCell::new([Color::Rgb(0); 100]) };
    }

    pub fn color(index: u8) -> Color { STATE.with(|state| state.borrow()[index as usize]) }

    pub fn reset() {
        STATE.with(|state| state.borrow_mut().fill(Color::Rgb(0)));
        take_updates();
    }

    pub fn take_updates() -> Vec<Update> {
        UPDATES.with(|updates| std::mem::take(&mut *updates.borrow_mut()))
    }

    pub fn clear() {
            STATE.with(|state| state.borrow_mut().fill(Color::Rgb(0)));
            UPDATES.with(|updates| updates.borrow_mut().push(Update::Clear));
    }
    pub fn tick() {}
    pub fn tempo_start() {}
    pub fn tempo_stop() {}
    pub fn tempo_midi_clock() {}
    pub fn set_palette(index: u8, color: u8) { novation(index, color); }
    pub fn set_palette_raw(index: u8, color: u8) { if index < 100 { novation(index, color); } }
    pub fn set_rgb(index: u8, r: u8, g: u8, b: u8) { set(index, (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)); }
    pub fn novation_raw(index: u8, color: u8) { novation(index, color); }
    pub fn pulse(index: u8, color: u32) { set(index, color); }
    pub fn novation_rgb(color: u8) -> [u8; 3] { let (r,g,b) = crate::utils::palette::PALETTE_NOVATION.rgb(color); [r,g,b] }
    pub fn daw_raw(index: u8, color: [u8; 3], _flash: Option<[u8; 3]>, _pulse: bool) { set_raw(index, (u32::from(color[0]) << 16) | (u32::from(color[1]) << 8) | u32::from(color[2])); }
    pub fn set(index: u8, color: u32) {
            STATE.with(|state| state.borrow_mut()[index as usize] = Color::Rgb(color));
            UPDATES.with(|updates| updates.borrow_mut().push(Update::Set(index, color)));
    }
    pub fn set_raw(index: u8, color: u32) {
            STATE.with(|state| state.borrow_mut()[index as usize] = Color::Rgb(color));
            UPDATES.with(|updates| updates.borrow_mut().push(Update::Raw(index, color)));
    }
    pub fn pulse_raw(_index: u8, _color: u32) {}
    pub fn novation(index: u8, color: u8) {
            STATE.with(|state| state.borrow_mut()[index as usize] = Color::Palette(color));
            UPDATES.with(|updates| updates.borrow_mut().push(Update::Novation(index, color)));
    }
}

pub mod settings {
    use std::cell::RefCell;

    #[derive(Clone, Copy)]
    pub struct Settings {
        pub brightness: u8,
        pub mirror_enabled: u8,
        pub aftertouch_curve: u8,
        pub palette: u8,
        pub custom_palette: [[[u8; 128]; 3]; 3],
        pub velocity_enabled: u8,
        pub velocity_floor: u8,
        pub velocity_point_a: u8,
        pub velocity_point_b: u8,
        pub velocity_point_c: u8,
        pub velocity_point_d: u8,
        pub x_note_scale_mode: u8,
        pub x_note_scale: u8,
        pub x_note_root: u8,
        pub x_note_overlap: u8,
        pub x_note_channel: u8,
        pub aftertouch_mode: u8,
    }

    const DEFAULT: Settings = Settings {
            brightness: 7, mirror_enabled: 0, aftertouch_curve: 0, palette: 0,
            custom_palette: [[[0; 128]; 3]; 3],
            velocity_enabled: 0, velocity_floor: if cfg!(feature = "launchpad-x") { 5 } else { 0 }, velocity_point_a: 63,
            velocity_point_b: 63, aftertouch_mode: 0,
            velocity_point_c: 63, velocity_point_d: 63,
            x_note_scale_mode: 0, x_note_scale: 0, x_note_root: 0,
            x_note_overlap: 5, x_note_channel: 0,
        };
    std::thread_local! {
        static SETTINGS: RefCell<Settings> = const { RefCell::new(DEFAULT) };
    }
    pub fn reset() { SETTINGS.with(|s| *s.borrow_mut() = DEFAULT); }

    pub fn with<T>(f: impl FnOnce(&Settings) -> T) -> T {
        SETTINGS.with(|s| f(&s.borrow()))
    }

    pub fn update(f: impl FnOnce(&mut Settings)) {
        SETTINGS.with(|s| f(&mut s.borrow_mut()));
    }

    pub fn save() {}
    pub fn get() -> Settings { with(Clone::clone) }
}

#[path = "../../src/sys/rotation.rs"]
pub mod rotation;

#[path = "../../src/sys/sysex/mod.rs"]
pub mod sysex;

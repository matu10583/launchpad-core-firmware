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

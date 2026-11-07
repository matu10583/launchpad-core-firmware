//! Loopback-only front end for the real firmware AppHost; not hardware USB.
#[path = "../../../src/app/guide.rs"]
pub mod note_guide;
#[path = "../app/mod.rs"]
#[allow(dead_code, unused_imports)]
mod app;
#[path = "../sys.rs"]
#[allow(dead_code)]
mod sys;
#[path = "../driver.rs"]
mod driver;
#[path = "../utils.rs"]
mod utils;

use app::emulator::{Device, Emulator, Input, Mode, Observation};
use app::MidiEvent;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use sys::{led::{Color, Update}, midi::MidiPort};

const PAGE: &str = include_str!("../../emulator-ui/index.html");

fn parse_port(s: &str) -> Result<MidiPort, &'static str> {
    match s { "midi" => Ok(MidiPort::Midi), "daw" => Ok(MidiPort::Daw), _ => Err("port must be midi or daw") }
}

fn hex(s: &str) -> Result<Vec<u8>, &'static str> {
    let s: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if s.len() % 2 != 0 || s.len() > 1200 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid hex (maximum 600 bytes)");
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i+2], 16).map_err(|_| "invalid hex")).collect()
}

fn number(s: Option<&str>, max: u16) -> Result<u16, &'static str> {
    let n = s.ok_or("missing number")?.parse::<u16>().map_err(|_| "invalid number")?;
    if n > max { Err("number out of range") } else { Ok(n) }
}

fn command(text: &str, model: u8) -> Result<Input, &'static str> {
    let (op, rest) = text.trim().split_once(' ').unwrap_or((text.trim(), ""));
    let mut words = rest.split_whitespace();
    match op {
        "press" | "release" => {
            let pad = number(words.next(), 99)? as u8;
            let velocity = if op == "press" { number(words.next(), 127)? as u8 } else { 0 };
            if words.next().is_some() { return Err("extra arguments") }
            Ok(Input::Surface { pad, pressed: op == "press", velocity })
        }
        "aftertouch" => {
            let pad = number(words.next(), 99)? as u8;
            let value = number(words.next(), 127)? as u8;
            if words.next().is_some() { return Err("extra arguments") }
            Ok(Input::Aftertouch { pad, value })
        }
        "advance" => {
            let ticks = number(words.next(), 10_000)?;
            if words.next().is_some() { return Err("extra arguments") }
            Ok(Input::AdvanceMs(ticks))
        }
        "switch" => match rest.trim() {
            "note" => Ok(Input::Switch(Mode::Note)),
            "custom" => Ok(Input::Switch(Mode::Custom)),
            "setup" => Ok(Input::Switch(Mode::Setup)),
            "daw" => Ok(Input::Switch(Mode::Daw)),
            "performance" => Ok(Input::Switch(Mode::Performance)),
            "programmer" => Ok(Input::Switch(Mode::Programmer)),
            "boot" => Ok(Input::Switch(Mode::Boot)),
            _ => Err("unsupported mode"),
        },
        "sysex" | "guide" => {
            let port = parse_port(words.next().ok_or("missing port")?)?;
            let payload = hex(&words.collect::<Vec<_>>().join(""))?;
            let bytes = if op == "guide" {
                let mut frame = [0; 610];
                let len = note_guide::reply(&payload, model, &mut frame).ok_or("invalid guide payload")?;
                frame[..len].to_vec()
            } else { payload };
            if bytes.first() != Some(&0xf0) || bytes.last() != Some(&0xf7) { return Err("SysEx must start F0 and end F7") }
            Ok(Input::Sysex { port, bytes })
        }
        "midi" => {
            let port = parse_port(words.next().ok_or("missing port")?)?;
            let values = hex(&words.collect::<Vec<_>>().join(""))?;
            if values.len() != 3 { return Err("MIDI event needs 3 bytes") }
            Ok(Input::Midi(MidiEvent { port, status: values[0], data1: values[1], data2: values[2] }))
        }
        _ => Err("unknown command"),
    }
}

fn json(observation: Observation, mode: Mode, model: u8) -> String {
    let device = if model == note_guide::MODEL_X { "x" } else { "pro-mk3" };
    let mut out = format!("{{\"device\":\"{device}\",\"mode\":\"{}\",\"surface\":[", match mode {
        Mode::Note => "note", Mode::Custom => "custom", Mode::Setup => "setup",
        Mode::Daw => "daw", Mode::Performance => "performance", Mode::Programmer => "programmer",
        Mode::PaletteEditor => "palette-editor", Mode::Boot => "boot",
    });
    for (i, color) in observation.surface.iter().enumerate() {
        if i > 0 { out.push(','); }
        match color {
            Color::Rgb(value) => { write!(out, "[\"rgb\",{value}]").unwrap(); }
            Color::Palette(value) => { write!(out, "[\"palette\",{value}]").unwrap(); }
        }
    }
    out.push_str("],\"midi\":[");
    for (i, (port, bytes)) in observation.midi.iter().enumerate() {
        if i > 0 { out.push(','); }
        write!(out, "{{\"port\":\"{}\",\"hex\":\"", match port { MidiPort::Midi => "midi", MidiPort::Daw => "daw", MidiPort::Din => "din" }).unwrap();
        for byte in bytes { write!(out, "{byte:02X}").unwrap(); }
        out.push_str("\"}");
    }
    let clears = observation.updates.iter().filter(|u| matches!(u, Update::Clear)).count();
    write!(out, "],\"updates\":{},\"clears\":{clears}}}", observation.updates.len()).unwrap();
    out
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'\r\nConnection: close\r\n\r\n{body}", body.len())
}

fn handle(stream: &mut TcpStream, emu: &mut Emulator, model: u8) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    let mut request = [0u8; 4096];
    let mut used = 0;
    let end = loop {
        if used == request.len() { return respond(stream, "413 Payload Too Large", "text/plain", "request too large"); }
        let n = stream.read(&mut request[used..])?;
        if n == 0 { return Ok(()); }
        used += n;
        if let Some(at) = request[..used].windows(4).position(|w| w == b"\r\n\r\n") { break at + 4; }
    };
    let headers = match std::str::from_utf8(&request[..end]) { Ok(h) => h, Err(_) => return respond(stream, "400 Bad Request", "text/plain", "invalid headers") };
    let mut lines = headers.split("\r\n");
    let path = lines.next().unwrap_or("");
    let mut length = None;
    let mut host_ok = false;
    let mut origin_ok = true;
    let mut host = "";
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("host") {
                host_ok = matches!(value, "127.0.0.1:7878" | "localhost:7878");
                host = value;
            }
            if name.eq_ignore_ascii_case("origin") { origin_ok = value == format!("http://{host}"); }
            if name.eq_ignore_ascii_case("content-length") { length = value.parse::<usize>().ok(); }
        }
    }
    if !host_ok { return respond(stream, "403 Forbidden", "text/plain", "loopback host required"); }
    if path == "GET / HTTP/1.1" { return respond(stream, "200 OK", "text/html; charset=utf-8", PAGE); }
    if path == "GET /api/state HTTP/1.1" {
        let snapshot = emu.step(Input::AdvanceMs(0));
        return respond(stream, "200 OK", "application/json", &json(snapshot, emu.mode(), model));
    }
    if path != "POST /api/step HTTP/1.1" { return respond(stream, "404 Not Found", "text/plain", "not found"); }
    if !origin_ok { return respond(stream, "403 Forbidden", "text/plain", "invalid origin"); }
    let Some(size) = length else { return respond(stream, "411 Length Required", "text/plain", "content length required") };
    if size > 2048 || end + size > request.len() { return respond(stream, "413 Payload Too Large", "text/plain", "request too large"); }
    while used < end + size {
        let n = stream.read(&mut request[used..end + size])?;
        if n == 0 { return Ok(()); }
        used += n;
    }
    let Ok(text) = std::str::from_utf8(&request[end..end + size]) else { return respond(stream, "400 Bad Request", "text/plain", "invalid text") };
    match command(text, model) {
        Ok(input) => { let snapshot = emu.step(input); respond(stream, "200 OK", "application/json", &json(snapshot, emu.mode(), model)) }
        Err(message) => respond(stream, "400 Bad Request", "text/plain", message),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let device = match (args.next().as_deref(), args.next()) {
        (Some("x"), None) => Device::X,
        (Some("pro-mk3"), None) => Device::ProMk3,
        _ => return Err("usage: cargo run -p xtask --bin emulator-web -- x|pro-mk3".into()),
    };
    if device != if cfg!(feature = "launchpad-x") { Device::X } else { Device::ProMk3 } {
        return Err("wrong build feature: use --no-default-features --features launchpad-pro-mk3 for pro-mk3".into());
    }
    let model = if device == Device::X { note_guide::MODEL_X } else { note_guide::MODEL_PRO_MK3 };
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    let mut emu = Emulator::new(device, Mode::Note);
    eprintln!("Emulator ({device:?}): http://127.0.0.1:7878/ (local only; not hardware USB)");
    for connection in listener.incoming() {
        let mut stream = connection?;
        if let Err(error) = handle(&mut stream, &mut emu, model) { eprintln!("browser connection: {error}"); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guide_input_and_validation() {
        let Input::Sysex { port, bytes } = command("guide midi 21 00 0F 0D", note_guide::MODEL_X).unwrap() else { panic!("not sysex") };
        assert_eq!(port, MidiPort::Midi);
        assert_eq!(note_guide::decode(&bytes, note_guide::MODEL_X), Some(note_guide::Command::SetPad { layer: 0, pad: 15, color: 13 }));
        assert!(command("press 100 70", note_guide::MODEL_X).is_err());
        assert!(command("sysex midi F0 80 F7", note_guide::MODEL_X).is_ok());
        assert!(command("advance 10001", note_guide::MODEL_X).is_err());
        assert!(matches!(command("aftertouch 15 70", note_guide::MODEL_X), Ok(Input::Aftertouch { pad: 15, value: 70 })));
    }

    #[test]
    fn browser_http_uses_real_app_and_rejects_other_origins() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let device = if cfg!(feature = "launchpad-x") { Device::X } else { Device::ProMk3 };
        let model = if cfg!(feature = "launchpad-x") { note_guide::MODEL_X } else { note_guide::MODEL_PRO_MK3 };
        let server = std::thread::spawn(move || {
            let mut emu = Emulator::new(device, Mode::Custom);
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                handle(&mut stream, &mut emu, model).unwrap();
            }
        });
        let send = |request: &str| {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        };
        let body = "guide midi 10";
        let response = send(&format!("POST /api/step HTTP/1.1\r\nHost: localhost:7878\r\nOrigin: http://localhost:7878\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        assert!(response.contains("200 OK"));
        assert!(response.contains(if device == Device::X { "\"device\":\"x\"" } else { "\"device\":\"pro-mk3\"" }));
        assert!(response.contains("501001F7"));
        let body = "guide midi 21 00 0F 0D";
        let response = send(&format!("POST /api/step HTTP/1.1\r\nHost: localhost:7878\r\nOrigin: http://localhost:7878\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        assert!(response.contains("[\"palette\",13]"));
        let response = send("POST /api/step HTTP/1.1\r\nHost: localhost:7878\r\nOrigin: https://example.invalid\r\nContent-Length: 11\r\n\r\nadvance 500");
        assert!(response.contains("403 Forbidden"));
        server.join().unwrap();
    }
}

//! Packet encoding/decoding for Attack Shark mice (see PROTOCOL.md).

use serde::{Deserialize, Serialize};

use crate::tables::{BUTTON_CODES, DEFAULT_BUTTON_CODES, DPI_CODES};

pub const VENDOR_ID: u16 = 0x1d57;
pub const PRODUCT_IDS: [u16; 2] = [0xfa60, 0xfa65];

pub const REPORT_STATUS: u8 = 0x03;
pub const REPORT_DPI: u8 = 0x04;
pub const REPORT_LIGHT: u8 = 0x05;
pub const REPORT_POLLING: u8 = 0x06;
pub const REPORT_BUTTONS: u8 = 0x08;
pub const REPORT_MACRO: u8 = 0x09;
pub const REPORT_BULK: u8 = 0x0a;
pub const REPORT_VERSION: u8 = 0x0b;
pub const REPORT_QUERY: u8 = 0xa0;

pub const STAGES: usize = 8;
pub const SLOTS: usize = 18;
pub const MACRO_MAX_EVENTS: usize = 48;

/// Per-model limits, taken from the vendor app's page XML.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub id: u8,
    pub name: &'static str,
    /// Vendor app page (res/MS/MS_n).
    pub page: u8,
    pub stages: usize,
    pub dpi_slider_max: u16,
    /// Remappable button slots (0-based) with their labels.
    pub buttons: &'static [(usize, &'static str)],
    /// Whether this model's limits were verified; writes are disabled otherwise.
    pub verified: bool,
}

const X11_BUTTONS: &[(usize, &str)] = &[
    (0, "Left button"),
    (1, "Right button"),
    (2, "Middle button"),
    (6, "Forward button"),
    (7, "Back button"),
];

const GENERIC_BUTTONS: &[(usize, &str)] = &[
    (0, "Button 1"),
    (1, "Button 2"),
    (2, "Button 3"),
    (6, "Button 4"),
    (7, "Button 5"),
];

/// Model byte from the status report -> vendor app page.
const MODEL_PAGES: [(u8, u8); 18] = [
    (0x85, 1), (0x4d, 2), (0x10, 3), (0x55, 4), (0x4e, 5), (0xb1, 6),
    (0xbe, 7), (0x01, 8), (0x03, 9), (0x07, 10), (0x02, 11), (0x06, 12),
    (0x08, 13), (0x04, 14), (0x0e, 15), (0xe5, 16), (0x24, 17), (0xea, 18),
];

impl Model {
    pub fn from_id(id: u8) -> Option<Model> {
        let page = MODEL_PAGES.iter().find(|(m, _)| *m == id)?.1;
        if id == 0x55 {
            return Some(Model {
                id,
                name: "Attack Shark X11",
                page,
                stages: 6,
                dpi_slider_max: 320,
                buttons: X11_BUTTONS,
                verified: true,
            });
        }
        Some(Model {
            id,
            name: "Attack Shark mouse",
            page,
            stages: STAGES,
            dpi_slider_max: 320,
            buttons: GENERIC_BUTTONS,
            verified: false,
        })
    }
}

/// Slider position -> DPI, as shown by the vendor app.
pub fn slider_to_dpi(v: u16) -> u32 {
    if v <= 200 { v as u32 * 50 } else { (v as u32 - 100) * 100 }
}

/// DPI -> nearest valid slider position.
pub fn dpi_to_slider(dpi: u32) -> u16 {
    let v = if dpi <= 10_000 {
        ((dpi + 25) / 50).max(1) as u16
    } else {
        ((dpi + 50) / 100 + 100) as u16
    };
    snap_slider(v)
}

/// Above 20000 DPI the mouse works in 200 DPI steps (even slider positions);
/// the vendor app's odd positions there encode to the wrong code.
pub fn snap_slider(v: u16) -> u16 {
    if v > 300 && v % 2 == 1 { v + 1 } else { v }
}

/// Encode a slider position into (code, x2 flag, high-range bit).
pub fn encode_dpi(v: u16) -> (u8, bool, bool) {
    let v = snap_slider(v) as usize;
    let high = v >= 221;
    if v == 0 {
        return (0, false, false);
    }
    if v <= 300 {
        if (221..=300).contains(&v) {
            (DPI_CODES[v - 100], false, high)
        } else {
            (DPI_CODES[v], (201..=220).contains(&v), high)
        }
    } else {
        (DPI_CODES[200 + ((v - 300) >> 1)], true, high)
    }
}

/// Inverse of `encode_dpi`. Searches the same table ranges as the vendor
/// app's reader, but maps the x4 range without its off-by-one at 20200 DPI.
pub fn decode_dpi(code: u8, flag: bool, high: bool) -> u16 {
    if (code, flag, high) == (0, false, false) {
        return 0;
    }
    let range = if flag { 191..=220 } else { 1..=200 };
    let t = range.clone().find(|&t| DPI_CODES[t] == code).unwrap_or(*range.start()) as u16;
    match (high, flag) {
        (false, _) => t,
        (true, false) => t + 100,
        (true, true) => 300 + 2 * (t - 200),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PollingRate {
    Hz125,
    Hz250,
    Hz500,
    Hz1000,
}

impl PollingRate {
    pub const ALL: [PollingRate; 4] = [Self::Hz125, Self::Hz250, Self::Hz500, Self::Hz1000];

    fn code(self) -> u8 {
        match self {
            Self::Hz125 => 8,
            Self::Hz250 => 4,
            Self::Hz500 => 2,
            Self::Hz1000 => 1,
        }
    }

    fn from_code(c: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.code() == c)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hz125 => "125 Hz",
            Self::Hz250 => "250 Hz",
            Self::Hz500 => "500 Hz",
            Self::Hz1000 => "1000 Hz",
        }
    }
}

pub const LIGHT_MODES: [&str; 7] = [
    "Off",
    "Static",
    "Breathing",
    "Neon",
    "Colour breathing",
    "Static (DPI colour)",
    "Breathing (DPI colour)",
];

/// Modes for which the brightness byte is sent.
pub fn light_mode_uses_brightness(mode: u8) -> bool {
    mode == 1 || mode == 5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroRepeat {
    Times,
    UntilKeyPress,
    WhileHeld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroEvent {
    /// HID keyboard usage, or 0xf1..=0xf5 for mouse buttons.
    pub key: u8,
    pub down: bool,
    /// Delay after this event, in ms.
    pub delay_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub repeat: MacroRepeat,
    pub count: u8,
    pub events: Vec<MacroEvent>,
}

/// Mouse buttons usable inside macros.
pub const MACRO_MOUSE_KEYS: [(u8, &str); 5] = [
    (0xf1, "Left click"),
    (0xf2, "Right click"),
    (0xf3, "Middle click"),
    (0xf5, "Forward"),
    (0xf4, "Back"),
];

/// What a button slot does. Stored as the raw 3-byte slot so unknown
/// assignments read from the mouse survive a round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding(pub [u8; 3]);

pub const KIND_KEYBOARD: u8 = 0x11;
pub const KIND_MACRO: u8 = 0x12;

/// Fixed functions offered in the button menu, with the vendor app's code.
pub const BUTTON_FUNCTIONS: &[(u8, &str)] = &[
    (0x01, "Left click"),
    (0x02, "Right click"),
    (0x03, "Middle click"),
    (0x04, "Forward"),
    (0x05, "Back"),
    (0x09, "Double click"),
    (0x0a, "Fire button"),
    (0x16, "Easy aim"),
    (0x17, "LED cycle"),
    (0x0b, "Scroll up"),
    (0x0c, "Scroll down"),
    (0x0e, "Tilt right"),
    (0x0d, "Tilt left"),
    (0x06, "DPI cycle"),
    (0x07, "DPI +"),
    (0x08, "DPI −"),
    (0x40, "Media player"),
    (0x41, "Play / pause"),
    (0x42, "Stop"),
    (0x43, "Previous track"),
    (0x44, "Next track"),
    (0x45, "Volume +"),
    (0x46, "Volume −"),
    (0x47, "Mute"),
    (0x50, "Browser home"),
    (0x51, "Browser favourites"),
    (0x52, "Browser forward"),
    (0x53, "Browser back"),
    (0x54, "Browser stop"),
    (0x55, "Browser refresh"),
    (0x56, "Browser search"),
    (0x57, "Email"),
    (0x58, "Calculator"),
    (0x59, "My computer"),
    (0x20, "Cut"),
    (0x21, "Copy"),
    (0x22, "Paste"),
    (0x23, "Open"),
    (0x24, "Save"),
    (0x25, "Find"),
    (0x26, "Redo"),
    (0x27, "Undo"),
    (0x28, "Select all"),
    (0x29, "Print"),
    (0x2a, "Close window"),
    (0x2b, "Switch windows"),
    (0x2c, "Show desktop"),
    (0x2d, "Run command"),
    (0x2e, "Lock screen"),
    (0x32, "Screen capture"),
    (0x00, "Disabled"),
];

impl Binding {
    pub fn from_app_code(code: u8) -> Binding {
        BUTTON_CODES
            .iter()
            .find(|r| r.0 == code)
            .map(|r| Binding([r.1, r.2, r.3]))
            .unwrap_or(Binding([0x01, 0, 0]))
    }

    pub fn keyboard(modifiers: u8, usage: u8) -> Binding {
        Binding([KIND_KEYBOARD, modifiers, usage])
    }

    pub fn macro_slot(slot: usize) -> Binding {
        Binding([KIND_MACRO, 0, slot as u8 + 1])
    }

    /// Vendor app code of a fixed function, if this binding is one.
    pub fn app_code(self) -> Option<u8> {
        BUTTON_FUNCTIONS
            .iter()
            .map(|(c, _)| *c)
            .find(|&c| Binding::from_app_code(c) == self)
    }

    pub fn is_keyboard(self) -> bool {
        self.0[0] == KIND_KEYBOARD
    }

    pub fn is_macro(self) -> bool {
        self.0[0] == KIND_MACRO
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub dpi_slider: [u16; STAGES],
    pub dpi_enabled: [bool; STAGES],
    pub dpi_color: [Rgb; STAGES],
    /// 0-based.
    pub active_stage: u8,
    pub angle_snap: bool,
    pub ripple_control: bool,
    pub polling: PollingRate,
    pub light_mode: u8,
    /// 1..=8
    pub brightness: u8,
    /// 4..=8
    pub speed: u8,
    pub light_color: Rgb,
    /// Idle time before the light turns off, in units of 30 s (1..=60).
    pub sleep: u8,
    /// Idle time before deep sleep, in minutes (1..=60).
    pub deep_sleep: u8,
    /// Key debounce in units of 2 ms (2..=25).
    pub debounce: u8,
    pub buttons: [Binding; SLOTS],
    /// Macro per button slot; the mouse can't report these back.
    #[serde(default)]
    pub macros: Vec<Option<Macro>>,
}

impl Default for Settings {
    /// Factory profile from the vendor app (MS_4).
    fn default() -> Self {
        let stage_colors = [
            Rgb(0xff, 0, 0),
            Rgb(0, 0xff, 0),
            Rgb(0, 0, 0xff),
            Rgb(0xff, 0xff, 0),
            Rgb(0, 0xff, 0xff),
            Rgb(0xff, 0, 0xff),
            Rgb(0xff, 0x40, 0),
            Rgb(0xff, 0xff, 0xff),
        ];
        Settings {
            dpi_slider: [16, 32, 48, 64, 100, 320, 0, 0],
            dpi_enabled: [true, true, true, true, true, true, false, false],
            dpi_color: stage_colors,
            active_stage: 1,
            angle_snap: false,
            ripple_control: true,
            polling: PollingRate::Hz1000,
            light_mode: 0,
            brightness: 8,
            speed: 6,
            light_color: Rgb(0, 0xff, 0),
            sleep: 1,
            deep_sleep: 10,
            debounce: 4,
            // Unused slots read back as zeros on a factory-fresh mouse.
            buttons: DEFAULT_BUTTON_CODES.map(|c| {
                if c == 0 { Binding([0; 3]) } else { Binding::from_app_code(c) }
            }),
            macros: vec![None; SLOTS],
        }
    }
}

fn checksum(bytes: &[u8]) -> [u8; 2] {
    let sum = bytes.iter().fold(0u16, |acc, &b| acc.wrapping_add(b as u16));
    sum.to_be_bytes()
}

fn bits(flags: impl IntoIterator<Item = bool>) -> u8 {
    flags.into_iter().enumerate().fold(0, |acc, (i, f)| acc | ((f as u8) << i))
}

impl Settings {
    /// Check the rules the vendor app enforces before writing.
    pub fn validate(&self, model: Option<Model>) -> Result<(), String> {
        let stages = model.map_or(STAGES, |m| m.stages);
        if !self.dpi_enabled[..stages].iter().any(|&e| e) {
            return Err("Enable at least one DPI stage".into());
        }
        if !self.dpi_enabled[self.active_stage as usize] {
            return Err("The active DPI stage must be enabled".into());
        }
        let left = Binding::from_app_code(0x01);
        let buttons = model.map_or(&[][..], |m| m.buttons);
        if !buttons.is_empty() && !buttons.iter().any(|&(slot, _)| self.buttons[slot] == left) {
            return Err("At least one button must be Left click".into());
        }
        for &(slot, label) in buttons {
            if self.buttons[slot].is_macro() && self.macros[slot].is_none() {
                return Err(format!("{label} is set to a macro, but no macro is recorded"));
            }
        }
        Ok(())
    }

    pub fn dpi_packet(&self) -> Vec<u8> {
        let mut p = vec![0u8; 0x38];
        p[..3].copy_from_slice(&[REPORT_DPI, 0x38, 0x01]);
        p[3] = self.angle_snap as u8;
        p[4] = self.ripple_control as u8;
        p[5] = bits(self.dpi_enabled);
        for (i, &v) in self.dpi_slider.iter().enumerate() {
            let (code, flag, _) = encode_dpi(v);
            p[8 + i] = code;
            p[16 + i] = flag as u8;
        }
        p[6] = bits(self.dpi_slider.map(|v| encode_dpi(v).2));
        p[7] = p[6];
        p[24] = self.active_stage + 1;
        for (i, c) in self.dpi_color.iter().enumerate() {
            p[25 + i * 3..28 + i * 3].copy_from_slice(&[c.0, c.1, c.2]);
        }
        p[49] = 0x02;
        let sum = checksum(&p[3..50]);
        p[50..52].copy_from_slice(&sum);
        p
    }

    pub fn light_packet(&self) -> Vec<u8> {
        let mut p = vec![0u8; 0x0f];
        p[..3].copy_from_slice(&[REPORT_LIGHT, 0x0f, 0x01]);
        p[3] = self.light_mode << 4;
        p[4] = (self.deep_sleep & 0xf0) | (9 - self.speed);
        let level = if light_mode_uses_brightness(self.light_mode) { self.brightness } else { 8 };
        p[5] = (self.deep_sleep << 4) | level;
        p[6..9].copy_from_slice(&[self.light_color.0, self.light_color.1, self.light_color.2]);
        p[9] = self.sleep;
        p[10] = self.debounce;
        let sum = checksum(&p[3..11]);
        p[11..13].copy_from_slice(&sum);
        p
    }

    pub fn polling_packet(&self) -> Vec<u8> {
        let rate = self.polling.code();
        let mut p = vec![0u8; 9];
        p[..5].copy_from_slice(&[REPORT_POLLING, 0x09, 0x01, rate, !rate]);
        p
    }

    pub fn buttons_packet(&self) -> Vec<u8> {
        let mut p = vec![0u8; 0x3b];
        p[..3].copy_from_slice(&[REPORT_BUTTONS, 0x3b, 0x01]);
        for (i, b) in self.buttons.iter().enumerate() {
            p[3 + i * 3..6 + i * 3].copy_from_slice(&b.0);
        }
        let sum = checksum(&p[3..57]);
        p[57..59].copy_from_slice(&sum);
        p
    }

    /// Feature reports that upload the macro bound to `slot`.
    pub fn macro_packets(&self, slot: usize) -> Option<Vec<Vec<u8>>> {
        let m = self.macros.get(slot)?.as_ref()?;
        Some(macro_packets(slot as u8 + 1, m))
    }

    /// Apply a bulk (report 0x0a) read on top of `self`.
    pub fn apply_bulk(&mut self, b: &[u8]) -> Result<(), String> {
        if b.len() < 120 || b[0] != REPORT_BULK {
            return Err(format!("unexpected bulk report ({} bytes)", b.len()));
        }
        if let Some(rate) = PollingRate::from_code(b[3]) {
            self.polling = rate;
        }
        self.apply_dpi_payload(&b[5..52]);
        self.apply_light_payload(&b[55..63]);
        for i in 0..SLOTS {
            self.buttons[i] = Binding([b[65 + i * 3], b[66 + i * 3], b[67 + i * 3]]);
        }
        Ok(())
    }

    /// Report 0x04 payload (bytes 3.. of the packet).
    pub fn apply_dpi_payload(&mut self, p: &[u8]) {
        self.angle_snap = p[0] != 0;
        self.ripple_control = p[1] != 0;
        for i in 0..STAGES {
            self.dpi_enabled[i] = p[2] & (1 << i) != 0;
            let high = p[3] & (1 << i) != 0;
            self.dpi_slider[i] = decode_dpi(p[5 + i], p[13 + i] != 0, high);
        }
        self.active_stage = p[21].saturating_sub(1).min(STAGES as u8 - 1);
        for i in 0..STAGES {
            let c = &p[22 + i * 3..25 + i * 3];
            self.dpi_color[i] = Rgb(c[0], c[1], c[2]);
        }
    }

    /// Report 0x05 payload (bytes 3.. of the packet).
    pub fn apply_light_payload(&mut self, p: &[u8]) {
        self.light_mode = (p[0] >> 4).min(LIGHT_MODES.len() as u8 - 1);
        self.speed = (9 - (p[1] & 0x0f).min(9)).clamp(4, 8);
        self.brightness = (p[2] & 0x0f).clamp(1, 8);
        self.deep_sleep = ((p[2] >> 4) | (p[1] & 0xf0)).clamp(1, 60);
        self.light_color = Rgb(p[3], p[4], p[5]);
        self.sleep = p[6].clamp(1, 60);
        self.debounce = p[7].clamp(2, 25);
    }

    pub fn apply_polling_payload(&mut self, p: &[u8]) {
        if let Some(rate) = PollingRate::from_code(p[0]) {
            self.polling = rate;
        }
    }
}

pub fn macro_packets(number: u8, m: &Macro) -> Vec<Vec<u8>> {
    let mut p = vec![0u8; 0x83];
    p[..3].copy_from_slice(&[REPORT_MACRO, 0x83, number]);
    p[3] = match m.repeat {
        MacroRepeat::Times => 0,
        MacroRepeat::UntilKeyPress => 1,
        MacroRepeat::WhileHeld => 2,
    };
    p[7] = m.count.max(1);
    let direction = |down: bool| if down { 0x01 } else { 0x81 };
    let ticks = |ms: u16| ((ms.max(1) as f32 / 10.0) as f64 + 0.5) as u8;
    let mut out = Vec::new();
    for e in m.events.iter().take(MACRO_MAX_EVENTS) {
        if e.delay_ms < 0x4f7 {
            out.extend([ticks(e.delay_ms) | direction(e.down), e.key]);
        } else {
            let rest = (e.delay_ms % 200).max(1);
            out.extend([ticks(rest) | direction(e.down), e.key]);
            out.extend([(e.delay_ms / 200) as u8, 0x03]);
        }
    }
    let room = 0x81 - 29;
    out.truncate(room - room % 2);
    p[28] = (out.len() / 2) as u8;
    p[29..29 + out.len()].copy_from_slice(&out);
    let sum = checksum(&p[3..0x81]);
    p[0x81..0x83].copy_from_slice(&sum);

    p[3..]
        .chunks(60)
        .enumerate()
        .map(|(k, chunk)| {
            let len = if k == 2 { 0x0c } else { 0x40 };
            let mut c = vec![0u8; 64];
            c[..4].copy_from_slice(&[REPORT_MACRO, len, number, k as u8]);
            c[4..4 + chunk.len()].copy_from_slice(chunk);
            c
        })
        .collect()
}

pub fn query_packet(report: u8, len: u8, flag: u8) -> [u8; 8] {
    [REPORT_QUERY, report, len, 0, flag, 0, 0, 0]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    StageChanged(u8),
    PollingChanged(u8),
    Battery { percent: u8, charging: bool, full: bool },
    Ack { report: u8, ok: bool },
    LightModeChanged(u8),
    Other(u8),
}

/// Parse a 0x03 input report into (model byte, event).
pub fn parse_status(b: &[u8]) -> Option<(u8, Status)> {
    if b.len() < 5 || b[0] != REPORT_STATUS {
        return None;
    }
    let (model, kind, lo, hi) = (b[1], b[2], b[3], b[4]);
    let status = match kind {
        0x10 => Status::StageChanged(lo.saturating_sub(1)),
        0x20 => Status::PollingChanged((lo & 0x0f).saturating_sub(1)),
        0x40 => Status::Battery { percent: hi.min(100), charging: lo == 3, full: lo == 2 },
        0x50 => Status::Ack { report: hi, ok: lo == 0 },
        0x70 => Status::LightModeChanged(lo >> 4),
        k => Status::Other(k),
    };
    Some((model, status))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bulk report read from a real X11 (factory settings, firmware 0x0110).
    const X11_BULK: &str = "0a8001 01fe0001 3f2020 12253 84b7581 0000000000000001 0000 01 ff0000 00ff00 0000ff ffff00 00ffff ff00ff ff4000 ffffff 02 00670f 0003a800ff000104 af01 020000 030000 040000 000000 000000 0d0000 060000 050000 000000000000000000000000000000000000000000000000 090000 0a0000 34 0000000000000000";

    fn x11_bulk() -> Vec<u8> {
        let h: String = X11_BULK.split_whitespace().collect();
        (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn bulk_read_matches_factory_profile() {
        let bulk = x11_bulk();
        assert_eq!(bulk.len(), 128);
        let mut s = Settings::default();
        s.dpi_slider = [0; STAGES];
        s.apply_bulk(&bulk).unwrap();
        let d = Settings::default();
        assert_eq!(s.dpi_slider, d.dpi_slider);
        assert_eq!(s.dpi_enabled, d.dpi_enabled);
        assert_eq!(s.dpi_color, d.dpi_color);
        assert_eq!(s.polling, PollingRate::Hz1000);
        assert_eq!(s.active_stage, 0);
        assert_eq!((s.angle_snap, s.ripple_control), (false, true));
        assert_eq!((s.light_mode, s.brightness, s.speed), (0, 8, 6));
        assert_eq!(s.light_color, Rgb(0, 0xff, 0));
        assert_eq!((s.sleep, s.deep_sleep, s.debounce), (1, 10, 4));
        assert_eq!(s.buttons, d.buttons);
    }

    #[test]
    fn dpi_round_trips_for_every_slider_position() {
        for v in (1..=340u16).filter(|&v| snap_slider(v) == v) {
            let (code, flag, high) = encode_dpi(v);
            assert_eq!(decode_dpi(code, flag, high), v, "slider {v}");
        }
    }

    #[test]
    fn dpi_encoding_matches_vendor_app() {
        // (slider, code, flag, high) as produced by the vendor app's writer.
        assert_eq!(encode_dpi(16), (0x12, false, false));
        assert_eq!(encode_dpi(100), (0x75, false, false));
        assert_eq!(encode_dpi(210), (0x81, true, false));
        assert_eq!(encode_dpi(221), (DPI_CODES[121], false, true));
        assert_eq!(encode_dpi(320), (0x81, true, true));
    }

    #[test]
    fn dpi_labels() {
        assert_eq!(slider_to_dpi(16), 800);
        assert_eq!(slider_to_dpi(320), 22000);
        assert_eq!(dpi_to_slider(22000), 320);
        assert_eq!(dpi_to_slider(10100), 201);
    }

    #[test]
    fn dpi_packet_payload_matches_device_layout() {
        let bulk = x11_bulk();
        let mut s = Settings::default();
        s.apply_bulk(&bulk).unwrap();
        let p = s.dpi_packet();
        assert_eq!(&p[3..50], &bulk[5..52]);
        assert_eq!(&p[50..52], &[0x0f, 0x67]);
    }

    #[test]
    fn light_packet_payload_matches_device_layout() {
        let bulk = x11_bulk();
        let mut s = Settings::default();
        s.apply_bulk(&bulk).unwrap();
        let p = s.light_packet();
        assert_eq!(&p[3..11], &bulk[55..63]);
        assert_eq!(&p[11..13], &[0x01, 0xaf]);
    }

    #[test]
    fn polling_packet() {
        let s = Settings::default();
        assert_eq!(s.polling_packet(), vec![6, 9, 1, 1, 0xfe, 0, 0, 0, 0]);
    }

    #[test]
    fn button_codes() {
        assert_eq!(Binding::from_app_code(0x01).0, [0x02, 0, 0]);
        assert_eq!(Binding::from_app_code(0x2e).0, [KIND_KEYBOARD, 0x08, 0x0f]);
        assert_eq!(Binding([0x06, 0, 0]).app_code(), Some(0x04));
        for (code, name) in BUTTON_FUNCTIONS {
            assert!(BUTTON_CODES.iter().any(|r| r.0 == *code), "{name}");
        }
    }

    #[test]
    fn macro_chunks() {
        let m = Macro {
            name: "a".into(),
            repeat: MacroRepeat::Times,
            count: 1,
            events: vec![
                MacroEvent { key: 0x04, down: true, delay_ms: 50 },
                MacroEvent { key: 0x04, down: false, delay_ms: 2000 },
            ],
        };
        let c = macro_packets(1, &m);
        assert_eq!(c.len(), 3);
        assert_eq!(&c[0][..4], &[9, 0x40, 1, 0]);
        assert_eq!(&c[2][..4], &[9, 0x0c, 1, 2]);
        // count, then (5|down, A), (rest 0 -> 1 tick | up, A), (2000/200, 3)
        assert_eq!(c[0][4 + 25], 3);
        assert_eq!(&c[0][4 + 26..4 + 32], &[0x05, 0x04, 0x81, 0x04, 0x0a, 0x03]);
    }

    #[test]
    fn status_reports() {
        assert_eq!(
            parse_status(&[3, 0x55, 0x40, 1, 0x63]),
            Some((0x55, Status::Battery { percent: 99, charging: false, full: false }))
        );
        assert_eq!(parse_status(&[3, 0x55, 0x50, 0, 6]), Some((0x55, Status::Ack { report: 6, ok: true })));
    }
}

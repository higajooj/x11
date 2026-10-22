//! hidraw access and the background worker that owns the device.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{
    self, Settings, Status, PRODUCT_IDS, REPORT_BULK, REPORT_BUTTONS, REPORT_DPI, REPORT_LIGHT,
    REPORT_MACRO, REPORT_POLLING, REPORT_QUERY, REPORT_VERSION, VENDOR_ID,
};

/// Without a status report for this long, the mouse is asleep or out of range.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(6);
const WRITE_ATTEMPTS: usize = 10;
const ACK_TIMEOUT: Duration = Duration::from_millis(500);

/// Top-level collection on usage page 0x0B that carries the feature reports.
const CONFIG_COLLECTION: [u8; 4] = [0x05, 0x0b, 0x09, 0x00];

fn hid_ioctl(nr: u64, len: usize) -> u64 {
    // _IOC(_IOC_READ | _IOC_WRITE, 'H', nr, len)
    (3 << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | nr
}

pub struct Hidraw {
    file: File,
}

impl Hidraw {
    /// Find the receiver's configuration interface.
    pub fn find() -> Option<PathBuf> {
        let ids: Vec<String> = PRODUCT_IDS
            .iter()
            .map(|pid| format!("HID_ID=0003:{:08X}:{:08X}", VENDOR_ID, pid))
            .collect();
        let mut nodes: Vec<_> = fs::read_dir("/sys/class/hidraw").ok()?.flatten().collect();
        nodes.sort_by_key(|e| e.file_name());
        nodes.into_iter().find_map(|entry| {
            let dev = entry.path().join("device");
            let uevent = fs::read_to_string(dev.join("uevent")).ok()?;
            if !ids.iter().any(|id| uevent.lines().any(|l| l == id)) {
                return None;
            }
            let desc = fs::read(dev.join("report_descriptor")).ok()?;
            desc.windows(4)
                .any(|w| w == CONFIG_COLLECTION)
                .then(|| Path::new("/dev").join(entry.file_name()))
        })
    }

    pub fn open(path: &Path) -> io::Result<Hidraw> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        Ok(Hidraw { file })
    }

    pub fn set_feature(&self, data: &[u8]) -> io::Result<()> {
        let mut buf = data.to_vec();
        let r = unsafe {
            libc::ioctl(self.file.as_raw_fd(), hid_ioctl(0x06, buf.len()) as _, buf.as_mut_ptr())
        };
        if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    pub fn get_feature(&self, report: u8, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        buf[0] = report;
        let r = unsafe {
            libc::ioctl(self.file.as_raw_fd(), hid_ioctl(0x07, len) as _, buf.as_mut_ptr())
        };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        buf.truncate(r as usize);
        Ok(buf)
    }

    /// Wait up to `timeout` for one input report.
    pub fn read_input(&mut self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let r = unsafe { libc::poll(&mut pfd, 1, timeout.as_millis() as i32) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "device removed"));
        }
        if r == 0 {
            return Ok(None);
        }
        let mut buf = [0u8; 64];
        match self.file.read(&mut buf) {
            Ok(n) => Ok(Some(buf[..n].to_vec())),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// One logical write: feature reports followed by an ack for `report`.
#[derive(Debug, Clone)]
pub struct Write {
    pub what: &'static str,
    pub report: u8,
    pub packets: Vec<Vec<u8>>,
}

/// The feature reports needed to turn `old` (what the mouse has) into `new`.
pub fn plan_writes(new: &Settings, old: Option<&Settings>) -> Vec<Write> {
    let changed = |f: &dyn Fn(&Settings) -> Vec<u8>| old.is_none_or(|o| f(o) != f(new));
    let mut writes = Vec::new();
    for (slot, b) in new.buttons.iter().enumerate() {
        let fresh = old.is_none_or(|o| o.buttons[slot] != *b || o.macros[slot] != new.macros[slot]);
        if b.is_macro() && fresh {
            if let Some(packets) = new.macro_packets(slot) {
                writes.push(Write { what: "macro", report: REPORT_MACRO, packets });
            }
        }
    }
    let sections: [(&str, u8, fn(&Settings) -> Vec<u8>); 4] = [
        ("DPI settings", REPORT_DPI, Settings::dpi_packet),
        ("lighting and power settings", REPORT_LIGHT, Settings::light_packet),
        ("polling rate", REPORT_POLLING, Settings::polling_packet),
        ("button assignments", REPORT_BUTTONS, Settings::buttons_packet),
    ];
    for (what, report, packet) in sections {
        if changed(&packet) {
            writes.push(Write { what, report, packets: vec![packet(new)] });
        }
    }
    writes
}

#[derive(Debug)]
pub enum Command {
    Read,
    Write(Vec<Write>),
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Receiver found; `None` when it's gone.
    Receiver(Option<PathBuf>),
    PermissionDenied(PathBuf),
    /// Mouse model byte from status reports; `None` when the mouse stops reporting.
    Mouse(Option<u8>),
    Status(Status),
    Settings { settings: Box<Settings>, version: u16 },
    Busy(Option<String>),
    WriteDone(std::result::Result<(), String>),
    ReadFailed(String),
}

pub struct Worker {
    commands: mpsc::Sender<Command>,
}

impl Worker {
    pub fn spawn(events: async_channel::Sender<Event>) -> Worker {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("attack-shark-hid".into())
            .spawn(move || Session::new(events).run(rx))
            .expect("spawn HID worker");
        Worker { commands: tx }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.commands.send(cmd);
    }
}

struct Session {
    events: async_channel::Sender<Event>,
    dev: Option<Hidraw>,
    denied: Option<PathBuf>,
    model: Option<u8>,
    last_status: Option<Instant>,
}

type Result<T> = std::result::Result<T, String>;

impl Session {
    fn new(events: async_channel::Sender<Event>) -> Session {
        Session { events, dev: None, denied: None, model: None, last_status: None }
    }

    fn emit(&self, e: Event) {
        let _ = self.events.send_blocking(e);
    }

    fn run(mut self, commands: mpsc::Receiver<Command>) {
        loop {
            if self.dev.is_none() && !self.connect() {
                match commands.recv_timeout(Duration::from_secs(1)) {
                    Ok(Command::Read) => self.emit(Event::ReadFailed("No receiver connected".into())),
                    Ok(Command::Write(_)) => {
                        self.emit(Event::WriteDone(Err("No receiver connected".into())))
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                continue;
            }
            loop {
                match commands.try_recv() {
                    Ok(cmd) => self.handle(cmd),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if let Err(e) = self.pump(Duration::from_millis(200)) {
                self.disconnect(e);
            }
            if self.model.is_some()
                && self.last_status.is_some_and(|t| t.elapsed() > HEARTBEAT_TIMEOUT)
            {
                self.model = None;
                self.emit(Event::Mouse(None));
            }
        }
    }

    fn connect(&mut self) -> bool {
        let Some(path) = Hidraw::find() else {
            self.denied = None;
            return false;
        };
        match Hidraw::open(&path) {
            Ok(dev) => {
                self.denied = None;
                self.dev = Some(dev);
                self.last_status = Some(Instant::now());
                self.emit(Event::Receiver(Some(path)));
                true
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                if self.denied.as_ref() != Some(&path) {
                    self.denied = Some(path.clone());
                    self.emit(Event::PermissionDenied(path));
                }
                false
            }
            Err(_) => false,
        }
    }

    fn disconnect(&mut self, why: String) {
        eprintln!("receiver lost: {why}");
        self.dev = None;
        self.model = None;
        self.emit(Event::Mouse(None));
        self.emit(Event::Receiver(None));
    }

    /// Read one input report (if any) and forward it. Returns acks it saw.
    fn pump(&mut self, timeout: Duration) -> Result<Option<(u8, bool)>> {
        let dev = self.dev.as_mut().ok_or("no device")?;
        let Some(report) = dev.read_input(timeout).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let Some((model, status)) = protocol::parse_status(&report) else {
            return Ok(None);
        };
        self.last_status = Some(Instant::now());
        if self.model != Some(model) {
            self.model = Some(model);
            self.emit(Event::Mouse(Some(model)));
        }
        if let Status::Ack { report, ok } = status {
            return Ok(Some((report, ok)));
        }
        self.emit(Event::Status(status));
        Ok(None)
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Read => {
                self.emit(Event::Busy(Some("Reading settings from the mouse…".into())));
                let r = self.read_settings();
                self.emit(Event::Busy(None));
                match r {
                    Ok((settings, version)) => {
                        self.emit(Event::Settings { settings: Box::new(settings), version })
                    }
                    Err(e) => self.emit(Event::ReadFailed(e)),
                }
            }
            Command::Write(writes) => {
                let r = self.write_all(&writes);
                self.emit(Event::Busy(None));
                self.emit(Event::WriteDone(r));
            }
        }
    }

    fn dev(&self) -> Result<&Hidraw> {
        self.dev.as_ref().ok_or_else(|| "No receiver connected".to_string())
    }

    /// Ask the mouse for a report and fetch it once it says it's ready.
    fn query(&mut self, report: u8, len: u8, flag: u8) -> Result<Vec<u8>> {
        let io = |e: io::Error| format!("report {report:#04x}: {e}");
        self.dev()?.set_feature(&protocol::query_packet(report, len, flag)).map_err(io)?;
        let mut ready = false;
        for _ in 0..10 {
            self.pump(Duration::from_millis(100))?;
            let r = self.dev()?.get_feature(REPORT_QUERY, 8).map_err(io)?;
            if r.get(1) == Some(&1) {
                ready = true;
                break;
            }
        }
        if !ready {
            return Err(format!("report {report:#04x}: mouse did not answer (asleep?)"));
        }
        let data = self.dev()?.get_feature(report, len as usize).map_err(io)?;
        if data.first() != Some(&report) {
            return Err(format!("report {report:#04x}: unexpected reply"));
        }
        Ok(data)
    }

    fn read_settings(&mut self) -> Result<(Settings, u16)> {
        let v = self.query(REPORT_VERSION, 8, 0)?;
        let version = u16::from_be_bytes([v[2], v[3]]);
        let mut s = Settings::default();
        // The vendor app reads reports one by one when the version's high
        // byte equals the model byte, and the bulk report otherwise.
        if self.model.is_some_and(|m| m == (version >> 8) as u8) {
            let p = self.query(REPORT_POLLING, 9, 1)?;
            s.apply_polling_payload(&p[3..]);
            let p = self.query(REPORT_LIGHT, 0x0f, 1)?;
            s.apply_light_payload(&p[3..]);
            let p = self.query(REPORT_DPI, 0x38, 1)?;
            s.apply_dpi_payload(&p[3..]);
        } else {
            let bulk = self.query(REPORT_BULK, 0x80, 1)?;
            s.apply_bulk(&bulk)?;
        }
        Ok((s, version))
    }

    fn write_all(&mut self, writes: &[Write]) -> Result<()> {
        for w in writes {
            self.emit(Event::Busy(Some(format!("Writing {}…", w.what))));
            self.write_one(w)?;
        }
        Ok(())
    }

    /// Send a write and wait for its ack, retrying like the vendor app does.
    fn write_one(&mut self, w: &Write) -> Result<()> {
        for _ in 0..WRITE_ATTEMPTS {
            for p in &w.packets {
                self.dev()?.set_feature(p).map_err(|e| format!("{}: {e}", w.what))?;
                if w.packets.len() > 1 {
                    thread::sleep(Duration::from_millis(100));
                }
            }
            let deadline = Instant::now() + ACK_TIMEOUT;
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                match self.pump(left)? {
                    Some((report, true)) if report == w.report => return Ok(()),
                    Some((report, false)) if report == w.report => break,
                    _ => {}
                }
            }
        }
        Err(format!("{}: the mouse did not acknowledge the change", w.what))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Binding;

    #[test]
    fn only_changed_sections_are_written() {
        let old = Settings::default();
        assert!(plan_writes(&old, Some(&old)).is_empty());
        let mut new = old.clone();
        new.polling = protocol::PollingRate::Hz500;
        let w = plan_writes(&new, Some(&old));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].packets[0][..5], [6, 9, 1, 2, 0xfd]);
        assert_eq!(plan_writes(&new, None).len(), 4);
    }

    #[test]
    fn macro_is_written_before_buttons() {
        let old = Settings::default();
        let mut new = old.clone();
        new.buttons[6] = Binding::macro_slot(6);
        new.macros[6] = Some(protocol::Macro {
            name: "m".into(),
            repeat: protocol::MacroRepeat::Times,
            count: 1,
            events: vec![],
        });
        let w = plan_writes(&new, Some(&old));
        assert_eq!(w.iter().map(|w| w.report).collect::<Vec<_>>(), [REPORT_MACRO, REPORT_BUTTONS]);
        assert_eq!(w[0].packets[0][2], 7);
    }
}

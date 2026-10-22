//! Command-line modes: `--read` and `--apply PROFILE`.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use crate::device::{plan_writes, Command, Event, Worker};
use crate::protocol::{Model, Settings};
use crate::store;

const USAGE: &str = "usage: attack-shark [--read | --apply PROFILE.json]";

/// Handle command-line options; `None` means start the GUI.
pub fn run(args: &[String]) -> Option<i32> {
    let result = match args.first().map(String::as_str) {
        None => return None,
        Some("--read") => read(),
        Some("--apply") => match args.get(1) {
            Some(path) => apply(Path::new(path)),
            None => Err(USAGE.into()),
        },
        Some("--help" | "-h") => {
            println!("{USAGE}");
            Ok(())
        }
        // Leave GTK's own options to GTK.
        Some(_) => return None,
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    })
}

struct Connection {
    worker: Worker,
    events: async_channel::Receiver<Event>,
}

impl Connection {
    /// Wait for an event `f` accepts, failing on errors or after `timeout`.
    fn wait<T>(&self, timeout: Duration, mut f: impl FnMut(Event) -> Option<T>) -> Result<T, String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.events.try_recv() {
                Ok(Event::PermissionDenied(path)) => {
                    return Err(format!(
                        "permission denied opening {} (install the udev rule)",
                        path.display()
                    ))
                }
                Ok(Event::ReadFailed(e)) => return Err(format!("read failed: {e}")),
                Ok(event) => {
                    if let Some(v) = f(event) {
                        return Ok(v);
                    }
                }
                Err(async_channel::TryRecvError::Empty) => thread::sleep(Duration::from_millis(20)),
                Err(async_channel::TryRecvError::Closed) => break,
            }
        }
        Err("timed out waiting for the mouse (is it awake?)".into())
    }
}

fn connect_and_read() -> Result<(Connection, Model, Settings, u16), String> {
    let (tx, rx) = async_channel::unbounded();
    let conn = Connection { worker: Worker::spawn(tx), events: rx };
    let id = conn.wait(Duration::from_secs(10), |e| match e {
        Event::Mouse(Some(id)) => Some(id),
        _ => None,
    })?;
    let model = Model::from_id(id).ok_or_else(|| format!("unknown mouse model {id:#04x}"))?;
    conn.worker.send(Command::Read);
    let (settings, version) = conn.wait(Duration::from_secs(10), |e| match e {
        Event::Settings { settings, version } => Some((*settings, version)),
        _ => None,
    })?;
    Ok((conn, model, settings, version))
}

fn read() -> Result<(), String> {
    let (_, model, settings, version) = connect_and_read()?;
    eprintln!("{} (model {:#04x}), firmware {version:#06x}", model.name, model.id);
    println!("{}", serde_json::to_string_pretty(&settings).expect("settings serialize"));
    Ok(())
}

fn apply(path: &Path) -> Result<(), String> {
    let profile = store::load(path)?;
    let (conn, model, current, _) = connect_and_read()?;
    if !model.verified {
        return Err(format!("model {:#04x} isn't verified; refusing to write", model.id));
    }
    profile.validate(Some(model))?;
    let writes = plan_writes(&profile, Some(&current));
    if writes.is_empty() {
        eprintln!("the mouse already has these settings");
        return Ok(());
    }
    for w in &writes {
        eprintln!("writing {}", w.what);
    }
    conn.worker.send(Command::Write(writes));
    conn.wait(Duration::from_secs(60), |e| match e {
        Event::WriteDone(r) => Some(r),
        _ => None,
    })??;
    store::save_current(&profile)?;
    eprintln!("done");
    Ok(())
}

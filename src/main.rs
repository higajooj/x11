mod device;
mod protocol;
mod tables;

use std::time::{Duration, Instant};

use device::{Command, Event, Worker};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--read") {
        std::process::exit(read_to_stdout());
    }
}

/// Print the mouse's current settings as JSON.
fn read_to_stdout() -> i32 {
    let (tx, rx) = async_channel::unbounded();
    let worker = Worker::spawn(tx);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut requested = false;
    while Instant::now() < deadline {
        let Ok(event) = rx.recv_blocking() else { break };
        match event {
            Event::PermissionDenied(path) => {
                eprintln!("permission denied opening {} (install the udev rule)", path.display());
                return 1;
            }
            Event::Mouse(Some(model)) if !requested => {
                eprintln!("mouse model byte {model:#04x}");
                requested = true;
                worker.send(Command::Read);
            }
            Event::Settings { settings, version } => {
                eprintln!("firmware version {version:#06x}");
                println!("{}", serde_json::to_string_pretty(&settings).unwrap());
                return 0;
            }
            Event::ReadFailed(e) => {
                eprintln!("read failed: {e}");
                return 1;
            }
            _ => {}
        }
    }
    eprintln!("no mouse found");
    1
}

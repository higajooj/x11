mod device;
mod keys;
mod protocol;
mod store;
mod tables;
mod ui;

use std::time::{Duration, Instant};

use adw::prelude::*;

use device::{Command, Event, Worker};

fn main() -> gtk::glib::ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--read") {
        std::process::exit(read_to_stdout());
    }
    let app = adw::Application::builder().application_id(ui::APP_ID).build();
    app.connect_activate(ui::build);
    app.run()
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

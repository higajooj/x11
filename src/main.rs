mod cli;
mod device;
mod keys;
mod protocol;
mod store;
mod tables;
mod ui;

use adw::prelude::*;

fn main() -> gtk::glib::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    let app = adw::Application::builder().application_id(ui::APP_ID).build();
    app.connect_activate(ui::build);
    app.run()
}

mod buttons;
mod lighting;
mod macro_editor;
mod performance;
mod power;
mod shortcut;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::device::{plan_writes, Command, Event, Worker};
use crate::protocol::{self, Model, Rgb, Settings, Status};
use crate::store;

pub const APP_ID: &str = "io.github.AttackSharkGtk";

#[derive(Default)]
pub struct State {
    pub settings: Settings,
    /// What the mouse is known to hold.
    pub synced: Option<Settings>,
    /// Settings being written, promoted to `synced` on success.
    pending: Option<Settings>,
    pub model: Option<Model>,
    receiver: Option<PathBuf>,
    denied: Option<PathBuf>,
    pub battery: Option<(u8, bool, bool)>,
    pub version: Option<u16>,
    busy: Option<String>,
}

impl State {
    fn can_write(&self) -> bool {
        self.busy.is_none() && self.model.is_some_and(|m| m.verified)
    }
}

pub struct Ctx {
    pub state: RefCell<State>,
    loading: Cell<bool>,
    refreshers: RefCell<Vec<Box<dyn Fn(&Ctx)>>>,
    worker: Worker,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    apply: gtk::Button,
    read: gtk::Button,
    battery_icon: gtk::Image,
    battery_label: gtk::Label,
    spinner: adw::Spinner,
}

pub type CtxRef = Rc<Ctx>;

impl Ctx {
    /// Change the edited settings from a widget handler.
    pub fn edit(&self, f: impl FnOnce(&mut Settings)) {
        if self.loading.get() {
            return;
        }
        f(&mut self.state.borrow_mut().settings);
        self.update_chrome();
    }

    /// Like `edit`, then push the result back into every widget.
    pub fn edit_and_refresh(&self, f: impl FnOnce(&mut Settings)) {
        if self.loading.get() {
            return;
        }
        f(&mut self.state.borrow_mut().settings);
        self.refresh();
    }

    /// True while widgets are being filled from the settings.
    pub fn is_loading(&self) -> bool {
        self.loading.get()
    }

    pub fn on_refresh(&self, f: impl Fn(&Ctx) + 'static) {
        self.refreshers.borrow_mut().push(Box::new(f));
    }

    pub fn refresh(&self) {
        self.loading.set(true);
        for r in self.refreshers.borrow().iter() {
            r(self);
        }
        self.loading.set(false);
        self.update_chrome();
    }

    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    fn update_chrome(&self) {
        let st = self.state.borrow();
        let dirty = st.synced.as_ref() != Some(&st.settings);
        self.apply.set_sensitive(st.can_write() && dirty);
        self.read.set_sensitive(st.busy.is_none() && st.model.is_some());
        self.spinner.set_visible(st.busy.is_some());
        self.spinner.set_tooltip_text(st.busy.as_deref());

        match st.battery.filter(|_| st.model.is_some()) {
            Some((percent, charging, full)) => {
                let level = (percent as u32 + 5) / 10 * 10;
                let icon = if charging || full {
                    format!("battery-level-{level}-charging-symbolic")
                } else {
                    format!("battery-level-{level}-symbolic")
                };
                self.battery_icon.set_icon_name(Some(&icon));
                self.battery_label.set_text(&format!("{percent}%"));
                self.battery_icon.set_visible(true);
                self.battery_label.set_visible(true);
            }
            None => {
                self.battery_icon.set_visible(false);
                self.battery_label.set_visible(false);
            }
        }

        let message = if let Some(path) = &st.denied {
            Some(format!(
                "Permission denied opening {}. Install the udev rule from the README.",
                path.display()
            ))
        } else if st.receiver.is_none() {
            Some("No Attack Shark receiver found. Plug it in or connect the cable.".into())
        } else if st.model.is_none() {
            Some("Receiver found, but the mouse isn't responding. Move it to wake it up.".into())
        } else if let Some(m) = st.model.filter(|m| !m.verified) {
            Some(format!(
                "Model {:#04x} (vendor page MS_{}) hasn't been verified, so changes are disabled.",
                m.id, m.page
            ))
        } else {
            None
        };
        self.banner.set_revealed(message.is_some());
        if let Some(m) = message {
            self.banner.set_title(&m);
        }
    }

    fn handle(&self, event: Event) {
        match event {
            Event::Receiver(path) => {
                let mut st = self.state.borrow_mut();
                st.denied = None;
                st.receiver = path;
            }
            Event::PermissionDenied(path) => self.state.borrow_mut().denied = Some(path),
            Event::Mouse(id) => {
                let model = id.and_then(Model::from_id);
                let was_connected = self.state.borrow().model.is_some();
                {
                    let mut st = self.state.borrow_mut();
                    st.model = model;
                    if model.is_none() {
                        st.battery = None;
                    }
                }
                if !was_connected && model.is_some() {
                    self.worker.send(Command::Read);
                }
                self.refresh();
            }
            Event::Status(status) => self.handle_status(status),
            Event::Settings { settings, version } => self.loaded(*settings, version),
            Event::Busy(text) => self.state.borrow_mut().busy = text,
            Event::WriteDone(result) => {
                let mut st = self.state.borrow_mut();
                let pending = st.pending.take();
                match result {
                    Ok(()) => {
                        if let Some(p) = pending {
                            if let Err(e) = store::save_current(&p) {
                                eprintln!("saving profile: {e}");
                            }
                            st.synced = Some(p);
                        }
                        drop(st);
                        self.toast("Settings saved to the mouse");
                    }
                    Err(e) => {
                        drop(st);
                        self.toast(&e);
                        // Some writes may have landed; find out what the mouse holds now.
                        self.worker.send(Command::Read);
                    }
                }
            }
            Event::ReadFailed(e) => self.toast(&format!("Couldn't read settings: {e}")),
        }
        self.update_chrome();
    }

    fn handle_status(&self, status: Status) {
        let update = |f: &dyn Fn(&mut Settings)| {
            let mut st = self.state.borrow_mut();
            f(&mut st.settings);
            if let Some(s) = st.synced.as_mut() {
                f(s);
            }
        };
        match status {
            Status::Battery { percent, charging, full } => {
                let battery = Some((percent, charging, full));
                if self.state.borrow().battery == battery {
                    return;
                }
                self.state.borrow_mut().battery = battery;
            }
            Status::StageChanged(stage) => {
                update(&|s| s.active_stage = stage.min(protocol::STAGES as u8 - 1))
            }
            Status::PollingChanged(i) => {
                if let Some(&rate) = protocol::PollingRate::ALL.get(i as usize) {
                    update(&|s| s.polling = rate);
                }
            }
            Status::LightModeChanged(mode) => {
                update(&|s| s.light_mode = mode.min(protocol::LIGHT_MODES.len() as u8 - 1))
            }
            Status::Ack { .. } | Status::Other(_) => return,
        }
        self.refresh();
    }

    fn loaded(&self, mut settings: Settings, version: u16) {
        // Macros can't be read back; restore them from the last applied profile.
        if let Some(saved) = store::load_current() {
            for (i, b) in settings.buttons.iter().enumerate() {
                if b.is_macro() {
                    settings.macros[i] = saved.macros.get(i).cloned().flatten();
                }
            }
        }
        {
            let mut st = self.state.borrow_mut();
            st.version = Some(version);
            st.synced = Some(settings.clone());
            st.settings = settings;
        }
        self.refresh();
    }

    fn read_from_mouse(&self) {
        let st = self.state.borrow();
        if st.synced.as_ref().is_some_and(|s| *s != st.settings) {
            drop(st);
            self.toast("Discarded unsaved changes");
        }
        self.worker.send(Command::Read);
    }

    fn apply(&self) {
        let mut st = self.state.borrow_mut();
        if let Err(e) = st.settings.validate(st.model) {
            drop(st);
            self.toast(&e);
            return;
        }
        let writes = plan_writes(&st.settings, st.synced.as_ref());
        if writes.is_empty() {
            return;
        }
        st.pending = Some(st.settings.clone());
        st.busy = Some("Writing settings…".into());
        drop(st);
        self.worker.send(Command::Write(writes));
        self.update_chrome();
    }
}

pub fn rgba(c: Rgb) -> gdk::RGBA {
    gdk::RGBA::new(c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0, 1.0)
}

pub fn rgb(c: &gdk::RGBA) -> Rgb {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(b(c.red()), b(c.green()), b(c.blue()))
}

pub fn color_button() -> gtk::ColorDialogButton {
    let button =
        gtk::ColorDialogButton::new(Some(gtk::ColorDialog::builder().with_alpha(false).build()));
    button.set_valign(gtk::Align::Center);
    button
}

pub fn build(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Attack Shark")
        .default_width(720)
        .default_height(760)
        .build();

    let (tx, rx) = async_channel::unbounded();
    let ctx: CtxRef = Rc::new(Ctx {
        state: RefCell::new(State::default()),
        loading: Cell::new(false),
        refreshers: RefCell::new(Vec::new()),
        worker: Worker::spawn(tx),
        window: window.clone(),
        toasts: adw::ToastOverlay::new(),
        banner: adw::Banner::new(""),
        apply: gtk::Button::builder().label("Apply").css_classes(["suggested-action"]).build(),
        read: gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Read settings from the mouse")
            .build(),
        battery_icon: gtk::Image::new(),
        battery_label: gtk::Label::new(None),
        spinner: adw::Spinner::new(),
    });
    if let Some(saved) = store::load_current() {
        ctx.state.borrow_mut().settings = saved;
    }

    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(&performance::page(&ctx), Some("dpi"), "Performance", "power-profile-performance-symbolic");
    stack.add_titled_with_icon(&buttons::page(&ctx), Some("buttons"), "Buttons", "input-mouse-symbolic");
    stack.add_titled_with_icon(&lighting::page(&ctx), Some("lighting"), "Lighting", "display-brightness-symbolic");
    stack.add_titled_with_icon(&power::page(&ctx), Some("power"), "Power", "battery-symbolic");

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(
        &adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build(),
    ));
    header.pack_start(&ctx.read);
    let battery = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    battery.append(&ctx.battery_icon);
    battery.append(&ctx.battery_label);
    battery.add_css_class("dim-label");
    header.pack_start(&battery);
    header.pack_start(&ctx.spinner);

    let menu = gio::Menu::new();
    menu.append(Some("Import Profile…"), Some("win.import"));
    menu.append(Some("Export Profile…"), Some("win.export"));
    menu.append(Some("Reset to Factory Defaults"), Some("win.defaults"));
    menu.append(Some("About"), Some("win.about"));
    header.pack_end(&gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).build());
    header.pack_end(&ctx.apply);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&ctx.banner);
    toolbar.set_content(Some(&stack));
    ctx.toasts.set_child(Some(&toolbar));
    window.set_content(Some(&ctx.toasts));

    let c = ctx.clone();
    ctx.apply.connect_clicked(move |_| c.apply());
    let c = ctx.clone();
    ctx.read.connect_clicked(move |_| c.read_from_mouse());
    add_actions(&ctx);

    let c = ctx.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = rx.recv().await {
            c.handle(event);
        }
    });

    ctx.refresh();
    window.present();
}

fn add_actions(ctx: &CtxRef) {
    let window = ctx.window();

    let import = gio::SimpleAction::new("import", None);
    let c = ctx.clone();
    import.connect_activate(move |_, _| {
        let c = c.clone();
        glib::spawn_future_local(async move {
            let dialog = profile_dialog("Import Profile");
            let Ok(file) = dialog.open_future(Some(c.window())).await else { return };
            let Some(path) = file.path() else { return };
            match store::load(&path) {
                Ok(s) => {
                    c.state.borrow_mut().settings = s;
                    c.refresh();
                    c.toast("Profile imported. Press Apply to save it to the mouse.");
                }
                Err(e) => c.toast(&e),
            }
        });
    });
    window.add_action(&import);

    let export = gio::SimpleAction::new("export", None);
    let c = ctx.clone();
    export.connect_activate(move |_, _| {
        let c = c.clone();
        glib::spawn_future_local(async move {
            let dialog = profile_dialog("Export Profile");
            dialog.set_initial_name(Some("profile.json"));
            let Ok(file) = dialog.save_future(Some(c.window())).await else { return };
            let Some(path) = file.path() else { return };
            let settings = c.state.borrow().settings.clone();
            match store::save(&path, &settings) {
                Ok(()) => c.toast("Profile exported"),
                Err(e) => c.toast(&e),
            }
        });
    });
    window.add_action(&export);

    let defaults = gio::SimpleAction::new("defaults", None);
    let c = ctx.clone();
    defaults.connect_activate(move |_, _| {
        c.edit_and_refresh(|s| *s = Settings::default());
        c.toast("Factory defaults loaded. Press Apply to save them to the mouse.");
    });
    window.add_action(&defaults);

    let about = gio::SimpleAction::new("about", None);
    let c = ctx.clone();
    about.connect_activate(move |_, _| {
        let st = c.state.borrow();
        let mut details = String::from(
            "Configure Attack Shark X11 mice on Linux. Protocol recovered from the vendor's Windows software.",
        );
        if let Some(v) = st.version {
            details.push_str(&format!("\n\nMouse firmware: {v:#06x}"));
        }
        adw::AboutDialog::builder()
            .application_name("Attack Shark")
            .application_icon("input-mouse")
            .version(env!("CARGO_PKG_VERSION"))
            .comments(details)
            .license_type(gtk::License::MitX11)
            .build()
            .present(Some(c.window()));
    });
    window.add_action(&about);
}

fn profile_dialog(title: &str) -> gtk::FileDialog {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Profiles"));
    filter.add_suffix("json");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder().title(title).filters(&filters).build();
    let dir = store::config_dir().join("profiles");
    if std::fs::create_dir_all(&dir).is_ok() {
        dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    }
    dialog
}

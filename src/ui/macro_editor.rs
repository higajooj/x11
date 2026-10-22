//! Dialog for recording and editing a button macro.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::{Rc, Weak};
use std::time::Instant;

use adw::prelude::*;
use gtk::glib;

use crate::keys;
use crate::protocol::{Macro, MacroEvent, MacroRepeat, MACRO_MAX_EVENTS, MACRO_MOUSE_KEYS};

const REPEAT_MODES: [(MacroRepeat, &str); 3] = [
    (MacroRepeat::Times, "A set number of times"),
    (MacroRepeat::UntilKeyPress, "Until any button is pressed"),
    (MacroRepeat::WhileHeld, "While the button is held"),
];
const DEFAULT_DELAY_MS: u16 = 50;

struct Editor {
    m: RefCell<Macro>,
    list: gtk::ListBox,
    status: gtk::Label,
    record: gtk::ToggleButton,
    /// Keys currently held while recording, to skip auto-repeat.
    held: RefCell<HashSet<u8>>,
    last_event: Cell<Option<Instant>>,
}

impl Editor {
    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let m = self.m.borrow();
        for (i, e) in m.events.iter().enumerate() {
            self.list.append(&self.event_row(i, e));
        }
        self.status.set_text(&format!("{} of {} events", m.events.len(), MACRO_MAX_EVENTS));
    }

    fn event_row(self: &Rc<Self>, i: usize, e: &MacroEvent) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(keys::key_name(e.key))
            .subtitle(if e.down { "Press" } else { "Release" })
            .build();
        let delay = gtk::SpinButton::with_range(1.0, 60_000.0, 10.0);
        delay.set_valign(gtk::Align::Center);
        delay.set_value(e.delay_ms as f64);
        delay.set_tooltip_text(Some("Delay after this event (ms)"));
        let remove = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Remove")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        row.add_suffix(&delay);
        row.add_suffix(&gtk::Label::builder().label("ms").css_classes(["dim-label"]).build());
        row.add_suffix(&remove);

        let weak = Rc::downgrade(self);
        delay.connect_value_changed(move |spin| {
            if let Some(ed) = weak.upgrade() {
                if let Some(e) = ed.m.borrow_mut().events.get_mut(i) {
                    e.delay_ms = spin.value() as u16;
                }
            }
        });
        let weak: Weak<Self> = Rc::downgrade(self);
        remove.connect_clicked(move |_| {
            if let Some(ed) = weak.upgrade() {
                ed.m.borrow_mut().events.remove(i);
                ed.rebuild();
            }
        });
        row
    }

    /// Append an event, giving the previous one the time elapsed since it.
    fn push(self: &Rc<Self>, key: u8, down: bool) {
        let now = Instant::now();
        {
            let mut m = self.m.borrow_mut();
            if m.events.len() >= MACRO_MAX_EVENTS {
                self.record.set_active(false);
                return;
            }
            if let (Some(prev), Some(last)) = (self.last_event.get(), m.events.last_mut()) {
                last.delay_ms = now.duration_since(prev).as_millis().clamp(1, 60_000) as u16;
            }
            m.events.push(MacroEvent { key, down, delay_ms: DEFAULT_DELAY_MS });
        }
        self.last_event.set(Some(now));
        self.rebuild();
    }
}

/// Edit `initial` (or a new macro); `done` gets the result, or `None` if cancelled.
pub fn edit(
    parent: &impl IsA<gtk::Widget>,
    initial: Option<Macro>,
    done: impl Fn(Option<Macro>) + 'static,
) {
    let m = initial.unwrap_or_else(|| Macro {
        name: "Macro".into(),
        repeat: MacroRepeat::Times,
        count: 1,
        events: Vec::new(),
    });

    let dialog = adw::Dialog::builder()
        .title("Macro")
        .content_width(480)
        .content_height(640)
        .build();
    let header = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
    header.pack_start(&cancel);
    header.pack_end(&save);

    let page = adw::PreferencesPage::new();
    let general = adw::PreferencesGroup::new();
    let name = adw::EntryRow::builder().title("Name").text(&m.name).build();
    let labels: Vec<&str> = REPEAT_MODES.iter().map(|(_, l)| *l).collect();
    let repeat = adw::ComboRow::builder()
        .title("Repeat")
        .model(&gtk::StringList::new(&labels))
        .selected(REPEAT_MODES.iter().position(|(r, _)| *r == m.repeat).unwrap_or(0) as u32)
        .build();
    let count = adw::SpinRow::builder()
        .title("Times")
        .adjustment(&gtk::Adjustment::new(m.count.max(1) as f64, 1.0, 255.0, 1.0, 10.0, 0.0))
        .sensitive(m.repeat == MacroRepeat::Times)
        .build();
    general.add(&name);
    general.add(&repeat);
    general.add(&count);
    page.add(&general);

    let record = gtk::ToggleButton::builder()
        .icon_name("media-record-symbolic")
        .tooltip_text("Record keys typed while this is on")
        .css_classes(["flat"])
        .build();
    let mouse_menu = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let add_mouse = gtk::MenuButton::builder()
        .icon_name("input-mouse-symbolic")
        .tooltip_text("Add a mouse click")
        .css_classes(["flat"])
        .popover(&gtk::Popover::builder().child(&mouse_menu).build())
        .build();
    let clear = gtk::Button::builder()
        .icon_name("edit-clear-all-symbolic")
        .tooltip_text("Remove all events")
        .css_classes(["flat"])
        .build();
    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    tools.append(&record);
    tools.append(&add_mouse);
    tools.append(&clear);
    let status = gtk::Label::new(None);
    let events = adw::PreferencesGroup::builder()
        .title("Events")
        .description("Turn on recording and type to capture keys. Mouse clicks are added from the mouse menu.")
        .header_suffix(&tools)
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list.set_placeholder(Some(
        &gtk::Label::builder().label("No events yet").margin_top(12).margin_bottom(12).css_classes(["dim-label"]).build(),
    ));
    events.add(&list);
    status.add_css_class("dim-label");
    status.set_margin_top(6);
    events.add(&status);
    page.add(&events);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&page));
    dialog.set_child(Some(&view));

    let ed = Rc::new(Editor {
        m: RefCell::new(m),
        list,
        status,
        record: record.clone(),
        held: RefCell::new(HashSet::new()),
        last_event: Cell::new(None),
    });
    ed.rebuild();

    for (key, label) in MACRO_MOUSE_KEYS {
        let item = gtk::Button::builder().label(label).css_classes(["flat"]).build();
        let ed = ed.clone();
        let menu = add_mouse.clone();
        item.connect_clicked(move |_| {
            ed.last_event.set(None);
            ed.push(key, true);
            ed.push(key, false);
            menu.popdown();
        });
        mouse_menu.append(&item);
    }

    let e = ed.clone();
    clear.connect_clicked(move |_| {
        e.m.borrow_mut().events.clear();
        e.rebuild();
    });
    let e = ed.clone();
    record.connect_toggled(move |b| {
        e.last_event.set(None);
        e.held.borrow_mut().clear();
        if b.is_active() {
            b.grab_focus();
        }
    });
    let e = ed.clone();
    name.connect_changed(move |row| e.m.borrow_mut().name = row.text().to_string());
    let e = ed.clone();
    let count_row = count.clone();
    repeat.connect_selected_notify(move |row| {
        let mode = REPEAT_MODES[row.selected() as usize].0;
        e.m.borrow_mut().repeat = mode;
        count_row.set_sensitive(mode == MacroRepeat::Times);
    });
    let e = ed.clone();
    count.connect_value_notify(move |row| e.m.borrow_mut().count = row.value() as u8);

    let keyboard = gtk::EventControllerKey::new();
    keyboard.set_propagation_phase(gtk::PropagationPhase::Capture);
    let e = ed.clone();
    keyboard.connect_key_pressed(move |_, _, keycode, _| {
        if !e.record.is_active() {
            return glib::Propagation::Proceed;
        }
        if let Some(usage) = keys::hid_from_evdev(keycode.saturating_sub(8) as u16) {
            if e.held.borrow_mut().insert(usage) {
                e.push(usage, true);
            }
        }
        glib::Propagation::Stop
    });
    let e = ed.clone();
    keyboard.connect_key_released(move |_, _, keycode, _| {
        if !e.record.is_active() {
            return;
        }
        if let Some(usage) = keys::hid_from_evdev(keycode.saturating_sub(8) as u16) {
            if e.held.borrow_mut().remove(&usage) {
                e.push(usage, false);
            }
        }
    });
    dialog.add_controller(keyboard);

    let done = Rc::new(done);
    let saved = Rc::new(Cell::new(false));
    let (d, s, e, f) = (dialog.clone(), saved.clone(), ed.clone(), done.clone());
    save.connect_clicked(move |_| {
        s.set(true);
        f(Some(e.m.borrow().clone()));
        d.close();
    });
    let d = dialog.clone();
    cancel.connect_clicked(move |_| {
        d.close();
    });
    dialog.connect_closed(move |_| {
        if !saved.get() {
            done(None);
        }
    });
    dialog.present(Some(parent));
}

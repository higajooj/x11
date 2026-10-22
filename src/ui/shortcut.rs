//! Dialog that captures a key combination for a button.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};

use crate::keys;

/// Ask for a key combination; `done` gets (HID modifier mask, HID usage), or
/// `None` if cancelled.
pub fn ask(
    parent: &impl IsA<gtk::Widget>,
    initial: Option<(u8, u8)>,
    done: impl Fn(Option<(u8, u8)>) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading("Keyboard Shortcut")
        .body("Press the key combination this button should send.")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("set", "Set")]);
    dialog.set_response_appearance("set", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("set"));

    let label = gtk::Label::builder().css_classes(["title-2"]).margin_top(12).build();
    dialog.set_extra_child(Some(&label));

    let chosen = Rc::new(Cell::new(initial.unwrap_or((0, 0))));
    let show = {
        let label = label.clone();
        let dialog = dialog.clone();
        move |(mods, usage): (u8, u8)| {
            label.set_text(&if usage == 0 && mods == 0 {
                "Waiting for keys…".to_string()
            } else {
                keys::shortcut_label(mods, usage)
            });
            dialog.set_response_enabled("set", usage != 0);
        }
    };
    show(chosen.get());

    let keys_in = gtk::EventControllerKey::new();
    keys_in.set_propagation_phase(gtk::PropagationPhase::Capture);
    let c = chosen.clone();
    keys_in.connect_key_pressed(move |_, _, keycode, state| {
        let Some(usage) = keys::hid_from_evdev(keycode.saturating_sub(8) as u16) else {
            return glib::Propagation::Stop;
        };
        let mut mods = modifiers(state);
        if let Some(bit) = keys::modifier_bit(usage) {
            // A lone modifier: show it while waiting for the real key.
            mods |= bit;
            c.set((mods, 0));
        } else {
            c.set((mods, usage));
        }
        show(c.get());
        glib::Propagation::Stop
    });
    dialog.add_controller(keys_in);

    dialog.connect_response(None, move |_, response| {
        let (mods, usage) = chosen.get();
        done((response == "set" && usage != 0).then_some((mods, usage)));
    });
    dialog.present(Some(parent));
}

fn modifiers(state: gdk::ModifierType) -> u8 {
    let mut m = 0;
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        m |= keys::MOD_CTRL;
    }
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        m |= keys::MOD_SHIFT;
    }
    if state.contains(gdk::ModifierType::ALT_MASK) {
        m |= keys::MOD_ALT;
    }
    if state.contains(gdk::ModifierType::SUPER_MASK) {
        m |= keys::MOD_SUPER;
    }
    m
}

use adw::prelude::*;

use super::{macro_editor, shortcut, CtxRef};
use crate::keys;
use crate::protocol::{Binding, Macro, BUTTON_FUNCTIONS};

/// Button slots shown until the model is known (the X11's layout).
const DEFAULT_SLOTS: [usize; 5] = [0, 1, 2, 6, 7];

const KEYBOARD_ITEM: usize = BUTTON_FUNCTIONS.len();
const MACRO_ITEM: usize = BUTTON_FUNCTIONS.len() + 1;

pub fn page(ctx: &CtxRef) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::builder()
        .title("Buttons")
        .description("The DPI button and scroll wheel can't be reassigned.")
        .build();
    page.add(&group);

    let mut items: Vec<&str> = BUTTON_FUNCTIONS.iter().map(|(_, name)| *name).collect();
    items.push("Keyboard shortcut…");
    items.push("Macro…");

    for (n, slot) in DEFAULT_SLOTS.into_iter().enumerate() {
        let row = adw::ActionRow::new();
        let dropdown = gtk::DropDown::from_strings(&items);
        dropdown.set_valign(gtk::Align::Center);
        let edit = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Edit")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        row.add_suffix(&edit);
        row.add_suffix(&dropdown);
        group.add(&row);

        let c = ctx.clone();
        dropdown.connect_selected_notify(move |dd| {
            if c.is_loading() {
                return;
            }
            choose(&c, slot, dd.selected() as usize);
        });
        let c = ctx.clone();
        edit.connect_clicked(move |_| {
            let b = c.state.borrow().settings.buttons[slot];
            choose(&c, slot, if b.is_macro() { MACRO_ITEM } else { KEYBOARD_ITEM });
        });

        ctx.on_refresh(move |c| {
            let st = c.state.borrow();
            let label = st.model.and_then(|m| m.buttons.get(n)).map_or("", |b| b.1);
            row.set_title(if label.is_empty() { "Button" } else { label });
            let b = st.settings.buttons[slot];
            let selected = match b.app_code() {
                Some(code) => BUTTON_FUNCTIONS.iter().position(|(c, _)| *c == code),
                None if b.is_keyboard() => Some(KEYBOARD_ITEM),
                None if b.is_macro() => Some(MACRO_ITEM),
                None => None,
            };
            dropdown.set_selected(selected.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
            row.set_subtitle(&describe(b, st.settings.macros[slot].as_ref()));
            edit.set_visible(b.app_code().is_none() && (b.is_keyboard() || b.is_macro()));
        });
    }
    page
}

fn describe(b: Binding, m: Option<&Macro>) -> String {
    if b.app_code().is_some() {
        String::new()
    } else if b.is_keyboard() {
        keys::shortcut_label(b.0[1], b.0[2])
    } else if b.is_macro() {
        m.map_or("No macro recorded".into(), |m| {
            format!("{} · {} events", m.name, m.events.len())
        })
    } else {
        format!("Unknown assignment {:02x} {:02x} {:02x}", b.0[0], b.0[1], b.0[2])
    }
}

fn choose(ctx: &CtxRef, slot: usize, item: usize) {
    let current = ctx.state.borrow().settings.buttons[slot];
    if let Some(&(code, _)) = BUTTON_FUNCTIONS.get(item) {
        ctx.edit_and_refresh(|s| {
            s.buttons[slot] = Binding::from_app_code(code);
            s.macros[slot] = None;
        });
    } else if item == KEYBOARD_ITEM {
        let initial = current.is_keyboard().then_some((current.0[1], current.0[2]));
        let c = ctx.clone();
        shortcut::ask(ctx.window(), initial, move |chosen| match chosen {
            Some((mods, usage)) => c.edit_and_refresh(|s| {
                s.buttons[slot] = Binding::keyboard(mods, usage);
                s.macros[slot] = None;
            }),
            None => c.refresh(),
        });
    } else if item == MACRO_ITEM {
        let initial = ctx.state.borrow().settings.macros[slot].clone();
        let c = ctx.clone();
        macro_editor::edit(ctx.window(), initial, move |result| match result {
            Some(m) => c.edit_and_refresh(|s| {
                s.buttons[slot] = Binding::macro_slot(slot);
                s.macros[slot] = Some(m);
            }),
            None => c.refresh(),
        });
    }
}

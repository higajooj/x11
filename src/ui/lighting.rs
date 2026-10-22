use adw::prelude::*;

use super::{color_button, rgb, rgba, CtxRef};
use crate::protocol::{light_mode_uses_brightness, LIGHT_MODES};

pub fn page(ctx: &CtxRef) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::builder().title("Lighting").build();

    let mode = adw::ComboRow::builder()
        .title("Effect")
        .model(&gtk::StringList::new(&LIGHT_MODES))
        .build();
    let color = color_button();
    let color_row = adw::ActionRow::builder()
        .title("Colour")
        .subtitle("Used by the static and breathing effects")
        .build();
    color_row.add_suffix(&color);
    color_row.set_activatable_widget(Some(&color));
    let brightness = adw::SpinRow::builder()
        .title("Brightness")
        .adjustment(&gtk::Adjustment::new(8.0, 1.0, 8.0, 1.0, 1.0, 0.0))
        .build();
    let speed = adw::SpinRow::builder()
        .title("Speed")
        .subtitle("Higher is faster")
        .adjustment(&gtk::Adjustment::new(6.0, 4.0, 8.0, 1.0, 1.0, 0.0))
        .build();
    group.add(&mode);
    group.add(&color_row);
    group.add(&brightness);
    group.add(&speed);
    page.add(&group);

    let c = ctx.clone();
    mode.connect_selected_notify(move |row| {
        c.edit_and_refresh(|s| s.light_mode = row.selected() as u8);
    });
    let c = ctx.clone();
    color.connect_rgba_notify(move |b| c.edit(|s| s.light_color = rgb(&b.rgba())));
    let c = ctx.clone();
    brightness.connect_value_notify(move |row| c.edit(|s| s.brightness = row.value() as u8));
    let c = ctx.clone();
    speed.connect_value_notify(move |row| c.edit(|s| s.speed = row.value() as u8));

    ctx.on_refresh(move |c| {
        let s = &c.state.borrow().settings;
        mode.set_selected(s.light_mode as u32);
        color.set_rgba(&rgba(s.light_color));
        brightness.set_value(s.brightness as f64);
        speed.set_value(s.speed as f64);
        color_row.set_sensitive(matches!(s.light_mode, 1 | 2));
        brightness.set_sensitive(light_mode_uses_brightness(s.light_mode));
        speed.set_sensitive(matches!(s.light_mode, 2 | 3 | 4 | 6));
    });
    page
}

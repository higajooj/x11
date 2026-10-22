use adw::prelude::*;

use super::{color_button, rgb, rgba, CtxRef};
use crate::protocol::{dpi_to_slider, slider_to_dpi, PollingRate, STAGES};

pub fn page(ctx: &CtxRef) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.add(&stages_group(ctx));
    page.add(&sensor_group(ctx));
    page
}

fn stages_group(ctx: &CtxRef) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("DPI Stages")
        .description(
            "The DPI button cycles through the enabled stages. The selected stage is active \
             now, and its colour shows on the mouse in the DPI lighting modes.",
        )
        .build();

    let mut first: Option<gtk::CheckButton> = None;
    for i in 0..STAGES {
        let row = adw::ActionRow::builder().title(format!("Stage {}", i + 1)).build();

        let active = gtk::CheckButton::builder().tooltip_text("Active stage").build();
        active.set_group(first.as_ref());
        first.get_or_insert_with(|| active.clone());
        row.add_prefix(&active);

        let dpi = gtk::SpinButton::with_range(50.0, 22000.0, 50.0);
        dpi.set_valign(gtk::Align::Center);
        dpi.set_tooltip_text(Some("DPI"));
        let color = color_button();
        color.set_tooltip_text(Some("Stage colour"));
        let enabled = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .tooltip_text("Include in the DPI cycle")
            .build();
        row.add_suffix(&dpi);
        row.add_suffix(&color);
        row.add_suffix(&enabled);
        group.add(&row);

        let c = ctx.clone();
        active.connect_toggled(move |b| {
            if b.is_active() {
                c.edit(|s| s.active_stage = i as u8);
            }
        });
        let c = ctx.clone();
        dpi.connect_value_changed(move |spin| {
            let max = c.state.borrow().model.map_or(320, |m| m.dpi_slider_max);
            let v = dpi_to_slider(spin.value() as u32).clamp(1, max);
            let snapped = slider_to_dpi(v) as f64;
            if spin.value() != snapped {
                // Re-enters this handler with the snapped value.
                spin.set_value(snapped);
                return;
            }
            c.edit(|s| s.dpi_slider[i] = v);
        });
        let c = ctx.clone();
        color.connect_rgba_notify(move |b| c.edit(|s| s.dpi_color[i] = rgb(&b.rgba())));
        let c = ctx.clone();
        enabled.connect_active_notify(move |sw| c.edit(|s| s.dpi_enabled[i] = sw.is_active()));

        ctx.on_refresh(move |c| {
            let st = c.state.borrow();
            let s = &st.settings;
            let (stages, max) = st.model.map_or((6, 320), |m| (m.stages, m.dpi_slider_max));
            row.set_visible(i < stages);
            dpi.set_range(50.0, slider_to_dpi(max) as f64);
            dpi.set_value(slider_to_dpi(s.dpi_slider[i].max(1)) as f64);
            color.set_rgba(&rgba(s.dpi_color[i]));
            enabled.set_active(s.dpi_enabled[i]);
            active.set_active(s.active_stage as usize == i);
        });
    }
    group
}

fn sensor_group(ctx: &CtxRef) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title("Sensor").build();

    let labels: Vec<&str> = PollingRate::ALL.iter().map(|r| r.label()).collect();
    let polling = adw::ComboRow::builder()
        .title("Polling rate")
        .subtitle("How often the mouse reports to the computer")
        .model(&gtk::StringList::new(&labels))
        .build();
    let ripple = adw::SwitchRow::builder()
        .title("Ripple control")
        .subtitle("Smooth out sensor jitter at high DPI")
        .build();
    let snap = adw::SwitchRow::builder()
        .title("Angle snapping")
        .subtitle("Straighten nearly horizontal and vertical movements")
        .build();
    let debounce = adw::SpinRow::builder()
        .title("Button debounce")
        .subtitle("Milliseconds a click must settle before it registers")
        .adjustment(&gtk::Adjustment::new(8.0, 4.0, 50.0, 2.0, 10.0, 0.0))
        .build();
    group.add(&polling);
    group.add(&ripple);
    group.add(&snap);
    group.add(&debounce);

    let c = ctx.clone();
    polling.connect_selected_notify(move |row| {
        if let Some(&rate) = PollingRate::ALL.get(row.selected() as usize) {
            c.edit(|s| s.polling = rate);
        }
    });
    let c = ctx.clone();
    ripple.connect_active_notify(move |row| c.edit(|s| s.ripple_control = row.is_active()));
    let c = ctx.clone();
    snap.connect_active_notify(move |row| c.edit(|s| s.angle_snap = row.is_active()));
    let c = ctx.clone();
    debounce.connect_value_notify(move |row| c.edit(|s| s.debounce = (row.value() / 2.0) as u8));

    ctx.on_refresh(move |c| {
        let s = &c.state.borrow().settings;
        let idx = PollingRate::ALL.iter().position(|&r| r == s.polling).unwrap_or(3);
        polling.set_selected(idx as u32);
        ripple.set_active(s.ripple_control);
        snap.set_active(s.angle_snap);
        debounce.set_value(s.debounce as f64 * 2.0);
    });
    group
}

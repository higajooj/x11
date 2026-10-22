use adw::prelude::*;

use super::CtxRef;

pub fn page(ctx: &CtxRef) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let device = adw::PreferencesGroup::builder().title("Device").build();
    let model = adw::ActionRow::builder().title("Mouse").subtitle("Not connected").build();
    let firmware = adw::ActionRow::builder().title("Firmware").subtitle("Unknown").build();
    let battery = adw::ActionRow::builder().title("Battery").subtitle("Unknown").build();
    let level = gtk::LevelBar::builder()
        .min_value(0.0)
        .max_value(100.0)
        .valign(gtk::Align::Center)
        .width_request(120)
        .build();
    battery.add_suffix(&level);
    for row in [&model, &firmware, &battery] {
        row.add_css_class("property");
        device.add(row);
    }
    page.add(&device);

    let timers = adw::PreferencesGroup::builder()
        .title("Power Saving")
        .description("Idle time before the mouse saves power")
        .build();
    let sleep = adw::SpinRow::builder()
        .title("Lights off after")
        .subtitle("Minutes")
        .digits(1)
        .adjustment(&gtk::Adjustment::new(0.5, 0.5, 30.0, 0.5, 1.0, 0.0))
        .build();
    let deep = adw::SpinRow::builder()
        .title("Deep sleep after")
        .subtitle("Minutes. Click to wake the mouse.")
        .adjustment(&gtk::Adjustment::new(10.0, 1.0, 60.0, 1.0, 5.0, 0.0))
        .build();
    timers.add(&sleep);
    timers.add(&deep);
    page.add(&timers);

    let c = ctx.clone();
    sleep.connect_value_notify(move |row| c.edit(|s| s.sleep = (row.value() * 2.0).round() as u8));
    let c = ctx.clone();
    deep.connect_value_notify(move |row| c.edit(|s| s.deep_sleep = row.value() as u8));

    ctx.on_refresh(move |c| {
        let st = c.state.borrow();
        sleep.set_value(st.settings.sleep as f64 / 2.0);
        deep.set_value(st.settings.deep_sleep as f64);
        model.set_subtitle(&match st.model {
            Some(m) => format!("{} (model {:#04x})", m.name, m.id),
            None => "Not connected".into(),
        });
        firmware.set_subtitle(&st.version.map_or("Unknown".into(), |v| format!("{v:#06x}")));
        match st.battery {
            Some((percent, charging, full)) => {
                let state = if full {
                    " · charged"
                } else if charging {
                    " · charging"
                } else {
                    ""
                };
                battery.set_subtitle(&format!("{percent}%{state}"));
                level.set_value(percent as f64);
                level.set_visible(true);
            }
            None => {
                battery.set_subtitle("Unknown");
                level.set_visible(false);
            }
        }
    });
    page
}

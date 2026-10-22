//! Profile files. The mouse can't report macros back, so the last applied
//! profile is kept on disk to restore them.

use std::fs;
use std::path::{Path, PathBuf};

use crate::protocol::{Settings, SLOTS};

pub fn config_dir() -> PathBuf {
    gtk::glib::user_config_dir().join("attack-shark")
}

fn current_path() -> PathBuf {
    config_dir().join("current.json")
}

pub fn load(path: &Path) -> Result<Settings, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut s: Settings =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    s.macros.resize(SLOTS, None);
    Ok(s)
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(settings).expect("settings serialize");
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_current() -> Option<Settings> {
    load(&current_path()).ok()
}

pub fn save_current(settings: &Settings) -> Result<(), String> {
    save(&current_path(), settings)
}

//! Keyboard key names and Linux keycode -> HID usage mapping.

/// (evdev keycode, HID usage, name)
const KEYS: &[(u16, u8, &str)] = &[
    (30, 0x04, "A"), (48, 0x05, "B"), (46, 0x06, "C"), (32, 0x07, "D"), (18, 0x08, "E"),
    (33, 0x09, "F"), (34, 0x0a, "G"), (35, 0x0b, "H"), (23, 0x0c, "I"), (36, 0x0d, "J"),
    (37, 0x0e, "K"), (38, 0x0f, "L"), (50, 0x10, "M"), (49, 0x11, "N"), (24, 0x12, "O"),
    (25, 0x13, "P"), (16, 0x14, "Q"), (19, 0x15, "R"), (31, 0x16, "S"), (20, 0x17, "T"),
    (22, 0x18, "U"), (47, 0x19, "V"), (17, 0x1a, "W"), (45, 0x1b, "X"), (21, 0x1c, "Y"),
    (44, 0x1d, "Z"),
    (2, 0x1e, "1"), (3, 0x1f, "2"), (4, 0x20, "3"), (5, 0x21, "4"), (6, 0x22, "5"),
    (7, 0x23, "6"), (8, 0x24, "7"), (9, 0x25, "8"), (10, 0x26, "9"), (11, 0x27, "0"),
    (28, 0x28, "Enter"), (1, 0x29, "Esc"), (14, 0x2a, "Backspace"), (15, 0x2b, "Tab"),
    (57, 0x2c, "Space"), (12, 0x2d, "-"), (13, 0x2e, "="), (26, 0x2f, "["), (27, 0x30, "]"),
    (43, 0x31, "\\"), (39, 0x33, ";"), (40, 0x34, "'"), (41, 0x35, "`"), (51, 0x36, ","),
    (52, 0x37, "."), (53, 0x38, "/"), (58, 0x39, "Caps Lock"),
    (59, 0x3a, "F1"), (60, 0x3b, "F2"), (61, 0x3c, "F3"), (62, 0x3d, "F4"), (63, 0x3e, "F5"),
    (64, 0x3f, "F6"), (65, 0x40, "F7"), (66, 0x41, "F8"), (67, 0x42, "F9"), (68, 0x43, "F10"),
    (87, 0x44, "F11"), (88, 0x45, "F12"),
    (99, 0x46, "Print Screen"), (70, 0x47, "Scroll Lock"), (119, 0x48, "Pause"),
    (110, 0x49, "Insert"), (102, 0x4a, "Home"), (104, 0x4b, "Page Up"), (111, 0x4c, "Delete"),
    (107, 0x4d, "End"), (109, 0x4e, "Page Down"), (106, 0x4f, "Right"), (105, 0x50, "Left"),
    (108, 0x51, "Down"), (103, 0x52, "Up"), (69, 0x53, "Num Lock"),
    (98, 0x54, "Keypad /"), (55, 0x55, "Keypad *"), (74, 0x56, "Keypad -"),
    (78, 0x57, "Keypad +"), (96, 0x58, "Keypad Enter"), (79, 0x59, "Keypad 1"),
    (80, 0x5a, "Keypad 2"), (81, 0x5b, "Keypad 3"), (75, 0x5c, "Keypad 4"),
    (76, 0x5d, "Keypad 5"), (77, 0x5e, "Keypad 6"), (71, 0x5f, "Keypad 7"),
    (72, 0x60, "Keypad 8"), (73, 0x61, "Keypad 9"), (82, 0x62, "Keypad 0"),
    (83, 0x63, "Keypad ."), (127, 0x65, "Menu"),
    (29, 0xe0, "Left Ctrl"), (42, 0xe1, "Left Shift"), (56, 0xe2, "Left Alt"),
    (125, 0xe3, "Left Super"), (97, 0xe4, "Right Ctrl"), (54, 0xe5, "Right Shift"),
    (100, 0xe6, "Right Alt"), (126, 0xe7, "Right Super"),
];

pub const MOD_CTRL: u8 = 0x01;
pub const MOD_SHIFT: u8 = 0x02;
pub const MOD_ALT: u8 = 0x04;
pub const MOD_SUPER: u8 = 0x08;

/// HID usage for a Linux evdev keycode.
pub fn hid_from_evdev(code: u16) -> Option<u8> {
    KEYS.iter().find(|k| k.0 == code).map(|k| k.1)
}

/// Modifier bit for a modifier key's HID usage.
pub fn modifier_bit(usage: u8) -> Option<u8> {
    (0xe0..=0xe7).contains(&usage).then(|| 1 << (usage - 0xe0))
}

pub fn key_name(usage: u8) -> String {
    if let Some((_, name)) = crate::protocol::MACRO_MOUSE_KEYS.iter().find(|m| m.0 == usage) {
        return (*name).to_string();
    }
    KEYS.iter()
        .find(|k| k.1 == usage)
        .map(|k| k.2.to_string())
        .unwrap_or_else(|| format!("Key {usage:#04x}"))
}

/// "Ctrl+Shift+S" style label for a modifier mask and key.
pub fn shortcut_label(modifiers: u8, usage: u8) -> String {
    let mut parts = Vec::new();
    for (bit, name) in [(0x11, "Ctrl"), (0x22, "Shift"), (0x44, "Alt"), (0x88, "Super")] {
        if modifiers & bit != 0 {
            parts.push(name.to_string());
        }
    }
    if usage != 0 {
        parts.push(key_name(usage));
    }
    if parts.is_empty() { "None".into() } else { parts.join("+") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings() {
        assert_eq!(hid_from_evdev(30), Some(0x04));
        assert_eq!(hid_from_evdev(29), Some(0xe0));
        assert_eq!(modifier_bit(0xe3), Some(MOD_SUPER));
        assert_eq!(shortcut_label(0x0a, 0x16), "Shift+Super+S");
        assert_eq!(key_name(0xf1), "Left click");
    }
}

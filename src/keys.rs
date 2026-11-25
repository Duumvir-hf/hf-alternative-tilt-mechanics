//! The two wish-tilt keys.
//!
//! Q/E stay **exactly** vanilla - they fly the ship - so the wish is edited with its own keys. The
//! game's input record only carries its own six held flags (W/S/A/D/Q/E/Shift), so an arbitrary
//! letter is not in there; the keys are polled from the OS instead, once per frame, from the
//! stabiliser hook (which only runs in battle). Names are parsed from `wish_tilt.json`.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

/// Default left/right keys: `Z` and `X`.
pub(crate) const DEFAULT_LEFT: i32 = 0x5a;
pub(crate) const DEFAULT_RIGHT: i32 = 0x58;

/// Virtual-key code for a config name: a single letter or digit (`"Z"`, `"7"`), a few punctuation
/// names (`"comma"`, `"period"`, `"leftbracket"`, ...), arrows, modifiers or `"f1"`..`"f12"`.
pub(crate) fn virtual_key(name: &str) -> Option<i32> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut chars = name.chars();
    if let (Some(only), None) = (chars.next(), chars.next()) {
        let only = only.to_ascii_uppercase();
        if only.is_ascii_alphabetic() || only.is_ascii_digit() {
            return Some(only as i32);
        }
    }
    let code = match name.to_ascii_lowercase().as_str() {
        "space" => 0x20,
        "tab" => 0x09,
        "capslock" | "caps" => 0x14,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "insert" => 0x2d,
        "delete" => 0x2e,
        "lshift" => 0xa0,
        "rshift" => 0xa1,
        "lctrl" => 0xa2,
        "rctrl" => 0xa3,
        "lalt" => 0xa4,
        "ralt" => 0xa5,
        "semicolon" => 0xba,
        "equals" => 0xbb,
        "comma" => 0xbc,
        "minus" => 0xbd,
        "period" => 0xbe,
        "slash" => 0xbf,
        "quote" => 0xde,
        "leftbracket" => 0xdb,
        "backslash" => 0xdc,
        "rightbracket" => 0xdd,
        other => {
            let number = other.strip_prefix('f')?.parse::<u8>().ok()?;
            if !(1..=12).contains(&number) {
                return None;
            }
            return Some(0x70 + i32::from(number) - 1);
        }
    };
    Some(code)
}

/// True while the key is down. The high bit of `GetAsyncKeyState` is the current state.
pub(crate) fn down(code: i32) -> bool {
    if code <= 0 {
        return false;
    }
    // SAFETY: a plain OS query with no pointers; the game thread polls it once per frame.
    (unsafe { GetAsyncKeyState(code) } as u16) & 0x8000 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_names_resolve() {
        assert_eq!(virtual_key("Z"), Some(0x5a));
        assert_eq!(virtual_key("x"), Some(0x58));
        assert_eq!(virtual_key("7"), Some(0x37));
        assert_eq!(virtual_key(" space "), Some(0x20));
        assert_eq!(virtual_key("comma"), Some(0xbc));
        assert_eq!(virtual_key("right"), Some(0x27));
        assert_eq!(virtual_key("f12"), Some(0x7b));
        assert_eq!(virtual_key("f13"), None);
        assert_eq!(virtual_key("nope"), None);
        assert_eq!(virtual_key(""), None);
        assert_eq!(virtual_key("zz"), None);
    }

    #[test]
    fn the_defaults_are_z_and_x() {
        assert_eq!(DEFAULT_LEFT, 'Z' as i32);
        assert_eq!(DEFAULT_RIGHT, 'X' as i32);
        assert!(!down(0), "a zero code is never down");
    }
}

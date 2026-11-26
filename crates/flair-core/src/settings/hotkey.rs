//! Global shortcut description (`Alt+Space`, `Ctrl+Shift+K`).
//!
//! Only parsing and validation live here; registering the shortcut with the OS
//! is done by `flair-platform`.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Windows / Super / Meta key.
    pub super_key: bool,
    /// Canonical key name (`Space`, `K`, `F12`, `Enter`…).
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("empty shortcut")]
    Empty,
    #[error("unknown key: {0}")]
    UnknownKey(String),
    #[error("a shortcut needs exactly one non-modifier key")]
    KeyCount,
    #[error("a shortcut needs at least one modifier (Ctrl, Alt, Shift, Super), except F1–F24")]
    NoModifier,
}

const NAMED_KEYS: &[(&[&str], &str)] = &[
    (&["space", "espace"], "Space"),
    (&["enter", "return", "entrée", "entree"], "Enter"),
    (&["tab"], "Tab"),
    (&["escape", "esc", "échap", "echap"], "Escape"),
    (&["backspace"], "Backspace"),
    (&["insert", "ins"], "Insert"),
    (&["delete", "del", "suppr"], "Delete"),
    (&["home"], "Home"),
    (&["end", "fin"], "End"),
    (&["pageup"], "PageUp"),
    (&["pagedown"], "PageDown"),
    (&["up"], "Up"),
    (&["down"], "Down"),
    (&["left"], "Left"),
    (&["right"], "Right"),
    (&["`", "backquote"], "Backquote"),
];

impl Hotkey {
    pub fn parse(text: &str) -> Result<Self, HotkeyError> {
        let mut hotkey = Self {
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
            key: String::new(),
        };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        if parts.iter().all(|p| p.is_empty()) {
            return Err(HotkeyError::Empty);
        }
        let mut keys = Vec::new();
        for part in parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" | "ctl" => hotkey.ctrl = true,
                "alt" | "option" => hotkey.alt = true,
                "shift" | "maj" => hotkey.shift = true,
                "super" | "win" | "windows" | "meta" | "cmd" => hotkey.super_key = true,
                "" => return Err(HotkeyError::UnknownKey(text.to_owned())),
                other => keys.push(
                    canonical_key(other).ok_or_else(|| HotkeyError::UnknownKey(part.to_owned()))?,
                ),
            }
        }
        if keys.len() != 1 {
            return Err(HotkeyError::KeyCount);
        }
        hotkey.key = keys.remove(0);
        let has_modifier = hotkey.ctrl || hotkey.alt || hotkey.shift || hotkey.super_key;
        if !has_modifier && !is_function_key(&hotkey.key) {
            return Err(HotkeyError::NoModifier);
        }
        Ok(hotkey)
    }
}

fn canonical_key(lower: &str) -> Option<String> {
    if let Some((_, name)) = NAMED_KEYS.iter().find(|(forms, _)| forms.contains(&lower)) {
        return Some((*name).to_owned());
    }
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_alphanumeric()
    {
        return Some(c.to_ascii_uppercase().to_string());
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Some(format!("F{n}"));
    }
    None
}

fn is_function_key(key: &str) -> bool {
    key.len() > 1 && key.starts_with('F') && key[1..].bytes().all(|b| b.is_ascii_digit())
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.super_key, "Super"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_shortcuts() {
        let h = Hotkey::parse("Alt+Space").expect("valid");
        assert!(h.alt && !h.ctrl);
        assert_eq!(h.key, "Space");
        assert_eq!(
            Hotkey::parse("ctrl + espace").expect("valid").to_string(),
            "Ctrl+Space"
        );
        assert_eq!(
            Hotkey::parse("Ctrl+Shift+k").expect("valid").to_string(),
            "Ctrl+Shift+K"
        );
        assert_eq!(Hotkey::parse("F12").expect("valid").key, "F12");
    }

    #[test]
    fn rejects_invalid_shortcuts() {
        assert_eq!(Hotkey::parse(""), Err(HotkeyError::Empty));
        assert_eq!(Hotkey::parse("Space"), Err(HotkeyError::NoModifier));
        assert_eq!(Hotkey::parse("Ctrl+A+B"), Err(HotkeyError::KeyCount));
        assert_eq!(Hotkey::parse("Ctrl+Alt"), Err(HotkeyError::KeyCount));
        assert!(matches!(
            Hotkey::parse("Ctrl+Foo"),
            Err(HotkeyError::UnknownKey(_))
        ));
        assert!(matches!(
            Hotkey::parse("Ctrl++"),
            Err(HotkeyError::UnknownKey(_))
        ));
    }
}

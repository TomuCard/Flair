//! The two built-in themes. All other themes are user-made.

use super::{Color, Theme};

pub const LIGHT_NAME: &str = "Flair clair";
pub const DARK_NAME: &str = "Flair sombre";

pub fn light() -> Theme {
    Theme {
        name: LIGHT_NAME.into(),
        background: Color::rgb8(0xFA, 0xFA, 0xF9),
        foreground: Color::rgb8(0x1C, 0x1B, 0x1A),
        // Flair purple (#AD50D4) darkened in OKLCH to reach 5.8:1 on the background.
        accent: Some(Color::rgb8(0x93, 0x34, 0xB9)),
    }
}

pub fn dark() -> Theme {
    Theme {
        name: DARK_NAME.into(),
        background: Color::rgb8(0x16, 0x16, 0x15),
        foreground: Color::rgb8(0xEC, 0xEB, 0xE8),
        // Flair purple (#AD50D4) lightened in OKLCH to reach 5.8:1 on the background.
        accent: Some(Color::rgb8(0xC7, 0x6A, 0xEF)),
    }
}

pub fn all() -> [Theme; 2] {
    [light(), dark()]
}

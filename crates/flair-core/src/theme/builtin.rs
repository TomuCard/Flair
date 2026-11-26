//! The two built-in themes. All other themes are user-made.

use super::{Color, Theme};

pub const LIGHT_NAME: &str = "Flair clair";
pub const DARK_NAME: &str = "Flair sombre";

pub fn light() -> Theme {
    Theme {
        name: LIGHT_NAME.into(),
        background: Color::rgb8(0xFA, 0xFA, 0xF9),
        foreground: Color::rgb8(0x1C, 0x1B, 0x1A),
        accent: Some(Color::rgb8(0xB0, 0x3A, 0x0A)),
    }
}

pub fn dark() -> Theme {
    Theme {
        name: DARK_NAME.into(),
        background: Color::rgb8(0x16, 0x16, 0x15),
        foreground: Color::rgb8(0xEC, 0xEB, 0xE8),
        accent: Some(Color::rgb8(0xFB, 0x92, 0x3C)),
    }
}

pub fn all() -> [Theme; 2] {
    [light(), dark()]
}

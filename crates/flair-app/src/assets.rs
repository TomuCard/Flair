//! Files embedded in the binary: fonts and monochrome SVG icons.

use std::sync::LazyLock;

use flair_core::result::BuiltinIcon;
use iced::widget::svg;

pub const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
pub const INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
pub const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

pub fn fonts() -> [&'static [u8]; 4] {
    [INTER_REGULAR, INTER_MEDIUM, INTER_SEMIBOLD, JETBRAINS_MONO]
}

/// Fixed color of the application icon (executable, Start menu, `.desktop`).
pub const APP_ICON_COLOR: &str = "#AD50D4";

/// Logo sources (single color, `currentColor`), for rendering outside Iced (tray).
pub const LOGO_SVG: &str = include_str!("../../../assets/icons/flair.svg");
/// Simplified logo for 16 px.
pub const LOGO_TINY_SVG: &str = include_str!("../../../assets/icons/flair-tiny.svg");

macro_rules! svg_handle {
    ($name:ident, $path:literal) => {
        static $name: LazyLock<svg::Handle> =
            LazyLock::new(|| svg::Handle::from_memory(include_bytes!($path).as_slice()));
    };
}

svg_handle!(LOGO, "../../../assets/icons/flair.svg");
svg_handle!(APP, "../../../assets/ui-icons/app-window.svg");
svg_handle!(FILE, "../../../assets/ui-icons/file.svg");
svg_handle!(FOLDER, "../../../assets/ui-icons/folder.svg");
svg_handle!(EQUAL, "../../../assets/ui-icons/equal.svg");
svg_handle!(GLOBE, "../../../assets/ui-icons/globe.svg");
svg_handle!(TERMINAL, "../../../assets/ui-icons/terminal.svg");
svg_handle!(SETTINGS, "../../../assets/ui-icons/settings.svg");
svg_handle!(LOCK, "../../../assets/ui-icons/lock.svg");
svg_handle!(MOON, "../../../assets/ui-icons/moon.svg");
svg_handle!(POWER, "../../../assets/ui-icons/power.svg");
svg_handle!(RESTART, "../../../assets/ui-icons/rotate-ccw.svg");
svg_handle!(LOG_OUT, "../../../assets/ui-icons/log-out.svg");
svg_handle!(TRASH, "../../../assets/ui-icons/trash-2.svg");

/// The Flair logo (≥ 20 px version), shown in the search field.
pub fn logo() -> svg::Handle {
    LOGO.clone()
}

/// Monochrome UI icon (Lucide).
pub fn builtin(icon: BuiltinIcon) -> svg::Handle {
    let handle = match icon {
        BuiltinIcon::App => &APP,
        BuiltinIcon::File => &FILE,
        BuiltinIcon::Folder => &FOLDER,
        BuiltinIcon::Calculator => &EQUAL,
        BuiltinIcon::Web => &GLOBE,
        BuiltinIcon::Command => &TERMINAL,
        BuiltinIcon::Settings => &SETTINGS,
        BuiltinIcon::Lock => &LOCK,
        BuiltinIcon::Sleep => &MOON,
        BuiltinIcon::Power => &POWER,
        BuiltinIcon::Restart => &RESTART,
        BuiltinIcon::LogOut => &LOG_OUT,
        BuiltinIcon::Trash => &TRASH,
    };
    (*handle).clone()
}

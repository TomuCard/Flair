//! Choice of the active theme from the configuration.

use flair_core::settings::Config;
use flair_core::settings::config::ThemeMode;
use flair_core::theme::{Theme, builtin};

/// Theme for the configured mode. Only the built-in themes for now; following
/// the system theme live and custom themes come with milestone 8.
pub fn active(config: &Config) -> Theme {
    let name = match config.theme.mode {
        ThemeMode::Light => &config.theme.light,
        ThemeMode::Dark | ThemeMode::System => &config.theme.dark,
    };
    builtin::all()
        .into_iter()
        .find(|t| &t.name == name)
        .unwrap_or_else(|| {
            tracing::warn!(theme = %name, "unknown theme, using the built-in one");
            if config.theme.mode == ThemeMode::Light {
                builtin::light()
            } else {
                builtin::dark()
            }
        })
}

//! Resident services: tray icon and global shortcut. Created on the UI thread
//! (Windows requires it: their messages are pumped by the UI event loop).

use std::sync::Arc;

use flair_core::i18n::I18n;
use flair_core::settings::config::TrayIconStyle;
use flair_core::settings::{Config, Hotkey};
use flair_core::theme::{Tokens, builtin};
use flair_platform::hotkey::{self, HotkeyError, Registration};
use flair_platform::tray::{self, Tray};

use crate::assets;
use crate::external::{self, External};

pub struct Services {
    pub tray: Option<Tray>,
    /// Kept alive only: dropping the registration unregisters the shortcut.
    _hotkey: Option<Registration>,
    pub hotkey_error: Option<HotkeyError>,
}

impl Services {
    /// No tray, no shortcut (tests, development screenshots).
    pub fn none() -> Self {
        Self {
            tray: None,
            _hotkey: None,
            hotkey_error: None,
        }
    }

    pub fn start(config: &Config, i18n: &I18n, tokens: &Tokens) -> Self {
        let (hotkey, hotkey_error) = match register_hotkey(&config.hotkey) {
            Ok(registration) => {
                tracing::info!(hotkey = %config.hotkey, "global shortcut registered");
                crate::stats::set_hotkey(config.hotkey.clone());
                (Some(registration), None)
            }
            Err(error) => {
                tracing::warn!(hotkey = %config.hotkey, "{error}");
                crate::stats::set_hotkey(error.to_string());
                (None, Some(error))
            }
        };
        let tray = match create_tray(config, i18n, tokens) {
            Ok(tray) => {
                crate::stats::set_tray(true);
                Some(tray)
            }
            Err(e) => {
                tracing::warn!("no tray icon: {e}");
                None
            }
        };
        Self {
            tray,
            _hotkey: hotkey,
            hotkey_error,
        }
    }
}

impl Services {
    /// Registers a new shortcut; on failure the old one stays and the reason
    /// is returned (never left without a shortcut).
    pub fn try_hotkey(&mut self, text: &str) -> Result<(), String> {
        let registration = register_hotkey(text).map_err(|e| e.to_string())?;
        self._hotkey = Some(registration);
        self.hotkey_error = None;
        crate::stats::set_hotkey(text.to_owned());
        tracing::info!(hotkey = text, "global shortcut changed");
        Ok(())
    }

    /// Registers a new shortcut. If it fails, the old one is kept.
    pub fn reregister_hotkey(&mut self, text: &str) {
        match register_hotkey(text) {
            Ok(registration) => {
                self._hotkey = Some(registration);
                self.hotkey_error = None;
                crate::stats::set_hotkey(text.to_owned());
                tracing::info!(hotkey = text, "global shortcut changed");
            }
            Err(e) => tracing::warn!(hotkey = text, "shortcut kept unchanged: {e}"),
        }
    }
}

fn register_hotkey(text: &str) -> Result<Registration, HotkeyError> {
    let parsed =
        Hotkey::parse(text).map_err(|e| HotkeyError::Failed(text.to_owned(), e.to_string()))?;
    hotkey::register(&parsed, Arc::new(|| external::send(External::Toggle)))
}

/// Tray icon in a single color: the Flair light/dark foreground following the
/// taskbar, or the theme accent (`theme.tray_icon`).
pub fn tray_icon(config: &Config, tokens: &Tokens) -> Result<tray::IconRgba, String> {
    let color = match config.theme.tray_icon {
        TrayIconStyle::Accent => tokens.accent,
        TrayIconStyle::Auto if flair_platform::theme::taskbar_is_light() => {
            builtin::light().foreground
        }
        TrayIconStyle::Auto => builtin::dark().foreground,
    };
    let size = tray::icon_size();
    // Below 20 px the three circles blend together: use the simplified logo.
    let svg = if size < 20 {
        assets::LOGO_TINY_SVG
    } else {
        assets::LOGO_SVG
    };
    tray::render_svg(svg, color.to_rgb8(), size)
}

fn create_tray(config: &Config, i18n: &I18n, tokens: &Tokens) -> Result<Tray, String> {
    let labels = tray::Labels {
        tooltip: i18n.t_args("tray-tooltip", &[("hotkey", &config.hotkey)]),
        show: i18n.t("tray-show"),
        reindex: i18n.t("tray-reindex"),
        open_config: i18n.t("tray-open-config"),
        check_update: i18n.t("tray-check-update"),
        autostart: i18n.t("tray-autostart"),
        stats: i18n.t("tray-stats"),
        quit: i18n.t("tray-quit"),
    };
    // Reindexing rescans the applications (and the files, milestone 6).
    let enabled = tray::Enabled {
        reindex: true,
        check_update: true,
    };
    Tray::new(
        &labels,
        enabled,
        tray_icon(config, tokens)?,
        config.autostart,
        Arc::new(|action| external::send(External::Tray(action))),
    )
}

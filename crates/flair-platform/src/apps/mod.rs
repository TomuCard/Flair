//! Installed applications: listing, icons and launching.
//!
//! - Windows: Start menu shortcuts (`.lnk`, both the common and the user
//!   menu) and Store apps (`shell:AppsFolder`), icons from the shell.
//! - Linux: `.desktop` files of the XDG data folders, Flatpak and Snap,
//!   icons from the current icon theme.

use std::path::PathBuf;

use flair_core::LaunchTarget;

use crate::image::IconRgba;

#[cfg(target_os = "linux")]
pub mod desktop_entry;
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(windows)]
use windows as imp;

/// One installed application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// Stable id, for frecency (`app:firefox.desktop`, `app:lnk:…`, `app:uwp:…`).
    pub id: String,
    /// Default display name (Linux: `Name`; Windows: name shown by the shell).
    pub name: String,
    /// Translated names by language (`fr`, `en`), when they differ.
    pub localized: Vec<(String, String)>,
    /// Other searchable names: generic names, keywords, executable name,
    /// English aliases of Windows built-in apps…
    pub alternatives: Vec<String>,
    pub target: LaunchTarget,
    pub icon: AppIcon,
    /// Can be started as administrator (Windows desktop apps).
    pub can_elevate: bool,
}

impl App {
    /// Name to show for an interface language (`fr`, `en`).
    pub fn title(&self, lang: &str) -> &str {
        self.localized
            .iter()
            .find(|(l, _)| l == lang)
            .map_or(self.name.as_str(), |(_, n)| n.as_str())
    }
}

/// Where the icon of an app comes from; loaded later, in the background.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppIcon {
    None,
    /// Icon file (Linux absolute `Icon=` path).
    File(PathBuf),
    /// Name in the freedesktop icon theme.
    Themed(String),
    /// Shell parsing name (Windows: shortcut path or `shell:AppsFolder\…`).
    Shell(String),
}

/// Lists the installed applications. Slow (disk, shell): call it off the UI
/// thread.
pub fn scan() -> Vec<App> {
    imp::scan()
}

/// Folders to watch: a change inside means the list must be scanned again.
pub fn watch_roots() -> Vec<PathBuf> {
    imp::watch_roots()
}

/// Loads an icon at `size` pixels. Slow: call it off the UI thread.
pub fn load_icon(icon: &AppIcon, size: u32) -> Option<IconRgba> {
    match icon {
        AppIcon::None => None,
        _ => imp::load_icon(icon, size),
    }
}

/// Starts an application, as administrator if `elevated` (Windows).
pub fn launch(target: &LaunchTarget, elevated: bool) -> std::io::Result<()> {
    imp::launch(target, elevated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_follows_the_language() {
        let app = App {
            id: "app:files".into(),
            name: "Files".into(),
            localized: vec![("fr".into(), "Fichiers".into())],
            alternatives: vec![],
            target: LaunchTarget::DesktopEntry("/x.desktop".into()),
            icon: AppIcon::None,
            can_elevate: false,
        };
        assert_eq!(app.title("fr"), "Fichiers");
        assert_eq!(app.title("en"), "Files");
    }
}

//! Global shortcut.
//!
//! - Windows: `global-hotkey` (hidden message window, event-driven).
//! - X11: own implementation with `x11rb`, blocking on the X connection.
//!   `global-hotkey`'s X11 backend wakes up every 50 ms to poll, which breaks
//!   the "no wake-up when hidden" requirement.
//! - Wayland: not possible this way; the XDG GlobalShortcuts portal comes with
//!   milestone 8. Meanwhile, bind a desktop shortcut to `flair --toggle`.

use std::sync::Arc;

use flair_core::settings::Hotkey;

#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

/// Called on the shortcut press (from a background thread).
pub type Callback = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    /// Wayland session: applications cannot grab keys.
    #[error("global shortcuts are not available on Wayland")]
    Unsupported,
    /// Another application already uses this shortcut.
    #[error("the shortcut {0} is already used by another application")]
    Taken(String),
    #[error("cannot register the shortcut {0}: {1}")]
    Failed(String, String),
}

/// A registered shortcut; dropping it unregisters the shortcut.
pub struct Registration {
    #[cfg(windows)]
    _inner: windows::Registration,
    #[cfg(target_os = "linux")]
    _inner: x11::Registration,
}

/// True when the session is Wayland (the window uses Wayland too).
pub fn is_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Registers on X11 even when a Wayland session is detected (tests, XWayland).
#[cfg(target_os = "linux")]
pub fn register_x11(hotkey: &Hotkey, on_press: Callback) -> Result<Registration, HotkeyError> {
    x11::register(hotkey, on_press).map(|inner| Registration { _inner: inner })
}

/// Registers `hotkey`; `on_press` runs on every press (not on key repeat).
pub fn register(hotkey: &Hotkey, on_press: Callback) -> Result<Registration, HotkeyError> {
    if is_wayland() {
        return Err(HotkeyError::Unsupported);
    }
    #[cfg(windows)]
    let inner = windows::register(hotkey, on_press)?;
    #[cfg(target_os = "linux")]
    let inner = x11::register(hotkey, on_press)?;
    Ok(Registration { _inner: inner })
}

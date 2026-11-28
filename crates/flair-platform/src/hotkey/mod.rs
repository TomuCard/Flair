//! Global shortcut.
//!
//! - Windows: `global-hotkey` (hidden message window, event-driven).
//! - X11: own implementation with `x11rb`, blocking on the X connection.
//!   `global-hotkey`'s X11 backend wakes up every 50 ms to poll, which breaks
//!   the "no wake-up when hidden" requirement.
//! - Wayland: the XDG GlobalShortcuts portal when the desktop has it;
//!   otherwise, bind a desktop shortcut to `flair --toggle`.

use std::sync::Arc;

use flair_core::settings::Hotkey;

#[cfg(target_os = "linux")]
mod portal;
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
    _inner: Inner,
}

#[cfg(target_os = "linux")]
enum Inner {
    X11(#[allow(dead_code)] x11::Registration),
    Portal(#[allow(dead_code)] portal::Registration),
}

/// True when the session is Wayland (the window uses Wayland too).
pub fn is_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Registers on X11 even when a Wayland session is detected (tests, XWayland).
#[cfg(target_os = "linux")]
pub fn register_x11(hotkey: &Hotkey, on_press: Callback) -> Result<Registration, HotkeyError> {
    x11::register(hotkey, on_press).map(|inner| Registration {
        _inner: Inner::X11(inner),
    })
}

/// Registers `hotkey`; `on_press` runs on every press (not on key repeat).
/// `description` is shown by Wayland desktops (portal).
pub fn register(
    hotkey: &Hotkey,
    on_press: Callback,
    description: &str,
) -> Result<Registration, HotkeyError> {
    #[cfg(target_os = "linux")]
    {
        if is_wayland() {
            return portal::register(hotkey, on_press, description).map(|inner| Registration {
                _inner: Inner::Portal(inner),
            });
        }
        let _ = description;
        let inner = x11::register(hotkey, on_press)?;
        Ok(Registration {
            _inner: Inner::X11(inner),
        })
    }
    #[cfg(windows)]
    {
        let _ = description;
        let inner = windows::register(hotkey, on_press)?;
        Ok(Registration { _inner: inner })
    }
}

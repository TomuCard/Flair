//! Platform-specific window settings (the only `cfg` of the UI code).

use iced::window;

#[cfg(windows)]
pub fn settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        // Not in the taskbar (nor Alt+Tab, as a tool window).
        skip_taskbar: true,
        drag_and_drop: false,
        // Native DWM shadow and rounded corners on Windows 11.
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::Round,
    }
}

#[cfg(not(windows))]
pub fn settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        application_id: "flair".into(),
        ..Default::default()
    }
}

/// Whether the window is created resizable. Flair resizes it itself as
/// results come and go, the user never does:
/// - Windows: `false`, otherwise winit adds a resize border to the
///   undecorated window; programmatic resizing works anyway.
/// - Wayland: must be `true`, because winit pins min = max size on
///   non-resizable windows, which blocks programmatic resizing.
pub const RESIZABLE: bool = cfg!(not(windows));

/// Whether resizing the window from code is reliable.
///
/// On Wayland, winit applies `request_inner_size` immediately without sending
/// `WindowEvent::Resized`, and Iced only updates its layout on that event: the
/// window would grow but its content would not. There, the transparent window
/// keeps its maximum height and only the visible frame changes size (the
/// transparent area below it is not drawn). Layer-shell (milestone 8) will
/// revisit this.
pub fn can_resize() -> bool {
    !(cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some())
}

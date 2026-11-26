//! Window placement helpers: screen under the mouse, foreground rights.

/// Work area of a screen, in physical pixels, with its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}

/// Work area of the screen under the mouse pointer. `None` when unknown
/// (Linux for now: X11 support comes later, Wayland forbids it).
pub fn cursor_work_area() -> Option<WorkArea> {
    imp::cursor_work_area()
}

/// Allows any process to take the foreground (called by a `flair --toggle`
/// client before asking the running instance to show its window).
pub fn allow_foreground() {
    imp::allow_foreground();
}

#[cfg(windows)]
mod imp {
    use super::WorkArea;
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, GetCursorPos,
    };

    pub fn cursor_work_area() -> Option<WorkArea> {
        let mut point = POINT::default();
        // SAFETY: `point` is a valid, writable POINT.
        unsafe { GetCursorPos(&mut point) }.ok()?;
        // SAFETY: plain value arguments; the returned handle may be used directly.
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `info` is a valid MONITORINFO with `cbSize` set.
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96_u32, 96_u32);
        // SAFETY: valid monitor handle and writable out parameters.
        let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
        let r = info.rcWork;
        #[allow(clippy::cast_precision_loss)]
        Some(WorkArea {
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
            scale: dpi_x as f32 / 96.0,
        })
    }

    pub fn allow_foreground() {
        // SAFETY: no pointer arguments; failure is harmless (logged nowhere).
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    }
}

#[cfg(not(windows))]
mod imp {
    use super::WorkArea;

    pub fn cursor_work_area() -> Option<WorkArea> {
        None
    }

    pub fn allow_foreground() {}
}

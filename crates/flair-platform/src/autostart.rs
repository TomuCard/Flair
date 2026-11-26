//! Launch at session start: `HKCU\...\Run` on Windows, XDG autostart on Linux.

use std::path::Path;

/// Enables or disables launching `exe --background` when the session starts.
pub fn set(enabled: bool, exe: &Path) -> std::io::Result<()> {
    imp::set(enabled, exe)
}

pub fn is_enabled() -> bool {
    imp::is_enabled()
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey,
        RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    };
    use windows::core::w;

    const RUN: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    const VALUE: windows::core::PCWSTR = w!("Flair");

    fn io(e: windows::core::Error) -> std::io::Error {
        std::io::Error::other(e)
    }

    pub fn set(enabled: bool, exe: &Path) -> std::io::Result<()> {
        let mut key = HKEY::default();
        // SAFETY: constant key path, valid out parameter.
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN, None, KEY_SET_VALUE, &mut key) }
            .ok()
            .map_err(io)?;
        let result = if enabled {
            let command = format!("\"{}\" --background", exe.display());
            let wide: Vec<u16> = std::ffi::OsStr::new(&command)
                .encode_wide()
                .chain(Some(0))
                .collect();
            // SAFETY: `wide` is NUL-terminated and outlives the call; the byte
            // view covers exactly its allocation.
            let bytes =
                unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
            // SAFETY: open key handle, valid value name and data.
            unsafe { RegSetValueExW(key, VALUE, None, REG_SZ, Some(bytes)) }.ok()
        } else {
            // SAFETY: open key handle and constant value name. A missing value is fine.
            let r = unsafe { RegDeleteValueW(key, VALUE) };
            if r.0 == 2 { Ok(()) } else { r.ok() }
        };
        // SAFETY: closing the handle opened above.
        let _ = unsafe { RegCloseKey(key) };
        result.map_err(io)
    }

    pub fn is_enabled() -> bool {
        let mut key = HKEY::default();
        // SAFETY: constant key path, valid out parameter.
        if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN, None, KEY_READ, &mut key) }.is_err() {
            return false;
        }
        // SAFETY: open key; size query only (no buffer).
        let found =
            unsafe { RegGetValueW(key, None, VALUE, RRF_RT_REG_SZ, None, None, None) }.is_ok();
        // SAFETY: closing the handle opened above.
        let _ = unsafe { RegCloseKey(key) };
        found
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::{Path, PathBuf};

    fn desktop_file() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|b| b.config_dir().join("autostart").join("flair.desktop"))
    }

    pub fn set(enabled: bool, exe: &Path) -> std::io::Result<()> {
        let path = desktop_file().ok_or_else(|| std::io::Error::other("no home directory"))?;
        if enabled {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let exec = exe.display().to_string().replace('"', "\\\"");
            std::fs::write(
                &path,
                format!(
                    "[Desktop Entry]\nType=Application\nName=Flair\nExec=\"{exec}\" --background\nIcon=flair\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
                ),
            )
        } else {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        }
    }

    pub fn is_enabled() -> bool {
        desktop_file().is_some_and(|p| p.exists())
    }
}

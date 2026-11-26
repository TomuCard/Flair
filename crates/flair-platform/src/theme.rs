//! System appearance queried for the tray icon color.

/// True when the taskbar (Windows) / panel is light. Windows reads
/// `SystemUsesLightTheme`, which differs from the apps setting. Linux has no
/// standard way yet: panels are assumed dark (milestone 8 follows the portal).
pub fn taskbar_is_light() -> bool {
    imp::taskbar_is_light()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::core::w;

    pub fn taskbar_is_light() -> bool {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: constant strings, out buffer of the announced size.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("SystemUsesLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some((&raw mut value).cast()),
                Some(&mut size),
            )
        };
        result.is_ok() && value == 1
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn taskbar_is_light() -> bool {
        false
    }
}

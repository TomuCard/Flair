//! File system details that differ between systems: hidden files, the
//! drives to index.

use std::fs::Metadata;
use std::path::PathBuf;

/// Hidden: a name starting with `.`, or (Windows) the hidden or system
/// attribute.
pub fn is_hidden(name: &str, metadata: Option<&Metadata>) -> bool {
    name.starts_with('.') || metadata.is_some_and(imp::hidden_attribute)
}

/// Folders that can be watched (inotify watches allowed per user under
/// Linux); `None` when there is no such limit (Windows watches a whole tree
/// with one handle).
pub fn watch_limit() -> Option<usize> {
    if cfg!(target_os = "linux") {
        std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
            .ok()
            .and_then(|s| s.trim().parse().ok())
    } else {
        None
    }
}

/// The other fixed drives (`D:\`, `E:\`…), for `other_drives` (Windows only:
/// under Linux, mounted disks are folders of the tree).
pub fn other_drives() -> Vec<PathBuf> {
    imp::other_drives()
}

#[cfg(windows)]
mod imp {
    use std::os::windows::fs::MetadataExt;
    use std::path::PathBuf;

    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows::core::PCWSTR;

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    const DRIVE_FIXED: u32 = 3;

    pub fn hidden_attribute(metadata: &std::fs::Metadata) -> bool {
        metadata.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
    }

    pub fn other_drives() -> Vec<PathBuf> {
        let system = std::env::var("SystemDrive")
            .unwrap_or_else(|_| "C:".into())
            .to_uppercase();
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| format!("{}:\\", char::from(b'A' + i)))
            .filter(|root| !root.starts_with(&system))
            .filter(|root| {
                let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
                // SAFETY: NUL-terminated UTF-16 string alive for the call.
                let kind = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
                kind == DRIVE_FIXED
            })
            .map(PathBuf::from)
            .collect()
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;

    pub fn hidden_attribute(_metadata: &std::fs::Metadata) -> bool {
        false
    }

    pub fn other_drives() -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_files_are_hidden() {
        assert!(is_hidden(".bashrc", None));
        assert!(!is_hidden("notes.txt", None));
    }
}

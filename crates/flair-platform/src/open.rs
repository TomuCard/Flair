//! Opening files and folders with the system's default application.

use std::path::Path;

pub fn open_path(path: &Path) -> std::io::Result<()> {
    imp::open_path(path)
}

/// Opens the folder that contains `path`, with `path` selected when the file
/// manager allows it.
pub fn reveal_path(path: &Path) -> std::io::Result<()> {
    imp::reveal_path(path)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{PCWSTR, w};

    pub fn open_path(path: &Path) -> std::io::Result<()> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string alive for the call.
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(wide.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns a value > 32 on success.
        if result.0 as isize > 32 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub fn reveal_path(path: &Path) -> std::io::Result<()> {
        use std::os::windows::process::CommandExt;
        // `explorer /select,"C:\…"`: quotes kept as typed (raw argument).
        let mut child = std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()?;
        std::thread::spawn(move || child.wait());
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    use std::process::{Command, Stdio};

    pub fn open_path(path: &Path) -> std::io::Result<()> {
        let mut child = Command::new("xdg-open")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // Reap the child without blocking the caller (avoids a zombie).
        std::thread::spawn(move || child.wait());
        Ok(())
    }

    /// File managers implementing `org.freedesktop.FileManager1` (Nautilus,
    /// Dolphin, Nemo, Thunar…) select the file; otherwise the folder opens.
    pub fn reveal_path(path: &Path) -> std::io::Result<()> {
        let uri = file_uri(path);
        let path = path.to_owned();
        std::thread::spawn(move || {
            let shown = Command::new("dbus-send")
                .args([
                    "--session",
                    "--dest=org.freedesktop.FileManager1",
                    "--type=method_call",
                    "--print-reply",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &format!("array:string:{uri}"),
                    "string:",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !shown {
                let _ = open_path(path.parent().unwrap_or(&path));
            }
        });
        Ok(())
    }

    /// `file:///home/a%20b.txt`.
    fn file_uri(path: &Path) -> String {
        use std::os::unix::ffi::OsStrExt;
        let mut uri = String::from("file://");
        for &b in path.as_os_str().as_bytes() {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
                uri.push(char::from(b));
            } else {
                uri.push_str(&format!("%{b:02X}"));
            }
        }
        uri
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn file_uris_are_escaped() {
            assert_eq!(
                super::file_uri(std::path::Path::new("/home/a b/é.txt")),
                "file:///home/a%20b/%C3%A9.txt"
            );
        }
    }
}

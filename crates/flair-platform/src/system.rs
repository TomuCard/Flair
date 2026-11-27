//! System commands: lock, sleep, shut down, restart, log out, empty the
//! trash. Confirmation is asked by the interface before the destructive ones.

use flair_core::result::SystemCommand;

/// Runs a system command. Returns once it is requested (the system acts
/// afterwards).
pub fn run(command: SystemCommand) -> std::io::Result<()> {
    imp::run(command)
}

#[cfg(windows)]
mod imp {
    use flair_core::result::SystemCommand;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
        SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Power::SetSuspendState;
    use windows::Win32::System::Shutdown::{
        EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, EWX_SHUTDOWN, ExitWindowsEx, LockWorkStation,
        SHTDN_REASON_FLAG_PLANNED,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::UI::Shell::SHEmptyRecycleBinW;
    use windows::core::PCWSTR;

    const SHERB_NOCONFIRMATION: u32 = 0x1;
    const SHERB_NOPROGRESSUI: u32 = 0x2;
    const SHERB_NOSOUND: u32 = 0x4;

    fn io(e: windows::core::Error) -> std::io::Error {
        std::io::Error::other(e.to_string())
    }

    /// Shutting down and restarting need the shutdown privilege enabled in
    /// the process token (every user has it, disabled by default).
    fn enable_shutdown_privilege() -> std::io::Result<()> {
        let mut token = HANDLE::default();
        // SAFETY: pseudo-handle of this process; `token` receives a handle
        // closed below.
        unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            )
        }
        .map_err(io)?;
        let mut luid = LUID::default();
        // SAFETY: constant privilege name, valid output pointer.
        let looked = unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid) };
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: valid token handle and privileges structure for the call.
        let adjusted = looked.and_then(|()| unsafe {
            AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None)
        });
        // SAFETY: the handle was opened above and is no longer used.
        unsafe {
            let _ = CloseHandle(token);
        }
        adjusted.map_err(io)
    }

    pub fn run(command: SystemCommand) -> std::io::Result<()> {
        match command {
            // SAFETY: no arguments.
            SystemCommand::Lock => unsafe { LockWorkStation() }.map_err(io),
            SystemCommand::Sleep => {
                // SAFETY: plain call (sleep, not hibernation).
                if unsafe { SetSuspendState(false, false, false) } {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            }
            SystemCommand::Shutdown | SystemCommand::Restart => {
                enable_shutdown_privilege()?;
                let flags = if command == SystemCommand::Shutdown {
                    EWX_SHUTDOWN | EWX_POWEROFF
                } else {
                    EWX_REBOOT
                };
                // SAFETY: plain call with constant flags.
                unsafe { ExitWindowsEx(flags, SHTDN_REASON_FLAG_PLANNED) }.map_err(io)
            }
            SystemCommand::LogOut => {
                // SAFETY: plain call with constant flags.
                unsafe { ExitWindowsEx(EWX_LOGOFF, SHTDN_REASON_FLAG_PLANNED) }.map_err(io)
            }
            SystemCommand::EmptyTrash => {
                // SAFETY: no window, all drives (null root), constant flags.
                let result = unsafe {
                    SHEmptyRecycleBinW(
                        None,
                        PCWSTR::null(),
                        SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
                    )
                };
                // An already empty bin answers with an error: not a failure.
                if let Err(e) = result {
                    tracing::debug!(error = %e, "recycle bin");
                }
                Ok(())
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use flair_core::result::SystemCommand;

    /// Through systemd-logind (`loginctl`, `systemctl`): allowed for the user
    /// of the active local session by the default polkit rules.
    fn exec(program: &str, args: &[&str]) -> std::io::Result<()> {
        let status = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "{program} {}: {status}",
                args.join(" ")
            )))
        }
    }

    /// The trash of the home folder (freedesktop.org trash specification).
    fn trash_dir() -> Option<PathBuf> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(data.join("Trash"))
    }

    pub fn empty_dir(dir: &std::path::Path) -> std::io::Result<()> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let removed = if entry.file_type().is_ok_and(|t| t.is_dir()) {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if let Err(e) = removed {
                tracing::warn!(error = %e, path = %path.display(), "cannot empty from the trash");
            }
        }
        Ok(())
    }

    pub fn run(command: SystemCommand) -> std::io::Result<()> {
        match command {
            SystemCommand::Lock => lock(),
            SystemCommand::Sleep => exec("systemctl", &["suspend"]),
            SystemCommand::Shutdown => exec("systemctl", &["poweroff"]),
            SystemCommand::Restart => exec("systemctl", &["reboot"]),
            SystemCommand::LogOut => {
                let session = std::env::var("XDG_SESSION_ID")
                    .map_err(|_| std::io::Error::other("no XDG_SESSION_ID: unknown session"))?;
                exec("loginctl", &["terminate-session", &session])
            }
            SystemCommand::EmptyTrash => {
                let Some(trash) = trash_dir() else {
                    return Ok(());
                };
                empty_dir(&trash.join("files"))?;
                empty_dir(&trash.join("info"))?;
                empty_dir(&trash.join("expunged"))?;
                let _ = std::fs::remove_file(trash.join("directorysizes"));
                Ok(())
            }
        }
    }

    fn lock() -> std::io::Result<()> {
        match std::env::var("XDG_SESSION_ID") {
            Ok(session) => exec("loginctl", &["lock-session", &session]),
            Err(_) => exec("loginctl", &["lock-session"]),
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn empties_a_folder() {
            let dir = std::env::temp_dir().join(format!("flair-trash-{}", std::process::id()));
            std::fs::create_dir_all(dir.join("files/sub")).expect("dir");
            std::fs::write(dir.join("files/a.txt"), b"x").expect("write");
            std::fs::write(dir.join("files/sub/b.txt"), b"x").expect("write");
            super::empty_dir(&dir.join("files")).expect("empty");
            assert_eq!(
                std::fs::read_dir(dir.join("files")).expect("read").count(),
                0
            );
            assert!(super::empty_dir(&dir.join("missing")).is_ok());
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

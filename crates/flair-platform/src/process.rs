//! Process helpers for updates, installation and uninstallation.

use std::io;
use std::path::Path;
use std::time::Duration;

/// Waits until process `pid` has exited (or `timeout`). Returns true if it exited.
pub fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    imp::wait_for_exit(pid, timeout)
}

/// Starts `exe` with `args`, detached: it outlives the current process.
pub fn spawn_detached(exe: &Path, args: &[&str]) -> io::Result<()> {
    imp::spawn_detached(exe, args)
}

/// Deletes `file` (and the then-empty `dir`) once the current process has
/// exited: a running Windows executable cannot delete itself.
pub fn delete_after_exit(file: &Path, dir: Option<&Path>) -> io::Result<()> {
    imp::delete_after_exit(file, dir)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS, OpenProcess,
        PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    pub fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
        // SAFETY: plain arguments; failure means the process no longer exists.
        let Ok(handle) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
            return true;
        };
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: `handle` was just opened with SYNCHRONIZE rights.
        let result = unsafe { WaitForSingleObject(handle, ms) };
        // SAFETY: closing the handle opened above.
        let _ = unsafe { CloseHandle(handle) };
        result == WAIT_OBJECT_0
    }

    pub fn spawn_detached(exe: &Path, args: &[&str]) -> std::io::Result<()> {
        Command::new(exe)
            .args(args)
            .creation_flags((DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP).0)
            .spawn()
            .map(|_| ())
    }

    pub fn delete_after_exit(file: &Path, dir: Option<&Path>) -> std::io::Result<()> {
        // `cmd` waits ~2 s (ping), then deletes the file and the folder.
        let mut script = format!(
            "ping 127.0.0.1 -n 3 >NUL & del /F /Q \"{}\"",
            file.display()
        );
        if let Some(dir) = dir {
            script.push_str(&format!(" & rmdir /Q \"{}\"", dir.display()));
        }
        Command::new("cmd.exe")
            .raw_arg(format!("/C \"{script}\""))
            .creation_flags((CREATE_NO_WINDOW | DETACHED_PROCESS).0)
            .spawn()
            .map(|_| ())
    }
}

#[cfg(not(windows))]
mod imp {
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    pub fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
        // Only used for a few seconds right after an update or installation.
        let start = Instant::now();
        let proc = format!("/proc/{pid}");
        while Path::new(&proc).exists() {
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    pub fn spawn_detached(exe: &Path, args: &[&str]) -> std::io::Result<()> {
        Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map(|_| ())
    }

    pub fn delete_after_exit(file: &Path, dir: Option<&Path>) -> std::io::Result<()> {
        // Unix can unlink a running executable.
        std::fs::remove_file(file)?;
        if let Some(dir) = dir {
            let _ = std::fs::remove_dir(dir);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_a_finished_process_returns_at_once() {
        let child = std::process::Command::new(std::env::current_exe().expect("exe"))
            .arg("--list")
            .output()
            .expect("ran");
        drop(child);
        // A pid that cannot exist.
        assert!(wait_for_exit(u32::MAX - 1, Duration::from_millis(100)));
        // Our own process never exits during the test.
        assert!(!wait_for_exit(
            std::process::id(),
            Duration::from_millis(50)
        ));
    }
}

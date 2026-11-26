//! Console output for command line uses (`flair --stats`).
//!
//! The Windows release build is a GUI program (no console window): it must
//! attach to the console of the terminal that started it to print anything.
//! Text must also be encoded the way Windows terminals expect it:
//! - to a console: UTF-16 through `WriteConsoleW` (any character works);
//! - to a pipe (`flair --stats | Out-String`): in the console output code page,
//!   which is what PowerShell and cmd decode with (usually OEM 850 or 437),
//!   not UTF-8.

/// Attaches to the parent process console, if any (no-op outside Windows).
pub fn attach_parent() {
    imp::attach_parent();
}

/// Prints a line on standard output, correctly encoded for the terminal.
pub fn println(text: &str) {
    imp::write(&format!("{text}\n"), false);
}

/// Prints a line on standard error, correctly encoded for the terminal.
pub fn eprintln(text: &str) {
    imp::write(&format!("{text}\n"), true);
}

#[cfg(windows)]
mod imp {
    use std::io::Write as _;

    use windows::Win32::Globalization::{CP_UTF8, WideCharToMultiByte};
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, CONSOLE_MODE, GetConsoleMode, GetConsoleOutputCP,
        GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, WriteConsoleW,
    };

    pub fn attach_parent() {
        // SAFETY: no pointer arguments; failing (no parent console) is fine.
        let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }

    pub fn write(text: &str, stderr: bool) {
        let which = if stderr {
            STD_ERROR_HANDLE
        } else {
            STD_OUTPUT_HANDLE
        };
        // SAFETY: constant argument; an invalid handle is handled below.
        let Ok(handle) = (unsafe { GetStdHandle(which) }) else {
            return;
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut mode = CONSOLE_MODE::default();
        // SAFETY: valid out parameter; fails when the handle is not a console.
        if unsafe { GetConsoleMode(handle, &mut mode) }.is_ok() {
            // SAFETY: `wide` is alive for the call; the written count is optional.
            let _ = unsafe { WriteConsoleW(handle, &wide, None, None) };
            return;
        }
        // Pipe or file: use the console code page (UTF-8 without a console).
        // SAFETY: no arguments.
        let code_page = unsafe { GetConsoleOutputCP() };
        let bytes = if code_page == 0 || code_page == CP_UTF8 {
            text.as_bytes().to_vec()
        } else {
            // SAFETY: size query then conversion into a buffer of that size;
            // `wide` outlives both calls.
            unsafe {
                let size = WideCharToMultiByte(code_page, 0, &wide, None, None, None);
                let mut buffer = vec![0_u8; usize::try_from(size).unwrap_or(0)];
                let written =
                    WideCharToMultiByte(code_page, 0, &wide, Some(&mut buffer), None, None);
                buffer.truncate(usize::try_from(written).unwrap_or(0));
                buffer
            }
        };
        let _ = if stderr {
            std::io::stderr().write_all(&bytes)
        } else {
            std::io::stdout().write_all(&bytes)
        };
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io::Write as _;

    pub fn attach_parent() {}

    pub fn write(text: &str, stderr: bool) {
        let _ = if stderr {
            std::io::stderr().write_all(text.as_bytes())
        } else {
            std::io::stdout().write_all(text.as_bytes())
        };
    }
}

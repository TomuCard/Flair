//! Console output for command line uses (`flair --stats`).
//!
//! The Windows release build is a GUI program (no console window): it must
//! attach to the console of the terminal that started it to print anything.

/// Attaches to the parent process console, if any (no-op outside Windows).
pub fn attach_parent() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        // SAFETY: no pointer arguments; failing (no parent console) is fine.
        let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }
}

//! Memory used by the process, for `flair --stats`.

/// Resident memory, in bytes (working set on Windows, RSS on Linux).
pub fn resident_bytes() -> Option<u64> {
    imp::resident_bytes()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    pub fn resident_bytes() -> Option<u64> {
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        // SAFETY: pseudo-handle of the current process and a valid, sized struct.
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) }.ok()?;
        Some(counters.WorkingSetSize as u64)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn resident_bytes() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib * 1024)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_some_memory() {
        let bytes = super::resident_bytes().expect("available");
        assert!(bytes > 1024 * 1024);
    }
}

//! Memory used by the process (`flair --stats`), and giving it back while
//! Flair is hidden.

/// Resident memory, in bytes (working set on Windows, RSS on Linux).
pub fn resident_bytes() -> Option<u64> {
    imp::resident_bytes()
}

/// Flair is hidden: returns freed memory to the system (`malloc_trim`
/// under Linux, the working set under Windows) and, with `efficiency`,
/// enables the efficiency mode (EcoQoS) under Windows.
pub fn release(efficiency: bool) {
    imp::release(efficiency);
}

/// Flair is shown again: the efficiency mode is turned off.
pub fn wake() {
    imp::wake();
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

    fn throttle(on: bool) {
        use windows::Win32::System::Threading::{
            PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetProcessInformation,
        };
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if on {
                PROCESS_POWER_THROTTLING_EXECUTION_SPEED
            } else {
                0
            },
        };
        // SAFETY: pseudo-handle of this process, a valid struct of the given size.
        let result = unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                (&raw const state).cast(),
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
        };
        if let Err(e) = result {
            tracing::debug!(error = %e, "efficiency mode");
        }
    }

    pub fn release(efficiency: bool) {
        use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
        if efficiency {
            throttle(true);
        }
        // Pages go back to the standby list: free for others, read back
        // cheaply (from RAM, not disk) when Flair is shown.
        // SAFETY: pseudo-handle of this process.
        if let Err(e) = unsafe { EmptyWorkingSet(GetCurrentProcess()) } {
            tracing::debug!(error = %e, "working set");
        }
    }

    pub fn wake() {
        throttle(false);
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

    pub fn release(_efficiency: bool) {
        #[cfg(target_env = "gnu")]
        // SAFETY: glibc call without pointer arguments.
        unsafe {
            libc::malloc_trim(0);
        }
    }

    pub fn wake() {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_some_memory() {
        let bytes = super::resident_bytes().expect("available");
        assert!(bytes > 1024 * 1024);
    }
}

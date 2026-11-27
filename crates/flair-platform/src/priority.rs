//! Background priority for the threads that index files: they must not slow
//! the computer down, for the CPU nor for the disk.

/// Lowers the CPU and I/O priority of the calling thread, for good.
pub fn background_thread() {
    imp::background_thread();
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
    };

    pub fn background_thread() {
        // Background mode lowers both the CPU and the I/O priority.
        // SAFETY: pseudo-handle of the calling thread.
        let result = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
        if let Err(e) = result {
            tracing::debug!(error = %e, "cannot lower the thread priority");
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    /// `ioprio_set` arguments (linux/ioprio.h): idle class, this thread.
    const IOPRIO_WHO_PROCESS: libc::c_long = 1;
    const IOPRIO_CLASS_IDLE: libc::c_long = 3;
    const IOPRIO_CLASS_SHIFT: libc::c_long = 13;

    pub fn background_thread() {
        // SAFETY: gettid has no preconditions.
        let tid = unsafe { libc::gettid() };
        // Lowest CPU priority (nice 19) for this thread only (Linux applies
        // `setpriority` to a thread id).
        #[allow(clippy::cast_sign_loss)]
        // SAFETY: plain syscall on our own thread id.
        let nice = unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, 19) };
        // SAFETY: plain syscall on our own thread id, constant arguments.
        let io = unsafe {
            libc::syscall(
                libc::SYS_ioprio_set,
                IOPRIO_WHO_PROCESS,
                libc::c_long::from(tid),
                IOPRIO_CLASS_IDLE << IOPRIO_CLASS_SHIFT,
            )
        };
        if nice != 0 || io != 0 {
            tracing::debug!("cannot fully lower the thread priority");
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn background_thread() {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn lowering_does_not_fail() {
        std::thread::spawn(super::background_thread)
            .join()
            .expect("thread");
    }
}

//! Renderer selection (`resources.renderer = auto | gpu | cpu`).
//!
//! Iced reads the `ICED_BACKEND` environment variable when it creates its
//! compositor; it has no API for it.

/// Renderer requested by the configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renderer {
    /// Iced's default order: GPU (wgpu), then software if the GPU fails.
    Auto,
    /// GPU only (wgpu).
    Gpu,
    /// Software rendering (tiny-skia): no GPU context, no video memory.
    Cpu,
}

/// Selects the renderer. Must be called at the very start of `main`, before
/// any other thread is spawned. An `ICED_BACKEND` already set by the user wins.
pub fn select(renderer: Renderer) {
    if std::env::var_os("ICED_BACKEND").is_some() {
        return;
    }
    let value = match renderer {
        // WSLg exposes no hardware Vulkan driver: wgpu hangs while creating
        // its surface (measured: no frame after 30 s), software rendering
        // shows the window in ~80 ms.
        Renderer::Auto if is_wsl() => "tiny-skia",
        Renderer::Auto => return,
        Renderer::Gpu => "wgpu",
        Renderer::Cpu => "tiny-skia",
    };
    // SAFETY: `set_var` is only unsound when other threads read or write the
    // environment concurrently. This is called from `main` before the UI,
    // logging or any worker thread is started (documented contract above).
    unsafe { std::env::set_var("ICED_BACKEND", value) };
}

/// True when running under the Windows Subsystem for Linux.
pub fn is_wsl() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WSL_DISTRO_NAME").is_some()
            || std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists())
}

/// Name of the renderer actually requested, for `flair --stats`.
pub fn active_name() -> String {
    std::env::var("ICED_BACKEND").unwrap_or_else(|_| "auto (wgpu, tiny-skia)".into())
}

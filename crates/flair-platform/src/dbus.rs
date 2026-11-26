//! The process-wide D-Bus runtime (Linux): a single-thread Tokio runtime on
//! a dedicated thread, started on first use, parked while there is nothing to
//! do. Serves the tray icon (StatusNotifierItem) and the XDG portals (file
//! dialogs; global shortcuts and appearance with milestone 8).

use tokio::runtime;

pub fn runtime() -> Result<runtime::Handle, String> {
    static RUNTIME: std::sync::OnceLock<Result<runtime::Handle, String>> =
        std::sync::OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            let rt = runtime::Builder::new_current_thread()
                .enable_all()
                .thread_name("flair-dbus")
                .build()
                .map_err(|e| e.to_string())?;
            let handle = rt.handle().clone();
            std::thread::Builder::new()
                .name("flair-dbus".into())
                // Drives the runtime forever; parks while there is nothing to do.
                .spawn(move || rt.block_on(std::future::pending::<()>()))
                .map_err(|e| e.to_string())?;
            Ok(handle)
        })
        .clone()
}

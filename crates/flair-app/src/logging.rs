//! Logs: rotating daily file in the data directory, level from `FLAIR_LOG`
//! (takes precedence) or the `log_level` setting.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Number of daily log files kept.
const MAX_LOG_FILES: usize = 7;

/// Starts logging. Keep the returned guard alive until exit: it flushes the file.
pub fn init(log_dir: &Path, level: &str) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_env("FLAIR_LOG").unwrap_or_else(|_| {
        // Quiet dependencies (wgpu is very verbose), keep Flair's own level.
        EnvFilter::new(format!("warn,sctk_adwaita=off,fontdb=error,flair={level},flair_core={level},flair_platform={level},flair_providers={level}"))
    });

    // The appender prunes old files at startup: the directory must exist.
    if let Err(e) = std::fs::create_dir_all(log_dir) {
        eprintln!(
            "flair: cannot create the log directory {}: {e}",
            log_dir.display()
        );
    }
    let file = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("flair")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir);

    let (file_layer, guard) = match file {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            (Some(layer), Some(guard))
        }
        Err(e) => {
            eprintln!(
                "flair: cannot open the log directory {}: {e}",
                log_dir.display()
            );
            (None, None)
        }
    };

    // Also log to the terminal in debug builds (there is no console in release on Windows).
    let stderr_layer = cfg!(debug_assertions)
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();
    guard
}

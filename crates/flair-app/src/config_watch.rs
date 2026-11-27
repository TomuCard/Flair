//! Hot reload: the configuration folder (`config.toml`, `themes/`,
//! `lexicon/`) is watched; a change reloads the configuration, the themes and
//! the language files. The watching thread sleeps until the system reports a
//! change (no polling); a burst of writes gives one reload.

use std::path::Path;
use std::time::Duration;

use crate::external::{self, External};

/// Quiet time before reloading (editors write a file in several steps).
const SETTLE: Duration = Duration::from_millis(300);

/// Starts watching `dir` (created if missing).
pub fn start(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let relevant = event.is_ok_and(|e| {
            !e.kind.is_access()
                && e.paths.iter().any(|p| {
                    // Temporary files of atomic writes are followed by a rename.
                    p.extension().is_some_and(|x| x == "toml")
                })
        });
        if relevant {
            let _ = tx.send(());
        }
    });
    let mut watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "cannot watch the configuration");
            return;
        }
    };
    if let Err(e) = notify::Watcher::watch(&mut watcher, dir, notify::RecursiveMode::Recursive) {
        tracing::warn!(error = %e, "cannot watch the configuration");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("config-watch".into())
        .spawn(move || {
            let _watcher = watcher;
            while rx.recv().is_ok() {
                while rx.recv_timeout(SETTLE).is_ok() {}
                tracing::debug!("configuration folder changed");
                external::send(External::Reload);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "cannot start the configuration watcher");
    }
}

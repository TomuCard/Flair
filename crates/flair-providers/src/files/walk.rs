//! Parallel scan of the roots, in background priority, building the compact
//! index directly (parents first): no path is kept per file.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};

use super::index::{Builder, NO_PARENT};
use super::rules::Rules;

/// What a scan found.
#[derive(Debug, Default)]
pub struct Scanned {
    pub builder: Builder,
    /// Folders indexed (each one watched under Linux).
    pub dirs: usize,
    /// The scan stopped early (cancelled, or index full).
    pub incomplete: bool,
}

struct Queue {
    jobs: VecDeque<(u32, PathBuf)>,
    /// Workers busy with a folder (they may add jobs).
    busy: usize,
}

/// Threads used to scan: at most half of the cores.
pub fn threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .div_euclid(2)
        .max(1)
}

/// Scans `roots` (folders that do not exist are skipped).
pub fn scan(roots: &[PathBuf], rules: &Rules, threads: usize, cancel: &AtomicBool) -> Scanned {
    let mut builder = Builder::default();
    let mut jobs = VecDeque::new();
    for root in roots {
        let Some(text) = root.to_str() else { continue };
        if root.is_dir()
            && let Some(id) = builder.push(NO_PARENT, text, true)
        {
            jobs.push_back((id, root.clone()));
        }
    }
    let dirs = jobs.len();
    let state = Mutex::new((
        Queue { jobs, busy: 0 },
        Scanned {
            builder,
            dirs,
            incomplete: false,
        },
    ));
    let wake = Condvar::new();
    std::thread::scope(|s| {
        for i in 0..threads.max(1) {
            let (state, wake) = (&state, &wake);
            let spawned = std::thread::Builder::new()
                .name(format!("files-scan-{i}"))
                .spawn_scoped(s, move || {
                    flair_platform::priority::background_thread();
                    worker(state, wake, rules, cancel);
                });
            if let Err(e) = spawned {
                tracing::warn!(error = %e, "cannot start a scan thread");
            }
        }
    });
    let Ok((_, scanned)) = state.into_inner() else {
        return Scanned {
            incomplete: true,
            ..Scanned::default()
        };
    };
    scanned
}

fn worker(state: &Mutex<(Queue, Scanned)>, wake: &Condvar, rules: &Rules, cancel: &AtomicBool) {
    loop {
        // Next folder, or the end when nothing is queued nor in progress.
        let job = {
            let Ok(mut guard) = state.lock() else { return };
            loop {
                if cancel.load(Ordering::Relaxed) {
                    guard.1.incomplete = true;
                    guard.0.jobs.clear();
                }
                if let Some(job) = guard.0.jobs.pop_front() {
                    guard.0.busy += 1;
                    break job;
                }
                if guard.0.busy == 0 {
                    wake.notify_all();
                    return;
                }
                let Ok(next) = wake.wait(guard) else { return };
                guard = next;
            }
        };
        let (id, dir) = job;
        let children = read_folder(&dir, rules);
        let Ok(mut guard) = state.lock() else { return };
        let (queue, scanned) = &mut *guard;
        for (name, is_dir) in children {
            match scanned.builder.push(id, &name, is_dir) {
                Some(child) if is_dir => {
                    scanned.dirs += 1;
                    queue.jobs.push_back((child, dir.join(&name)));
                }
                Some(_) => {}
                None => {
                    scanned.incomplete = true;
                    break;
                }
            }
        }
        queue.busy -= 1;
        drop(guard);
        wake.notify_all();
    }
}

/// The entries of one folder kept by the rules: `(name, is a folder)`.
/// Links are indexed but never followed; unreadable folders are skipped.
pub fn read_folder(dir: &Path, rules: &Rules) -> Vec<(String, bool)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        // Names that are not valid Unicode cannot be shown nor typed.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // Attributes are free under Windows (read with the folder), and only
        // needed there (hidden attribute).
        let metadata = if cfg!(windows) {
            entry.metadata().ok()
        } else {
            None
        };
        if rules.excluded(dir, &name, metadata.as_ref()) {
            continue;
        }
        out.push((name, kind.is_dir()));
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::files::index::{to_bytes, view};

    /// A small tree in a fresh temporary folder.
    pub fn tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("flair-walk-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["Documents/Projets", "node_modules/pkg", ".hidden", "Photos"] {
            std::fs::create_dir_all(root.join(dir)).expect("dir");
        }
        for file in [
            "Documents/rapport.pdf",
            "Documents/Projets/plan.txt",
            "node_modules/pkg/index.js",
            ".hidden/secret.txt",
            "Photos/vacances.jpg",
            "notes.md",
        ] {
            std::fs::write(root.join(file), b"x").expect("file");
        }
        root
    }

    #[test]
    fn scans_with_exclusions() {
        let root = tree("scan");
        let rules = Rules::new(&["node_modules".into()], false);
        let scanned = scan(
            std::slice::from_ref(&root),
            &rules,
            3,
            &AtomicBool::new(false),
        );
        assert!(!scanned.incomplete);
        let bytes = to_bytes(&scanned.builder.finish(String::new())).expect("bytes");
        let v = view(&bytes).expect("view");
        let mut paths: Vec<String> = (0..v.len() as u32)
            .map(|id| {
                v.path(id)
                    .strip_prefix(&root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                "",
                "Documents",
                "Documents/Projets",
                "Documents/Projets/plan.txt",
                "Documents/rapport.pdf",
                "Photos",
                "Photos/vacances.jpg",
                "notes.md"
            ]
        );
        assert_eq!(scanned.dirs, 4);
        // Every parent comes before its children.
        for id in 0..v.len() as u32 {
            let parent = v.parent(id);
            assert!(parent == NO_PARENT || parent < id);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cancelled_scan_stops() {
        let root = tree("cancel");
        let scanned = scan(
            std::slice::from_ref(&root),
            &Rules::new(&[], false),
            2,
            &AtomicBool::new(true),
        );
        assert!(scanned.incomplete);
        let _ = std::fs::remove_dir_all(root);
    }
}

//! Resource statistics (`flair --stats`, tray "Statistics").
//!
//! Counters are process-wide atomics so the IPC thread can answer `--stats`
//! without waking the UI up.

use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use flair_core::i18n::I18n;
use serde::{Deserialize, Serialize};

/// Every event processed by Flair (window, keyboard, mouse, shortcut, tray,
/// IPC). Must not move while Flair is hidden and nothing happens.
static WAKEUPS: AtomicU64 = AtomicU64::new(0);
/// Time between the show request and the first frame, in microseconds (0 = never).
static LAST_SHOW_US: AtomicU64 = AtomicU64::new(0);
static VISIBLE: AtomicBool = AtomicBool::new(false);
static TRAY: AtomicBool = AtomicBool::new(false);
static HOTKEY: Mutex<String> = Mutex::new(String::new());
static STARTED: OnceLock<Instant> = OnceLock::new();

pub fn mark_started(at: Instant) {
    let _ = STARTED.set(at);
}

pub fn count_wakeup() {
    WAKEUPS.fetch_add(1, Ordering::Relaxed);
}

pub fn wakeups() -> u64 {
    WAKEUPS.load(Ordering::Relaxed)
}

pub fn record_show(duration: std::time::Duration) {
    let us = u64::try_from(duration.as_micros())
        .unwrap_or(u64::MAX)
        .max(1);
    LAST_SHOW_US.store(us, Ordering::Relaxed);
}

pub fn set_visible(visible: bool) {
    VISIBLE.store(visible, Ordering::Relaxed);
}

pub fn set_tray(ok: bool) {
    TRAY.store(ok, Ordering::Relaxed);
}

pub fn set_hotkey(status: String) {
    if let Ok(mut h) = HOTKEY.lock() {
        *h = status;
    }
}

/// Everything `flair --stats` reports. Sent as JSON over IPC.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub version: String,
    pub uptime_s: u64,
    pub ram_bytes: Option<u64>,
    pub wakeups: u64,
    pub last_show_ms: Option<f64>,
    pub renderer: String,
    pub visible: bool,
    pub hotkey: String,
    pub tray: bool,
    /// File index (milestone 6).
    pub index_file_bytes: Option<u64>,
    pub index_ram_bytes: Option<u64>,
    pub index_entries: Option<u64>,
    pub watched_folders: Option<u64>,
}

pub fn snapshot() -> Snapshot {
    let last_show = LAST_SHOW_US.load(Ordering::Relaxed);
    #[allow(clippy::cast_precision_loss)]
    Snapshot {
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_s: STARTED.get().map_or(0, |s| s.elapsed().as_secs()),
        ram_bytes: flair_platform::memory::resident_bytes(),
        wakeups: wakeups(),
        last_show_ms: (last_show > 0).then(|| last_show as f64 / 1000.0),
        renderer: flair_platform::renderer::active_name(),
        visible: VISIBLE.load(Ordering::Relaxed),
        hotkey: HOTKEY.lock().map(|h| h.clone()).unwrap_or_default(),
        tray: TRAY.load(Ordering::Relaxed),
        ..Snapshot::default()
    }
}

#[allow(clippy::cast_precision_loss)]
fn bytes(n: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    format!("{:.1} Mio", n as f64 / MIB)
}

fn duration(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}h{m:02}")
    } else if m > 0 {
        format!("{m} min {s:02} s")
    } else {
        format!("{s} s")
    }
}

/// Labelled lines, in the interface language.
pub fn lines(s: &Snapshot, i18n: &I18n) -> Vec<(String, String)> {
    let none = || i18n.t("stats-none");
    let opt = |v: Option<String>| v.unwrap_or_else(none);
    let yes_no = |b: bool| i18n.t(if b { "stats-yes" } else { "stats-no" });
    vec![
        (i18n.t("stats-version"), s.version.clone()),
        (i18n.t("stats-uptime"), duration(s.uptime_s)),
        (i18n.t("stats-ram"), opt(s.ram_bytes.map(bytes))),
        (
            i18n.t("stats-index-size"),
            match (s.index_file_bytes, s.index_ram_bytes) {
                (None, None) => none(),
                (f, r) => format!("{} / {}", opt(f.map(bytes)), opt(r.map(bytes))),
            },
        ),
        (
            i18n.t("stats-index-entries"),
            opt(s.index_entries.map(|n| n.to_string())),
        ),
        (
            i18n.t("stats-watched"),
            opt(s.watched_folders.map(|n| n.to_string())),
        ),
        (i18n.t("stats-wakeups"), s.wakeups.to_string()),
        (
            i18n.t("stats-last-show"),
            opt(s.last_show_ms.map(|ms| format!("{ms:.1} ms"))),
        ),
        (i18n.t("stats-renderer"), s.renderer.clone()),
        (i18n.t("stats-hotkey"), s.hotkey.clone()),
        (i18n.t("stats-tray"), yes_no(s.tray)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use flair_core::i18n::Lang;

    #[test]
    fn snapshot_round_trips_and_tolerates_unknown_fields() {
        let s = snapshot();
        let json = serde_json::to_string(&s).expect("serializable");
        let back: Snapshot = serde_json::from_str(&json).expect("parsable");
        assert_eq!(back.version, s.version);
        let future: Snapshot =
            serde_json::from_str(r#"{"wakeups": 3, "something_new": true}"#).expect("tolerant");
        assert_eq!(future.wakeups, 3);
    }

    #[test]
    fn formats_lines_in_both_languages() {
        let s = Snapshot {
            ram_bytes: Some(24 * 1024 * 1024),
            last_show_ms: Some(18.24),
            uptime_s: 3725,
            ..Snapshot::default()
        };
        let fr = lines(&s, &I18n::new(Lang::Fr));
        assert!(fr.contains(&("Mémoire utilisée".into(), "24.0 Mio".into())));
        assert!(fr.contains(&("Dernier affichage".into(), "18.2 ms".into())));
        assert!(fr.contains(&("Lancé depuis".into(), "1h02".into())));
        assert!(fr.contains(&("Entrées de l'index".into(), "—".into())));
        let en = lines(&s, &I18n::new(Lang::En));
        assert_eq!(en.len(), fr.len());
    }
}

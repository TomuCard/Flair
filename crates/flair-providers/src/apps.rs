//! Applications provider: an in-memory index of the installed apps, scanned
//! in the background, kept up to date by watching the folders they come
//! from, with icons loaded lazily (and cached on disk under Windows).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use flair_core::frecency::Frecency;
use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem,
};
use flair_platform::apps::{self, App, AppIcon};
use flair_platform::image::IconRgba;

/// Icons are loaded at this size (24 px in the list, up to 200 % scaling).
pub const ICON_PIXELS: u32 = 48;
/// Changes in the watched folders are grouped: the scan waits for this
/// much quiet (installers write many files).
const SETTLE: Duration = Duration::from_secs(2);

/// Interface language and texts used in results.
#[derive(Debug, Clone)]
pub struct Texts {
    /// `fr` or `en`: picks the displayed name of apps.
    pub lang: String,
    /// Subtitle of app results (`Application`).
    pub subtitle: String,
}

struct Entry {
    app: App,
    icon: IconSource,
    /// Names other than the title, searched without highlighting.
    alternatives: Vec<String>,
}

#[derive(Default)]
struct Index {
    entries: Vec<Entry>,
    scanned_at: Option<Instant>,
}

struct Shared {
    index: RwLock<Index>,
    texts: RwLock<Texts>,
    frecency: Arc<Mutex<Frecency>>,
    /// Disk cache of icons (Windows shell icons are slow to extract).
    icon_cache: Option<PathBuf>,
    scanning: AtomicBool,
    /// A scan was requested while another one was running.
    rescan: AtomicBool,
}

/// The applications provider. Cheap to clone (shared state).
#[derive(Clone)]
pub struct AppsProvider {
    shared: Arc<Shared>,
}

impl AppsProvider {
    pub fn new(texts: Texts, frecency: Arc<Mutex<Frecency>>, icon_cache: Option<PathBuf>) -> Self {
        Self {
            shared: Arc::new(Shared {
                index: RwLock::new(Index::default()),
                texts: RwLock::new(texts),
                frecency,
                icon_cache,
                scanning: AtomicBool::new(false),
                rescan: AtomicBool::new(false),
            }),
        }
    }

    /// Changes the interface language (configuration reloaded).
    pub fn set_texts(&self, texts: Texts) {
        if let Ok(mut t) = self.shared.texts.write() {
            *t = texts;
        }
        self.rebuild_names();
    }

    /// Number of indexed apps.
    pub fn len(&self) -> usize {
        self.shared.index.read().map_or(0, |i| i.entries.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Time since the last completed scan, if any.
    pub fn age(&self) -> Option<Duration> {
        self.shared
            .index
            .read()
            .ok()
            .and_then(|i| i.scanned_at)
            .map(|t| t.elapsed())
    }

    /// Watches the app folders and scans again after changes. The watcher
    /// sleeps until the OS reports a change: no periodic wake-up.
    pub fn watch(&self) {
        let roots: Vec<PathBuf> = apps::watch_roots()
            .into_iter()
            .filter(|r| r.is_dir())
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|e| !e.kind.is_access()) {
                let _ = tx.send(());
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "cannot watch the application folders");
                return;
            }
        };
        for root in &roots {
            if let Err(e) =
                notify::Watcher::watch(&mut watcher, root, notify::RecursiveMode::Recursive)
            {
                tracing::warn!(error = %e, root = %root.display(), "cannot watch");
            }
        }
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("apps-watch".into())
            .spawn(move || {
                // Owned by the thread: watching stops if the thread ends.
                let _watcher = watcher;
                while rx.recv().is_ok() {
                    // Wait until changes settle, then scan once.
                    while rx.recv_timeout(SETTLE).is_ok() {}
                    tracing::debug!("application folders changed");
                    this.refresh();
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot start the application watcher");
        }
    }

    /// Scans in the background (returns at once).
    fn start_scan(&self) {
        if self.shared.scanning.swap(true, Ordering::AcqRel) {
            self.shared.rescan.store(true, Ordering::Release);
            return;
        }
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("apps-scan".into())
            .spawn(move || {
                loop {
                    this.scan_now();
                    if !this.shared.rescan.swap(false, Ordering::AcqRel) {
                        break;
                    }
                }
                this.shared.scanning.store(false, Ordering::Release);
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot start the application scan");
            self.shared.scanning.store(false, Ordering::Release);
        }
    }

    /// Scans, swaps the index, then loads the missing icons, on the calling
    /// thread (development screenshots; otherwise use `refresh`).
    pub fn scan_blocking(&self) {
        self.scan_now();
    }

    /// Scans, swaps the index, then loads the missing icons.
    fn scan_now(&self) {
        let started = Instant::now();
        let found = apps::scan();
        let lang = self.lang();
        // Icons already loaded for the same source are kept.
        let mut known: HashMap<AppIcon, IconSource> = HashMap::new();
        if let Ok(index) = self.shared.index.read() {
            for e in &index.entries {
                if matches!(e.icon, IconSource::Rgba { .. }) {
                    known.insert(e.app.icon.clone(), e.icon.clone());
                }
            }
        }
        let entries: Vec<Entry> = found
            .into_iter()
            .map(|app| Entry {
                icon: known
                    .get(&app.icon)
                    .cloned()
                    .unwrap_or(IconSource::Builtin(BuiltinIcon::App)),
                alternatives: alternatives(&app, &lang),
                app,
            })
            .collect();
        let missing: Vec<AppIcon> = entries
            .iter()
            .filter(|e| !matches!(e.icon, IconSource::Rgba { .. }) && e.app.icon != AppIcon::None)
            .map(|e| e.app.icon.clone())
            .collect();
        let count = entries.len();
        if let Ok(mut index) = self.shared.index.write() {
            index.entries = entries;
            index.scanned_at = Some(Instant::now());
        }
        tracing::info!(
            count,
            elapsed_ms = started.elapsed().as_millis(),
            "applications indexed"
        );

        let icons_started = Instant::now();
        let mut loaded = 0;
        for icon in missing {
            let Some(pixels) = self.icon(&icon) else {
                continue;
            };
            let source = IconSource::Rgba {
                width: pixels.width,
                height: pixels.height,
                pixels: pixels.rgba.into(),
            };
            loaded += 1;
            if let Ok(mut index) = self.shared.index.write() {
                for e in index.entries.iter_mut().filter(|e| e.app.icon == icon) {
                    e.icon = source.clone();
                }
            }
        }
        tracing::info!(
            loaded,
            elapsed_ms = icons_started.elapsed().as_millis(),
            "application icons loaded"
        );
    }

    /// Loads an icon, through the disk cache for shell icons.
    fn icon(&self, icon: &AppIcon) -> Option<IconRgba> {
        let cache_file = match (icon, &self.shared.icon_cache) {
            (AppIcon::Shell(name), Some(dir)) => {
                Some(dir.join(format!("{:016x}.png", cache_key(name))))
            }
            _ => None,
        };
        if let Some(file) = &cache_file
            && let Ok(bytes) = std::fs::read(file)
            && let Some(pixels) = flair_platform::image::decode_png(&bytes)
        {
            return Some(pixels);
        }
        let pixels = apps::load_icon(icon, ICON_PIXELS)?;
        if let Some(file) = &cache_file
            && let Some(bytes) = flair_platform::image::encode_png(&pixels)
        {
            let saved = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(file, bytes));
            if let Err(e) = saved {
                tracing::debug!(error = %e, "cannot cache an icon");
            }
        }
        Some(pixels)
    }

    fn lang(&self) -> String {
        self.shared
            .texts
            .read()
            .map(|t| t.lang.clone())
            .unwrap_or_default()
    }

    fn rebuild_names(&self) {
        let lang = self.lang();
        if let Ok(mut index) = self.shared.index.write() {
            for e in &mut index.entries {
                e.alternatives = alternatives(&e.app, &lang);
            }
        }
    }
}

/// Every searchable name of an app except its title in `lang`.
fn alternatives(app: &App, lang: &str) -> Vec<String> {
    let title = app.title(lang);
    std::iter::once(&app.name)
        .chain(app.localized.iter().map(|(_, n)| n))
        .chain(&app.alternatives)
        .filter(|n| n.as_str() != title)
        .cloned()
        .collect()
}

/// Cache key of a shell icon: its name, plus the modification time of the
/// shortcut when it is a file (a changed shortcut gets a new icon).
fn cache_key(name: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    name.hash(&mut hasher);
    ICON_PIXELS.hash(&mut hasher);
    if let Ok(modified) = std::fs::metadata(Path::new(name)).and_then(|m| m.modified()) {
        modified.hash(&mut hasher);
    }
    hasher.finish()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Provider for AppsProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::App
    }

    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem> {
        if q.is_empty() || !q.accepts(ProviderKind::App) {
            return Vec::new();
        }
        let Ok(texts) = self.shared.texts.read().map(|t| t.clone()) else {
            return Vec::new();
        };
        let Ok(index) = self.shared.index.read() else {
            return Vec::new();
        };
        let prepared = Prepared::new(&q.text);
        let mut ranker = Ranker::new();
        let frecency = self.shared.frecency.lock().ok();
        let now = now();
        let mut out = Vec::new();
        for (i, e) in index.entries.iter().enumerate() {
            if i % 64 == 0 && cancel.is_cancelled() {
                return Vec::new();
            }
            let title = e.app.title(&texts.lang);
            let Some(m) =
                ranker.match_names(&prepared, title, e.alternatives.iter().map(String::as_str))
            else {
                continue;
            };
            let bonus = frecency.as_ref().map_or(0.0, |f| f.bonus(&e.app.id, now));
            let mut actions = vec![Action::Launch {
                target: e.app.target.clone(),
            }];
            if e.app.can_elevate {
                actions.push(Action::LaunchAsAdmin {
                    target: e.app.target.clone(),
                });
            }
            out.push(ResultItem {
                id: e.app.id.clone(),
                title: title.to_owned(),
                subtitle: Some(texts.subtitle.clone()),
                icon: e.icon.clone(),
                score: m.score + ProviderKind::App.rank_bonus() + bonus,
                match_indices: m.indices,
                actions,
                provider: ProviderKind::App,
            });
        }
        out
    }

    fn refresh(&self) {
        self.start_scan();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flair_core::LaunchTarget;

    fn app(id: &str, name: &str, fr: Option<&str>, alternatives: &[&str]) -> App {
        App {
            id: id.into(),
            name: name.into(),
            localized: fr
                .map(|f| ("fr".to_owned(), f.to_owned()))
                .into_iter()
                .collect(),
            alternatives: alternatives.iter().map(|a| (*a).to_owned()).collect(),
            target: LaunchTarget::DesktopEntry(format!("/{id}.desktop").into()),
            icon: AppIcon::None,
            can_elevate: false,
        }
    }

    fn provider(apps: Vec<App>, lang: &str, frecency: Frecency) -> AppsProvider {
        let p = AppsProvider::new(
            Texts {
                lang: lang.into(),
                subtitle: "Application".into(),
            },
            Arc::new(Mutex::new(frecency)),
            None,
        );
        if let Ok(mut index) = p.shared.index.write() {
            index.entries = apps
                .into_iter()
                .map(|app| Entry {
                    icon: IconSource::Builtin(BuiltinIcon::App),
                    alternatives: alternatives(&app, lang),
                    app,
                })
                .collect();
        }
        p
    }

    fn titles(p: &AppsProvider, text: &str) -> Vec<String> {
        let mut r = p.query(&Query::parse(text), &CancellationToken::new());
        r.sort_by(|a, b| b.score.total_cmp(&a.score));
        r.into_iter().map(|i| i.title).collect()
    }

    fn sample() -> Vec<App> {
        vec![
            app("code", "Visual Studio Code", None, &["code"]),
            app("vscodium", "VSCodium", None, &[]),
            app(
                "files",
                "Files",
                Some("Fichiers"),
                &["File Manager", "nautilus"],
            ),
            app("firefox", "Firefox", None, &["Web Browser"]),
            app("redist", "Microsoft Visual C++ Redistributable", None, &[]),
        ]
    }

    #[test]
    fn vsc_finds_visual_studio_code_first() {
        let p = provider(sample(), "fr", Frecency::in_memory());
        // `VSCodium` starts with `vsc`: both come before weaker matches.
        let found = titles(&p, "vsc");
        assert_eq!(found[..2], ["VSCodium", "Visual Studio Code"]);
        assert_eq!(found[2], "Microsoft Visual C++ Redistributable");
        let without = provider(
            sample()
                .into_iter()
                .filter(|a| a.id != "vscodium")
                .collect(),
            "fr",
            Frecency::in_memory(),
        );
        assert_eq!(titles(&without, "vsc")[0], "Visual Studio Code");
    }

    #[test]
    fn names_in_both_languages_are_searchable() {
        let p = provider(sample(), "fr", Frecency::in_memory());
        // The French title is displayed whatever the language typed.
        assert_eq!(titles(&p, "fichiers"), ["Fichiers"]);
        assert_eq!(
            titles(&p, "files").first().map(String::as_str),
            Some("Fichiers")
        );
        assert_eq!(titles(&p, "nautilus"), ["Fichiers"]);
        let en = provider(sample(), "en", Frecency::in_memory());
        assert_eq!(titles(&en, "fichiers"), ["Files"]);
    }

    #[test]
    fn frecency_lifts_used_apps() {
        let mut f = Frecency::in_memory();
        for _ in 0..10 {
            f.record("code", now());
        }
        let p = provider(sample(), "fr", f);
        assert_eq!(
            titles(&p, "vsc").first().map(String::as_str),
            Some("Visual Studio Code")
        );
    }

    #[test]
    fn filters_and_cancellation() {
        let p = provider(sample(), "fr", Frecency::in_memory());
        assert!(titles(&p, "= fire").is_empty());
        assert!(titles(&p, "").is_empty());
        let token = CancellationToken::new();
        token.cancel();
        assert!(p.query(&Query::parse("fire"), &token).is_empty());
    }

    #[test]
    fn elevation_is_a_secondary_action() {
        let mut apps = sample();
        apps[3].can_elevate = true;
        let p = provider(apps, "fr", Frecency::in_memory());
        let r = p.query(&Query::parse("firefox"), &CancellationToken::new());
        assert!(matches!(
            r[0].secondary_action(),
            Some(Action::LaunchAsAdmin { .. })
        ));
    }
}

//! Files provider: a compact index of the files under the configured roots,
//! saved on disk and searched straight from the mapped file, kept up to date
//! by watching the folders (changes grouped), in background priority.
//!
//! - Start: the saved index is usable at once; a background scan then
//!   catches up with what changed while Flair was not running.
//! - Changes between two scans go to a small in-memory overlay (removed
//!   entries, added paths). A large overlay triggers a new scan, which writes
//!   a new index file.
//! - On battery (`pause_on_battery`), watching events are kept aside and
//!   scans postponed; they are caught up when the window is shown on mains.
//! - Paths typed (`~/Doc`, `C:\Us`) list the folder directly.

pub mod index;
pub mod paths;
pub mod rules;
pub mod walk;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use flair_core::frecency::Frecency;
use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem,
};
use flair_platform::mmap::MappedFile;
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, NoCache};

use index::{ArchivedIndexData, Needle};
use rules::Rules;

/// Changes are grouped: one wake-up for a burst (a build writing 10 000
/// files), after this much quiet.
const DEBOUNCE: Duration = Duration::from_millis(1500);
/// Beyond this many changed paths at once, a new scan is cheaper.
const MAX_BATCH: usize = 5000;
/// Beyond this many overlay entries, a new scan writes a fresh index.
const MAX_OVERLAY: usize = 20_000;
/// A new folder bigger than this is indexed by a full scan instead.
const MAX_NEW_TREE: usize = 50_000;
/// Changes kept aside on battery before giving up for a full scan.
const MAX_PENDING: usize = 10_000;
/// Linux, too many folders to watch them all: the unwatched ones are
/// rescanned when the window is shown, at most this often.
const UNWATCHED_RESCAN: Duration = Duration::from_secs(15 * 60);
/// Candidates kept by the fast scan before the real ranking.
const CANDIDATES: usize = 64;
/// Results returned (the list shows fewer).
const MAX_ROWS: usize = 20;
/// Icons are loaded at this size (24 px in the list, up to 200 % scaling).
const ICON_PIXELS: u32 = 48;
/// Icons kept (one per file type mostly).
const MAX_ICONS: usize = 512;

/// What the provider needs from the configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// Expanded, without nested duplicates.
    pub roots: Vec<PathBuf>,
    pub rules: Rules,
    pub pause_on_battery: bool,
}

impl Settings {
    /// From the configured values: `~` expanded, the other drives added.
    pub fn new(
        enabled: bool,
        roots: &[String],
        exclude: &[String],
        include_hidden: bool,
        other_drives: bool,
        pause_on_battery: bool,
        home: Option<&Path>,
    ) -> Self {
        let mut all: Vec<PathBuf> = roots
            .iter()
            .filter_map(|r| paths::expand(r.trim(), home))
            .chain(if other_drives {
                flair_platform::fs::other_drives()
            } else {
                Vec::new()
            })
            .collect();
        all.sort_by_key(|p| p.components().count());
        let mut kept: Vec<PathBuf> = Vec::new();
        for root in all {
            if !kept.iter().any(|k| root.starts_with(k)) {
                kept.push(root);
            }
        }
        Self {
            enabled,
            roots: kept,
            rules: Rules::new(exclude, include_hidden),
            pause_on_battery,
        }
    }

    /// What the saved index depends on.
    fn fingerprint(&self) -> String {
        format!("{:?}|{:?}", self.roots, self.rules)
    }
}

/// Figures for `flair --stats`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileStats {
    pub entries: u64,
    pub file_bytes: u64,
    /// Memory held besides the mapped file (overlay, unsaved index).
    pub ram_bytes: u64,
    pub watched: u64,
    /// File change batches processed (wake-ups caused by the file system).
    pub events: u64,
}

/// Where the current index lives.
enum Backing {
    Mapped(MappedFile),
    Memory(rkyv::util::AlignedVec),
}

impl Backing {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Mapped(m) => m.bytes(),
            Self::Memory(v) => v,
        }
    }
}

/// Changes since the last scan.
#[derive(Default)]
struct Overlay {
    /// Entries of the index that no longer exist (with their content).
    removed: HashSet<u32>,
    added: Vec<Added>,
}

struct Added {
    path: PathBuf,
    name: String,
    dir: bool,
}

impl Overlay {
    fn len(&self) -> usize {
        self.removed.len() + self.added.len()
    }

    /// The entry or one of its folders was removed.
    fn hides(&self, view: &ArchivedIndexData, id: u32) -> bool {
        if self.removed.is_empty() {
            return false;
        }
        let mut current = id;
        let mut steps = 0;
        while current != index::NO_PARENT && steps < 4096 {
            if self.removed.contains(&current) {
                return true;
            }
            current = view.parent(current);
            steps += 1;
        }
        false
    }

    fn bytes(&self) -> usize {
        self.removed.len() * 8
            + self
                .added
                .iter()
                .map(|a| a.path.as_os_str().len() + a.name.len() + 48)
                .sum::<usize>()
    }
}

#[derive(Default)]
struct Pending {
    full: bool,
    paths: HashSet<PathBuf>,
}

type Watcher = Debouncer<RecommendedWatcher, NoCache>;

struct Shared {
    dir: PathBuf,
    home: Option<PathBuf>,
    frecency: Arc<Mutex<Frecency>>,
    settings: RwLock<Settings>,
    index: RwLock<Option<Arc<Backing>>>,
    file_bytes: AtomicU64,
    overlay: RwLock<Overlay>,
    scanning: AtomicBool,
    rescan: AtomicBool,
    cancel_scan: AtomicBool,
    /// Paths changed while a scan runs, replayed on its result.
    changed_during_scan: Mutex<Vec<PathBuf>>,
    pending: Mutex<Pending>,
    watcher: Mutex<Option<Watcher>>,
    watching: AtomicBool,
    watched: AtomicUsize,
    partial_watch: AtomicBool,
    last_scan: Mutex<Option<Instant>>,
    events: AtomicU64,
    /// Icons by type (`None`: being loaded, or none).
    icons: Mutex<std::collections::HashMap<String, Option<IconSource>>>,
    /// Requests to the icon loading thread, started on first use.
    icon_requests: Mutex<Option<std::sync::mpsc::Sender<(String, PathBuf, bool)>>>,
}

/// The files provider. Cheap to clone (shared state).
#[derive(Clone)]
pub struct FilesProvider {
    shared: Arc<Shared>,
}

impl FilesProvider {
    /// `dir`: folder of the index files.
    pub fn new(
        dir: PathBuf,
        home: Option<PathBuf>,
        frecency: Arc<Mutex<Frecency>>,
        settings: Settings,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                dir,
                home,
                frecency,
                settings: RwLock::new(settings),
                index: RwLock::new(None),
                file_bytes: AtomicU64::new(0),
                overlay: RwLock::new(Overlay::default()),
                scanning: AtomicBool::new(false),
                rescan: AtomicBool::new(false),
                cancel_scan: AtomicBool::new(false),
                changed_during_scan: Mutex::new(Vec::new()),
                pending: Mutex::new(Pending::default()),
                watcher: Mutex::new(None),
                watching: AtomicBool::new(false),
                watched: AtomicUsize::new(0),
                partial_watch: AtomicBool::new(false),
                last_scan: Mutex::new(None),
                events: AtomicU64::new(0),
                icons: Mutex::new(std::collections::HashMap::new()),
                icon_requests: Mutex::new(None),
            }),
        }
    }

    fn settings(&self) -> Settings {
        self.shared
            .settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// Opens the saved index, then watches and catches up in the background.
    /// Without `watch` (one-off runs like development screenshots), a missing
    /// index is built at once on the calling thread.
    pub fn start(&self, watch: bool) {
        let settings = self.settings();
        if !settings.enabled {
            return;
        }
        let loaded = self.load(&settings);
        if watch {
            self.shared.watching.store(true, Ordering::Release);
            if loaded {
                self.rewatch();
            }
            self.full_scan(false);
        } else if !loaded {
            self.scan_now();
        }
    }

    /// Scans again from scratch (tray "Reindex"), even on battery.
    pub fn reindex(&self) {
        if self.settings().enabled {
            self.full_scan(true);
        }
    }

    /// New configuration: rescans when the roots or exclusions changed.
    pub fn reconfigure(&self, settings: Settings) {
        let old = self.settings();
        if old == settings {
            return;
        }
        let rescan = old.fingerprint() != settings.fingerprint() || old.enabled != settings.enabled;
        if let Ok(mut s) = self.shared.settings.write() {
            *s = settings.clone();
        }
        if !rescan {
            return;
        }
        if !settings.enabled {
            self.shared.cancel_scan.store(true, Ordering::Release);
            if let Ok(mut w) = self.shared.watcher.lock() {
                *w = None;
            }
            self.shared.watched.store(0, Ordering::Relaxed);
            if let Ok(mut i) = self.shared.index.write() {
                *i = None;
            }
            return;
        }
        if self.shared.index.read().is_ok_and(|i| i.is_none()) {
            self.load(&settings);
        }
        self.shared.watching.store(true, Ordering::Release);
        self.full_scan(true);
    }

    /// The window is shown: work kept aside on battery is done once back on
    /// mains; unwatched folders (Linux limit) are rescanned now and then.
    pub fn on_show(&self) {
        let settings = self.settings();
        if !settings.enabled || (settings.pause_on_battery && flair_platform::power::on_battery()) {
            return;
        }
        let pending = self
            .shared
            .pending
            .lock()
            .map(|mut p| std::mem::take(&mut *p))
            .unwrap_or_default();
        let stale = self.shared.partial_watch.load(Ordering::Relaxed)
            && self
                .shared
                .last_scan
                .lock()
                .ok()
                .and_then(|t| *t)
                .is_some_and(|t| t.elapsed() > UNWATCHED_RESCAN);
        if pending.full || stale {
            self.full_scan(false);
        } else if !pending.paths.is_empty() {
            let this = self.clone();
            let paths: Vec<PathBuf> = pending.paths.into_iter().collect();
            let spawned = std::thread::Builder::new()
                .name("files-catch-up".into())
                .spawn(move || {
                    flair_platform::priority::background_thread();
                    this.apply_changes(paths);
                });
            if let Err(e) = spawned {
                tracing::warn!(error = %e, "cannot catch up with file changes");
            }
        }
    }

    pub fn stats(&self) -> FileStats {
        let entries = self
            .current()
            .and_then(|b| index::view(b.bytes()).map(|v| v.len() as u64))
            .unwrap_or(0);
        let memory = self.current().map_or(0, |b| match &*b {
            Backing::Memory(v) => v.len(),
            Backing::Mapped(_) => 0,
        });
        let (overlay, added, removed) = self
            .shared
            .overlay
            .read()
            .map_or((0, 0, 0), |o| (o.bytes(), o.added.len(), o.removed.len()));
        FileStats {
            // Changes since the last scan included (folders removed count
            // as one entry: close enough for statistics).
            entries: (entries + added as u64).saturating_sub(removed as u64),
            file_bytes: self.shared.file_bytes.load(Ordering::Relaxed),
            ram_bytes: (memory + overlay) as u64,
            watched: self.shared.watched.load(Ordering::Relaxed) as u64,
            events: self.shared.events.load(Ordering::Relaxed),
        }
    }

    fn current(&self) -> Option<Arc<Backing>> {
        self.shared.index.read().ok().and_then(|i| i.clone())
    }

    // ------------------------------------------------------------ index files

    /// Index files are never overwritten (a mapped file cannot be replaced
    /// under Windows): each scan writes a new one, the older ones are deleted
    /// once unused.
    fn index_files(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.shared.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("files-") && n.ends_with(".idx"))
            })
            .collect();
        files.sort();
        files
    }

    /// Opens the newest saved index. False when there is none usable.
    fn load(&self, settings: &Settings) -> bool {
        let Some(file) = self.index_files().pop() else {
            return false;
        };
        let mapped = match MappedFile::open(&file) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, file = %file.display(), "cannot open the file index");
                return false;
            }
        };
        let Some(view) = index::view(mapped.bytes()) else {
            tracing::warn!(file = %file.display(), "file index damaged or old: scanning again");
            return false;
        };
        let same = view.settings.as_str() == settings.fingerprint();
        let count = view.len();
        self.shared
            .file_bytes
            .store(mapped.len() as u64, Ordering::Relaxed);
        if let Ok(mut i) = self.shared.index.write() {
            *i = Some(Arc::new(Backing::Mapped(mapped)));
        }
        tracing::info!(count, same_settings = same, "file index opened");
        true
    }

    fn save(&self, bytes: &[u8]) -> Option<(PathBuf, MappedFile)> {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let file = self.shared.dir.join(format!("files-{millis:020}.idx"));
        let tmp = file.with_extension("idx.tmp");
        let written = std::fs::create_dir_all(&self.shared.dir)
            .and_then(|()| std::fs::write(&tmp, bytes))
            .and_then(|()| std::fs::rename(&tmp, &file));
        if let Err(e) = written {
            tracing::warn!(error = %e, "cannot save the file index");
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        match MappedFile::open(&file) {
            Ok(m) => Some((file, m)),
            Err(e) => {
                tracing::warn!(error = %e, "cannot map the new file index");
                None
            }
        }
    }

    /// Deletes the index files other than `keep` (those still mapped by a
    /// running search are deleted next time).
    fn clean(&self, keep: Option<&Path>) {
        for file in self.index_files() {
            if Some(file.as_path()) != keep {
                let _ = std::fs::remove_file(file);
            }
        }
    }

    // ------------------------------------------------------------ scanning

    /// Scans in the background (returns at once); postponed on battery
    /// unless `force`.
    fn full_scan(&self, force: bool) {
        let settings = self.settings();
        if !settings.enabled {
            return;
        }
        if !force && settings.pause_on_battery && flair_platform::power::on_battery() {
            tracing::info!("on battery: file indexing postponed");
            if let Ok(mut p) = self.shared.pending.lock() {
                p.full = true;
            }
            return;
        }
        if self.shared.scanning.swap(true, Ordering::AcqRel) {
            self.shared.rescan.store(true, Ordering::Release);
            return;
        }
        self.shared.cancel_scan.store(false, Ordering::Release);
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("files-index".into())
            .spawn(move || {
                flair_platform::priority::background_thread();
                loop {
                    this.scan_now();
                    if !this.shared.rescan.swap(false, Ordering::AcqRel) {
                        break;
                    }
                }
                this.shared.scanning.store(false, Ordering::Release);
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot start the file indexing");
            self.shared.scanning.store(false, Ordering::Release);
        }
    }

    /// Scans, saves, maps and swaps the index, on the calling thread.
    fn scan_now(&self) {
        let settings = self.settings();
        let started = Instant::now();
        if let Ok(mut c) = self.shared.changed_during_scan.lock() {
            c.clear();
        }
        let scanned = walk::scan(
            &settings.roots,
            &settings.rules,
            walk::threads(),
            &self.shared.cancel_scan,
        );
        if self.shared.cancel_scan.load(Ordering::Acquire) {
            tracing::info!("file indexing cancelled");
            return;
        }
        if scanned.incomplete {
            tracing::warn!("file index full: some files are left out");
        }
        let dirs = scanned.dirs;
        let data = scanned.builder.finish(settings.fingerprint());
        let count = data.entries.len();
        let Some(bytes) = index::to_bytes(&data) else {
            tracing::warn!("cannot serialize the file index");
            return;
        };
        drop(data);
        let size = bytes.len() as u64;
        let (backing, file) = match self.save(&bytes) {
            Some((file, mapped)) => (Backing::Mapped(mapped), Some(file)),
            None => (Backing::Memory(bytes), None),
        };
        if let Ok(mut i) = self.shared.index.write() {
            *i = Some(Arc::new(backing));
        }
        self.shared.file_bytes.store(size, Ordering::Relaxed);
        if let Ok(mut o) = self.shared.overlay.write() {
            *o = Overlay::default();
        }
        if let Ok(mut t) = self.shared.last_scan.lock() {
            *t = Some(Instant::now());
        }
        self.clean(file.as_deref());
        tracing::info!(
            count,
            dirs,
            size,
            elapsed_ms = started.elapsed().as_millis(),
            "files indexed"
        );
        if self.shared.watching.load(Ordering::Acquire) {
            self.rewatch();
        }
        let replay = self
            .shared
            .changed_during_scan
            .lock()
            .map(|mut c| std::mem::take(&mut *c))
            .unwrap_or_default();
        if !replay.is_empty() {
            self.apply_changes(replay);
        }
    }

    // ------------------------------------------------------------ watching

    /// (Re)creates the watcher for the current roots and index.
    fn rewatch(&self) {
        let settings = self.settings();
        let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
        let config = notify_debouncer_full::Config::default().with_timeout(DEBOUNCE);
        let debouncer = notify_debouncer_full::new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
            config, tx, NoCache,
        );
        let mut debouncer = match debouncer {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "cannot watch the files");
                return;
            }
        };
        let mut watched = 0;
        let mut partial = false;
        if let Some(limit) = flair_platform::fs::watch_limit() {
            // Linux: one watch per folder, only the indexed ones (excluded
            // folders like `node_modules` cost nothing), within half of the
            // user's limit (other programs need some).
            let budget = limit / 2;
            let dirs = self.indexed_dirs();
            partial = dirs.len() > budget;
            if partial {
                tracing::warn!(
                    folders = dirs.len(),
                    limit,
                    "too many folders to watch them all: the others are rescanned when Flair is shown (raise fs.inotify.max_user_watches to watch more)"
                );
            }
            for dir in dirs.iter().take(budget) {
                if debouncer.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                    watched += 1;
                }
            }
        } else {
            for root in &settings.roots {
                match debouncer.watch(root, RecursiveMode::Recursive) {
                    Ok(()) => watched += 1,
                    Err(e) => tracing::warn!(error = %e, root = %root.display(), "cannot watch"),
                }
            }
        }
        self.shared.watched.store(watched, Ordering::Relaxed);
        self.shared.partial_watch.store(partial, Ordering::Relaxed);
        if let Ok(mut w) = self.shared.watcher.lock() {
            // The previous watcher stops here; its thread ends with it.
            *w = Some(debouncer);
        }
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("files-watch".into())
            .spawn(move || {
                // Sleeps until the debouncer delivers a batch.
                while let Ok(result) = rx.recv() {
                    this.on_events(result);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot start the file watcher");
        }
    }

    /// Folders of the current index (watched one by one under Linux).
    fn indexed_dirs(&self) -> Vec<PathBuf> {
        let Some(backing) = self.current() else {
            return Vec::new();
        };
        let Some(view) = index::view(backing.bytes()) else {
            return Vec::new();
        };
        #[allow(clippy::cast_possible_truncation)]
        (0..view.len() as u32)
            .filter(|id| view.is_dir(*id))
            .map(|id| view.path(id))
            .collect()
    }

    fn on_events(&self, result: DebounceEventResult) {
        self.shared.events.fetch_add(1, Ordering::Relaxed);
        let events = match result {
            Ok(events) => events,
            Err(errors) => {
                for e in errors {
                    tracing::debug!(error = %e, "file watching error");
                }
                return;
            }
        };
        let rescan = events.iter().any(|e| e.event.need_rescan());
        let mut paths: Vec<PathBuf> = events
            .into_iter()
            .filter(|e| !e.event.kind.is_access())
            .flat_map(|e| e.event.paths)
            .collect();
        paths.sort();
        paths.dedup();
        if self.shared.scanning.load(Ordering::Acquire)
            && let Ok(mut c) = self.shared.changed_during_scan.lock()
        {
            c.extend(paths.iter().cloned());
        }
        let settings = self.settings();
        if settings.pause_on_battery && flair_platform::power::on_battery() {
            if let Ok(mut p) = self.shared.pending.lock() {
                p.full |= rescan;
                p.paths.extend(paths);
                if p.paths.len() > MAX_PENDING {
                    p.paths.clear();
                    p.full = true;
                }
            }
            return;
        }
        if rescan {
            self.full_scan(false);
            return;
        }
        self.apply_changes(paths);
    }

    /// Brings the overlay up to date with changed paths.
    fn apply_changes(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if paths.len() > MAX_BATCH {
            self.full_scan(false);
            return;
        }
        let settings = self.settings();
        let paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| {
                settings
                    .roots
                    .iter()
                    .any(|r| p.starts_with(r) && p != r && !settings.rules.excludes_path(r, p))
            })
            .collect();
        if paths.is_empty() {
            return;
        }
        let backing = self.current();
        let view = backing.as_deref().and_then(|b| index::view(b.bytes()));
        let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        let known: Vec<(u32, PathBuf)> = view.map(|v| v.find(&refs)).unwrap_or_default();
        let Ok(mut overlay) = self.shared.overlay.write() else {
            return;
        };
        let mut new_dirs: Vec<PathBuf> = Vec::new();
        let mut too_big = false;
        for path in &paths {
            let id = known.iter().find(|(_, p)| p == path).map(|(id, _)| *id);
            let metadata = std::fs::symlink_metadata(path);
            let Ok(metadata) = metadata else {
                // Removed (or renamed away).
                if let Some(id) = id {
                    overlay.removed.insert(id);
                }
                overlay.added.retain(|a| !a.path.starts_with(path));
                continue;
            };
            let visible = match (id, view) {
                (Some(id), Some(v)) => !overlay.hides(v, id),
                _ => false,
            };
            if visible || overlay.added.iter().any(|a| &a.path == path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let dir = metadata.is_dir();
            overlay.added.push(Added {
                path: path.clone(),
                name: name.to_owned(),
                dir,
            });
            if dir {
                // A folder created or moved in: its content too.
                let tree = walk::scan(
                    std::slice::from_ref(path),
                    &settings.rules,
                    1,
                    &AtomicBool::new(false),
                );
                let tree_bytes = index::to_bytes(&tree.builder.finish(String::new()));
                let tree_view = tree_bytes.as_deref().and_then(index::view);
                if let Some(t) = tree_view {
                    if t.len() > MAX_NEW_TREE {
                        too_big = true;
                        break;
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    for child in 1..t.len() as u32 {
                        let child_path = t.path(child);
                        if t.is_dir(child) {
                            new_dirs.push(child_path.clone());
                        }
                        overlay.added.push(Added {
                            name: t.name(child).to_owned(),
                            path: child_path,
                            dir: t.is_dir(child),
                        });
                    }
                }
                new_dirs.push(path.clone());
            }
        }
        let size = overlay.len();
        drop(overlay);
        if too_big || size > MAX_OVERLAY {
            self.full_scan(false);
            return;
        }
        // Linux: new folders need their own watch.
        if flair_platform::fs::watch_limit().is_some()
            && !new_dirs.is_empty()
            && let Ok(mut guard) = self.shared.watcher.lock()
            && let Some(w) = guard.as_mut()
        {
            for dir in new_dirs {
                if w.watch(&dir, RecursiveMode::NonRecursive).is_ok() {
                    self.shared.watched.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    // ------------------------------------------------------------ results

    /// The system icon of a file type when loaded; otherwise Flair's own,
    /// and the system one is loaded in the background (shown from the next
    /// keystroke).
    fn icon(&self, path: &Path, dir: bool) -> IconSource {
        let fallback = IconSource::Builtin(if dir {
            BuiltinIcon::Folder
        } else {
            BuiltinIcon::File
        });
        let key = flair_platform::file_icon::key(path, dir);
        let Ok(mut icons) = self.shared.icons.lock() else {
            return fallback;
        };
        if let Some(known) = icons.get(&key) {
            return known.clone().unwrap_or(fallback);
        }
        if icons.len() >= MAX_ICONS {
            icons.clear();
        }
        icons.insert(key.clone(), None);
        drop(icons);
        let Ok(mut requests) = self.shared.icon_requests.lock() else {
            return fallback;
        };
        if requests.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(String, PathBuf, bool)>();
            let shared = Arc::clone(&self.shared);
            let spawned = std::thread::Builder::new()
                .name("files-icons".into())
                .spawn(move || {
                    // Sleeps until an icon is wanted.
                    while let Ok((key, path, dir)) = rx.recv() {
                        let Some(pixels) = flair_platform::file_icon::load(&path, dir, ICON_PIXELS)
                        else {
                            continue;
                        };
                        let icon = IconSource::Rgba {
                            width: pixels.width,
                            height: pixels.height,
                            pixels: pixels.rgba.into(),
                        };
                        if let Ok(mut icons) = shared.icons.lock() {
                            icons.insert(key, Some(icon));
                        }
                    }
                });
            match spawned {
                Ok(_) => *requests = Some(tx),
                Err(e) => tracing::warn!(error = %e, "cannot start the file icon loader"),
            }
        }
        if let Some(tx) = requests.as_ref() {
            let _ = tx.send((key, path.to_owned(), dir));
        }
        fallback
    }

    fn row(
        &self,
        path: PathBuf,
        name: &str,
        dir: bool,
        score: f32,
        indices: Vec<u32>,
    ) -> ResultItem {
        let home = self.shared.home.as_deref();
        let subtitle = path.parent().map(|p| paths::tilde(p, home));
        let mut title = name.to_owned();
        if title.is_empty() {
            title = path.display().to_string();
        }
        ResultItem {
            id: format!("file:{}", path.display()),
            title,
            subtitle,
            icon: self.icon(&path, dir),
            score,
            match_indices: indices,
            actions: vec![
                Action::Open { path: path.clone() },
                Action::RevealInFolder { path },
            ],
            provider: ProviderKind::File,
        }
    }

    /// `~/Doc`: the matching entries of the folder, in order.
    fn path_rows(&self, text: &str) -> Vec<ResultItem> {
        let listed = paths::list(text, self.shared.home.as_deref(), MAX_ROWS);
        #[allow(clippy::cast_precision_loss)]
        listed
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                // Above the other sources, in folder order.
                let score = 50.0 - i as f32 * 0.01;
                self.row(l.path, &l.name, l.dir, score, Vec::new())
            })
            .collect()
    }

    fn index_rows(&self, text: &str, cancel: &CancellationToken) -> Vec<ResultItem> {
        let needle = Needle::new(text);
        if needle.is_empty() {
            return Vec::new();
        }
        let backing = self.current();
        let view = backing.as_deref().and_then(|b| index::view(b.bytes()));
        let Ok(overlay) = self.shared.overlay.read() else {
            return Vec::new();
        };
        // (quick score, path, name, dir, depth)
        let mut found: Vec<(u32, PathBuf, String, bool, usize)> = Vec::new();
        if let Some(v) = view {
            let Some(hits) = index::search(v, &needle, CANDIDATES, &|| cancel.is_cancelled())
            else {
                return Vec::new();
            };
            for (quick, id) in hits {
                if overlay.hides(v, id) {
                    continue;
                }
                found.push((
                    quick,
                    v.path(id),
                    v.name(id).to_owned(),
                    v.is_dir(id),
                    v.depth(id),
                ));
            }
        }
        for a in &overlay.added {
            if let Some(quick) = needle.quick(&a.name, a.dir) {
                let depth = a.path.components().count().saturating_sub(3);
                found.push((quick, a.path.clone(), a.name.clone(), a.dir, depth));
            }
        }
        drop(overlay);
        if cancel.is_cancelled() {
            return Vec::new();
        }
        let prepared = Prepared::new(text);
        let mut ranker = Ranker::new();
        let frecency = self.shared.frecency.lock().ok();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut rows: Vec<ResultItem> = found
            .into_iter()
            .map(|(quick, path, name, dir, depth)| {
                let (quality, indices) = ranker
                    .match_text(&prepared, &name)
                    .map_or((1.0, Vec::new()), |m| (m.score, m.indices));
                let id = format!("file:{}", path.display());
                let used = frecency.as_ref().map_or(0.0, |f| f.bonus(&id, now));
                // The exact name (with or without extension) first.
                let exact = if quick >= 4000 { 1.0 } else { 0.0 };
                #[allow(clippy::cast_precision_loss)]
                let deep = depth.min(12) as f32 * 0.05;
                let score = quality
                    + exact
                    + ProviderKind::File.rank_bonus()
                    + if dir { 0.25 } else { 0.0 }
                    + used
                    - deep;
                self.row(path, &name, dir, score, indices)
            })
            .collect();
        // Equal scores: the shorter path first.
        rows.sort_by(|a, b| {
            b.score.total_cmp(&a.score).then_with(|| {
                let len = |r: &ResultItem| r.subtitle.as_ref().map_or(0, String::len);
                len(a).cmp(&len(b))
            })
        });
        rows.truncate(MAX_ROWS);
        rows
    }
}

impl Provider for FilesProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::File
    }

    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem> {
        if q.is_empty() || !q.accepts(ProviderKind::File) || !self.settings().enabled {
            return Vec::new();
        }
        let text = q.text.as_str();
        if paths::is_path(text) {
            return self.path_rows(text);
        }
        // One character matches too much to be useful.
        if text.chars().count() < 2 {
            return Vec::new();
        }
        self.index_rows(text, cancel)
    }

    fn refresh(&self) {
        self.reindex();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(tag: &str) -> (FilesProvider, PathBuf, PathBuf) {
        let root = walk::tests::tree(tag);
        let dir =
            std::env::temp_dir().join(format!("flair-files-idx-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = Settings::new(
            true,
            &[root.display().to_string()],
            &["node_modules".into()],
            false,
            false,
            false,
            Some(&root),
        );
        let p = FilesProvider::new(
            dir.clone(),
            Some(root.clone()),
            Arc::new(Mutex::new(Frecency::default())),
            settings,
        );
        (p, root, dir)
    }

    fn titles(p: &FilesProvider, text: &str) -> Vec<String> {
        p.query(&Query::parse(text), &CancellationToken::new())
            .into_iter()
            .map(|r| r.title)
            .collect()
    }

    #[test]
    fn scans_saves_and_searches() {
        let (p, root, dir) = provider("search");
        p.start(false);
        assert_eq!(titles(&p, "rapport"), ["rapport.pdf"]);
        assert_eq!(titles(&p, "vacances")[0], "vacances.jpg");
        assert!(titles(&p, "index.js").is_empty(), "excluded folder");
        assert!(titles(&p, "secret").is_empty(), "hidden folder");
        let rows = p.query(&Query::parse("plan"), &CancellationToken::new());
        assert_eq!(
            rows[0].subtitle.as_deref().map(|s| s.replace('\\', "/")),
            Some("~/Documents/Projets".into())
        );
        assert!(matches!(rows[0].actions[1], Action::RevealInFolder { .. }));
        let stats = p.stats();
        assert_eq!(stats.entries, 8);
        assert!(stats.file_bytes > 0);

        // A new provider opens the saved file at once.
        let again = FilesProvider::new(
            dir.clone(),
            Some(root.clone()),
            Arc::new(Mutex::new(Frecency::default())),
            p.settings(),
        );
        assert!(again.load(&p.settings()));
        assert_eq!(titles(&again, "rapport"), ["rapport.pdf"]);
        assert_eq!(p.index_files().len(), 1);
        drop(again);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn changes_go_to_the_overlay() {
        let (p, root, dir) = provider("overlay");
        p.start(false);
        // Removed file, new file, new folder with content, excluded file.
        std::fs::remove_file(root.join("Documents/rapport.pdf")).expect("rm");
        std::fs::write(root.join("Documents/budget.ods"), b"x").expect("write");
        std::fs::create_dir_all(root.join("Musique/Jazz")).expect("dir");
        std::fs::write(root.join("Musique/Jazz/take five.mp3"), b"x").expect("write");
        std::fs::write(root.join("node_modules/new.js"), b"x").expect("write");
        p.apply_changes(vec![
            root.join("Documents/rapport.pdf"),
            root.join("Documents/budget.ods"),
            root.join("Musique"),
            root.join("node_modules/new.js"),
        ]);
        assert!(titles(&p, "rapport").is_empty());
        assert_eq!(titles(&p, "budget"), ["budget.ods"]);
        assert_eq!(titles(&p, "take five"), ["take five.mp3"]);
        assert!(titles(&p, "new.js").is_empty());
        // 8 scanned − rapport.pdf + budget.ods + Musique, Jazz, take five.mp3.
        assert_eq!(p.stats().entries, 11);
        // A removed folder hides its content.
        std::fs::remove_dir_all(root.join("Photos")).expect("rm");
        p.apply_changes(vec![root.join("Photos")]);
        assert!(titles(&p, "vacances").is_empty());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Waits (tests only) until `check` holds, up to 15 s.
    fn eventually(check: impl Fn() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            if check() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn watching_follows_changes() {
        let (p, root, dir) = provider("watch");
        p.start(true);
        assert!(eventually(
            || !p.shared.scanning.load(Ordering::Acquire) && p.stats().entries > 0
        ));
        assert!(p.stats().watched > 0);
        std::fs::write(root.join("Documents/devis.xlsx"), b"x").expect("write");
        assert!(
            eventually(|| titles(&p, "devis") == ["devis.xlsx"]),
            "created"
        );
        std::fs::rename(
            root.join("Documents/devis.xlsx"),
            root.join("Documents/facture.xlsx"),
        )
        .expect("mv");
        assert!(
            eventually(
                || titles(&p, "facture") == ["facture.xlsx"] && titles(&p, "devis").is_empty()
            ),
            "renamed"
        );
        std::fs::create_dir_all(root.join("Documents/Nouveau/Sous")).expect("dir");
        std::fs::write(root.join("Documents/Nouveau/Sous/profond.txt"), b"x").expect("write");
        assert!(
            eventually(|| titles(&p, "profond") == ["profond.txt"]),
            "new tree"
        );
        std::fs::remove_file(root.join("Documents/rapport.pdf")).expect("rm");
        assert!(eventually(|| titles(&p, "rapport").is_empty()), "removed");
        assert!(p.stats().events > 0);
        if let Ok(mut w) = p.shared.watcher.lock() {
            *w = None;
        }
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn typed_paths_list_folders() {
        let (p, root, dir) = provider("typed");
        assert_eq!(titles(&p, "~/Doc"), ["Documents"]);
        assert_eq!(titles(&p, "~/Documents/"), ["Projets", "rapport.pdf"]);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_expand_and_dedupe_roots() {
        let home = Path::new("/home/me");
        let s = Settings::new(
            true,
            &["~".into(), "~/Documents".into(), "/data".into()],
            &[],
            false,
            false,
            true,
            Some(home),
        );
        assert_eq!(s.roots, [PathBuf::from("/data"), PathBuf::from("/home/me")]);
    }
}

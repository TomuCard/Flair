//! Screens shown in the result list: installation offer, update, export,
//! import and uninstallation. Each is a short list of rows; `Enter` runs the
//! internal command of the selected row. Work runs on background threads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use flair_core::archive::{self, Archive, ImportMode, Warning};
use flair_core::result::BuiltinIcon;
use flair_core::settings::config::UpdateChannel;
use flair_core::update::{Decision, decide};
use flair_core::{Action, Provider, ResultItem};
use iced::Task;
use iced::futures::channel::{mpsc, oneshot};

use super::{Flair, Message};
use crate::build_info;
use crate::commands::{self, info, row};
use crate::updater::{self, Endpoint, Release};

/// What the list shows instead of search results.
#[derive(Debug, Clone)]
pub enum Screen {
    Install {
        autostart: bool,
    },
    Checking,
    UpdateAvailable {
        release: Arc<Release>,
        confirm_reset: bool,
    },
    Downloading {
        percent: u8,
    },
    Export,
    Import,
    ImportSummary {
        archive: Arc<Archive>,
    },
    Uninstall,
    /// Plain rows (results of an action).
    Rows(Vec<ResultItem>),
}

/// Results of background work.
#[derive(Debug, Clone)]
pub enum Done {
    Checked(Result<Arc<Release>, String>),
    AutoChecked(Option<String>),
    Progress(u8),
    Installed(Result<(), String>),
    SavePicked {
        settings_only: bool,
        path: Option<PathBuf>,
    },
    Exported(Result<(PathBuf, u64), String>),
    OpenPicked(Option<PathBuf>),
    ImportRead(Result<Arc<Archive>, String>),
    Imported(Result<(archive::Applied, Option<PathBuf>), String>),
    Undone(Result<(), String>),
    InstalledApp(Result<PathBuf, String>),
}

/// Runs `work` on a new thread; its result comes back as a message.
pub fn background<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: fn(T) -> Done,
) -> Task<Message> {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    Task::perform(rx, move |result| match result {
        Ok(value) => Message::Done(done(value)),
        // The worker panicked: report it as a failure where possible.
        Err(_) => Message::Done(Done::Installed(Err("internal error".into()))),
    })
}

pub fn bytes(n: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let kib = n as f64 / 1024.0;
    if kib < 10.0 {
        format!("{kib:.1} Ko")
    } else if kib < 1024.0 {
        format!("{kib:.0} Ko")
    } else {
        format!("{:.1} Mo", kib / 1024.0)
    }
}

/// `20261126-141200` → `26/11/2026 14:12` (UTC).
fn stamp(stamp: &str) -> String {
    let s = stamp.as_bytes();
    if s.len() < 15 {
        return stamp.to_owned();
    }
    let p = |a: usize, b: usize| std::str::from_utf8(&s[a..b]).unwrap_or("");
    format!(
        "{}/{}/{} {}:{}",
        p(6, 8),
        p(4, 6),
        p(0, 4),
        p(9, 11),
        p(11, 13)
    )
}

impl Flair {
    fn exe(&self) -> Option<PathBuf> {
        std::env::current_exe().ok()
    }

    pub(super) fn set_screen(&mut self, screen: Option<Screen>) -> Task<Message> {
        self.screen = screen;
        self.screen_rows = self.compute_screen_rows();
        self.input.clear();
        self.results.clear();
        self.selected = 0;
        Task::batch([self.resize(), self.scroll_to(0.0)])
    }

    fn message(&mut self, rows: Vec<ResultItem>) -> Task<Message> {
        self.set_screen(Some(Screen::Rows(rows)))
    }

    fn say(&mut self, text: String) -> Task<Message> {
        self.message(vec![info(text, None)])
    }

    fn compute_screen_rows(&self) -> Vec<ResultItem> {
        let t = |id: &str| self.i18n.t(id);
        let ta = |id: &str, args: &[(&str, &str)]| self.i18n.t_args(id, args);
        let Some(screen) = &self.screen else {
            return Vec::new();
        };
        match screen {
            Screen::Install { autostart } => {
                let dir = flair_platform::install::install_dir()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                let state = t(if *autostart { "state-on" } else { "state-off" });
                vec![
                    row(
                        "install:go",
                        t("install-title"),
                        Some(ta("install-desc", &[("path", &dir)])),
                        BuiltinIcon::App,
                    ),
                    row(
                        "install:autostart",
                        ta("install-autostart", &[("state", &state)]),
                        None,
                        BuiltinIcon::Settings,
                    ),
                    row(
                        "install:portable",
                        t("install-portable"),
                        Some(t("install-portable-desc")),
                        BuiltinIcon::Folder,
                    ),
                ]
            }
            Screen::Checking => vec![info(t("update-checking"), None)],
            Screen::UpdateAvailable {
                release,
                confirm_reset,
            } => {
                let m = &release.manifest;
                let mut rows = Vec::new();
                if *confirm_reset {
                    rows.push(row(
                        "update:install-reset",
                        t("update-reset-warning"),
                        Some(t("update-available-hint")),
                        BuiltinIcon::Restart,
                    ));
                } else {
                    rows.push(row(
                        "update:install",
                        ta(
                            "update-available",
                            &[("version", &m.version), ("current", build_info::VERSION)],
                        ),
                        Some(t("update-available-hint")),
                        BuiltinIcon::Restart,
                    ));
                }
                rows.extend(m.notes.iter().map(|note| info(note.clone(), None)));
                rows
            }
            Screen::Downloading { percent } => {
                vec![info(
                    ta("update-downloading", &[("percent", &percent.to_string())]),
                    None,
                )]
            }
            Screen::Export => vec![
                row(
                    "export:all",
                    t("export-all"),
                    Some(t("export-all-desc")),
                    BuiltinIcon::File,
                ),
                row(
                    "export:settings",
                    t("export-settings"),
                    Some(t("export-settings-desc")),
                    BuiltinIcon::Settings,
                ),
            ],
            Screen::Import => {
                let mut rows = vec![row(
                    "import:pick",
                    t("import-pick"),
                    Some(t("import-pick-desc")),
                    BuiltinIcon::Folder,
                )];
                for backup in archive::list_backups(&self.paths.data_dir.join("backups")) {
                    let reason_id = format!(
                        "backup-reason-{}",
                        backup.reason.split('-').next().unwrap_or("")
                    );
                    let reason = t(&reason_id);
                    rows.push(row(
                        &format!("import:file:{}", backup.path.display()),
                        ta(
                            "import-backup",
                            &[("date", &stamp(&backup.stamp)), ("reason", &reason)],
                        ),
                        Some(backup.path.display().to_string()),
                        BuiltinIcon::File,
                    ));
                }
                rows
            }
            Screen::ImportSummary { archive } => self.summary_rows(archive),
            Screen::Uninstall => vec![
                row(
                    "uninstall:keep",
                    t("uninstall-keep"),
                    Some(t("uninstall-keep-desc")),
                    BuiltinIcon::Trash,
                ),
                row(
                    "uninstall:all",
                    t("uninstall-all"),
                    Some(t("uninstall-all-desc")),
                    BuiltinIcon::Trash,
                ),
            ],
            Screen::Rows(rows) => rows.clone(),
        }
    }

    fn summary_rows(&self, archive: &Archive) -> Vec<ResultItem> {
        let t = |id: &str| self.i18n.t(id);
        let ta = |id: &str, args: &[(&str, &str)]| self.i18n.t_args(id, args);
        let m = &archive.manifest;
        let date = m.date.get(..10).map_or(String::new(), |d| {
            let p: Vec<&str> = d.split('-').collect();
            if p.len() == 3 {
                format!("{}/{}/{}", p[2], p[1], p[0])
            } else {
                d.to_owned()
            }
        });
        let mut contents = Vec::new();
        if archive.config.is_some() {
            contents.push(t("import-contents-settings"));
        }
        if !archive.themes.is_empty() {
            contents.push(ta(
                "import-contents-themes",
                &[("count", &archive.themes.len().to_string())],
            ));
        }
        if !archive.lexicons.is_empty() {
            contents.push(ta(
                "import-contents-lexicons",
                &[("count", &archive.lexicons.len().to_string())],
            ));
        }
        if let Some(history) = &archive.history {
            let count = serde_json::from_str::<serde_json::Value>(history)
                .ok()
                .and_then(|v| v.as_array().map(Vec::len))
                .unwrap_or(0);
            contents.push(ta(
                "import-contents-history",
                &[("count", &count.to_string())],
            ));
        }
        if archive.frecency.is_some() {
            contents.push(t("import-contents-frecency"));
        }
        let mut rows = vec![
            row(
                "import:replace",
                t("import-replace"),
                Some(t("import-replace-desc")),
                BuiltinIcon::File,
            ),
            row(
                "import:merge",
                t("import-merge"),
                Some(t("import-merge-desc")),
                BuiltinIcon::File,
            ),
            info(
                ta(
                    "import-summary",
                    &[
                        ("date", &date),
                        ("version", &m.flair_version),
                        ("host", &m.host),
                        ("os", &m.os),
                    ],
                ),
                Some(ta("import-contents", &[("contents", &contents.join(", "))])),
            ),
        ];
        for warning in &archive.warnings {
            let text = match warning {
                Warning::MissingRoot(path) => ta("import-warning-root", &[("path", path)]),
                Warning::Invalid { path, reason } => {
                    format!(
                        "{} — {reason}",
                        ta("import-warning-invalid", &[("path", path)])
                    )
                }
                Warning::Ignored(path) => ta("import-warning-ignored", &[("path", path)]),
            };
            rows.push(info(text, None));
        }
        rows
    }

    /// Runs an internal command (`Action::Internal`).
    pub(super) fn run_command(&mut self, id: &str) -> Task<Message> {
        tracing::info!(command = id, "command");
        match id {
            "update" => self.start_update_check(),
            "update:install" => self.start_update_install(false),
            "update:install-reset" => self.start_update_install(true),
            "update:rollback" => self.rollback(),
            "version" => {
                let rows = vec![info(
                    self.i18n
                        .t_args("version-line", &[("version", build_info::VERSION)]),
                    Some(self.i18n.t_args(
                        "version-detail",
                        &[
                            ("commit", build_info::COMMIT),
                            ("date", &build_info::date()),
                            ("channel", build_info::CHANNEL),
                        ],
                    )),
                )];
                self.message(rows)
            }
            "stats" => {
                let rows = super::stats_rows(&self.i18n, &crate::stats::snapshot());
                self.message(rows)
            }
            "export" => self.set_screen(Some(Screen::Export)),
            "export:all" => self.pick_export(false),
            "export:settings" => self.pick_export(true),
            "import" => self.set_screen(Some(Screen::Import)),
            "import:pick" => self.pick_import(),
            "import:replace" => self.apply_import(ImportMode::ReplaceAll),
            "import:merge" => self.apply_import(ImportMode::AddThemesAndLexicons),
            "undo" => {
                if self.last_setting.is_some() {
                    self.undo_setting()
                } else {
                    self.undo_import()
                }
            }
            "reindex" => {
                if let Some(sources) = &self.sources {
                    sources.apps.refresh();
                    sources.files.reindex();
                }
                self.say(self.i18n.t("reindex-started"))
            }
            "logs" => {
                let dir = self.paths.log_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = flair_platform::open::open_path(&dir) {
                    tracing::error!("cannot open the logs: {e}");
                }
                self.hide()
            }
            "history_clear" => {
                if let Some(sources) = &self.sources {
                    if let Ok(mut h) = sources.history.lock() {
                        h.clear();
                    }
                    if let Ok(mut f) = sources.frecency.lock() {
                        f.clear();
                    }
                }
                self.say(self.i18n.t("history-cleared"))
            }
            "restart_flair" => match self.exe() {
                Some(exe) => match updater::restart(&exe, "--show") {
                    Ok(()) => iced::exit(),
                    Err(e) => self.say(e.to_string()),
                },
                None => Task::none(),
            },
            "config:open" => {
                if let Err(e) = flair_platform::open::open_path(&self.config_path) {
                    tracing::error!("cannot open the configuration: {e}");
                }
                self.hide()
            }
            "config:reload" => {
                self.reload_config();
                self.say(self.i18n.t("config-reloaded"))
            }
            "install" => self.set_screen(Some(Screen::Install { autostart: true })),
            "install:autostart" => {
                if let Some(Screen::Install { autostart }) = self.screen {
                    return self.set_screen(Some(Screen::Install {
                        autostart: !autostart,
                    }));
                }
                Task::none()
            }
            "install:go" => self.install_app(),
            "install:portable" => {
                if let Some(exe) = self.exe() {
                    let _ = std::fs::write(
                        portable_marker(&self.paths.data_dir),
                        exe.display().to_string(),
                    );
                }
                self.set_screen(None)
            }
            "uninstall" => self.set_screen(Some(Screen::Uninstall)),
            "uninstall:keep" => self.uninstall(false),
            "uninstall:all" => self.uninstall(true),
            "quit" => iced::exit(),
            other => {
                if let Some(name) = other.strip_prefix("theme:use:") {
                    return self.use_theme(name);
                }
                if let Some(theme) = commands::theme::decode_new(other) {
                    return self.save_theme(&theme);
                }
                if let Some(text) = other.strip_prefix("fill:") {
                    return self.fill(text.to_owned());
                }
                if let Some((key, value)) = commands::settings::decode_set(other) {
                    return self.apply_setting(&key, value);
                }
                if let Some(id) = other.strip_prefix("help:") {
                    let rows = self.with_command_context(|cx| commands::help::detail(id, cx));
                    return self.message(rows);
                }
                if let Some(path) = other.strip_prefix("import:file:") {
                    return self.read_import(PathBuf::from(path));
                }
                if let Some(channel) = other.strip_prefix("update:channel:") {
                    return self.set_channel(channel);
                }
                tracing::warn!(command = other, "unknown command");
                Task::none()
            }
        }
    }

    /// Writes `text` in the field (commands taking arguments).
    fn fill(&mut self, text: String) -> Task<Message> {
        Task::batch([
            self.search(text),
            iced::widget::operation::move_cursor_to_end(iced::widget::Id::new(super::INPUT_ID)),
        ])
    }

    pub(super) fn with_command_context<R>(&self, f: impl FnOnce(&commands::Context<'_>) -> R) -> R {
        let cx = commands::Context {
            i18n: &self.i18n,
            words: &self.words,
            config: &self.config,
            languages: &self.languages,
            portable: false,
            update_available: None,
        };
        f(&cx)
    }

    /// Applies a setting chosen in a `>` command: validated, written with its
    /// comments kept, applied at once, undoable with `> annuler`.
    fn apply_setting(&mut self, key: &str, value: toml::Value) -> Task<Message> {
        let Some(desc) = flair_core::settings::registry::find(key) else {
            return Task::none();
        };
        let old = commands::settings::current(&self.config, key);
        // The new shortcut is registered first: refused, the old one stays.
        if key == "hotkey"
            && let Some(text) = value.as_str()
        {
            if let Err(e) = self.services.try_hotkey(text) {
                return self.say(self.i18n.t_args("hotkey-refused", &[("error", &e)]));
            }
            text.clone_into(&mut self.config.hotkey);
        }
        if key == "autostart"
            && let Some(enabled) = value.as_bool()
        {
            let registered = std::env::current_exe()
                .and_then(|exe| flair_platform::autostart::set(enabled, &exe));
            if let Err(e) = registered {
                return self.say(
                    self.i18n
                        .t_args("setting-failed", &[("error", &e.to_string())]),
                );
            }
            if let Some(tray) = &mut self.services.tray {
                tray.set_autostart_checked(enabled);
            }
        }
        if let Err(e) = flair_core::settings::edit::set_in_file(&self.config_path, key, &value) {
            let error = match &e {
                flair_core::settings::edit::EditError::Invalid(v) => self.i18n.value_error(v),
                other => other.to_string(),
            };
            return self.say(self.i18n.t_args("setting-failed", &[("error", &error)]));
        }
        tracing::info!(key, value = %value, "setting changed");
        self.last_setting = Some((key.to_owned(), old));
        self.reload_config();
        let shown = self.with_command_context(|cx| commands::settings::display(&value, cx));
        let name = desc.name.get(self.i18n.lang());
        let mut rows = vec![info(
            self.i18n
                .t_args("setting-applied", &[("name", name), ("value", &shown)]),
            Some(self.i18n.t("setting-applied-hint")),
        )];
        if desc.requires_restart {
            rows[0].subtitle = Some(self.i18n.t("setting-restart"));
            rows.push(row(
                "restart_flair",
                self.i18n.t("cmd-restart_flair"),
                Some(self.i18n.t("cmd-restart_flair-desc")),
                BuiltinIcon::Command,
            ));
        }
        self.message(rows)
    }

    /// Uses a theme: it goes to the light or dark slot depending on its
    /// background; a fixed mode of the other kind switches to its kind.
    fn use_theme(&mut self, name: &str) -> Task<Message> {
        use flair_core::settings::config::ThemeMode;
        let Some(theme) = crate::themes::find(name) else {
            return Task::none();
        };
        let light = theme.is_light();
        let slot = if light { "theme.light" } else { "theme.dark" };
        let old = commands::settings::current(&self.config, slot);
        let write = |key: &str, value: &str| {
            flair_core::settings::edit::set_in_file(
                &self.config_path,
                key,
                &toml::Value::String(value.to_owned()),
            )
        };
        let mut result = write(slot, name);
        let mode = self.config.theme.mode;
        if result.is_ok() && mode != ThemeMode::System && (mode == ThemeMode::Light) != light {
            result = write("theme.mode", if light { "light" } else { "dark" });
        }
        if let Err(e) = result {
            return self.say(
                self.i18n
                    .t_args("setting-failed", &[("error", &e.to_string())]),
            );
        }
        self.last_setting = Some((slot.to_owned(), old));
        self.reload_config();
        let kind = self
            .i18n
            .t(if light { "theme-light" } else { "theme-dark" });
        self.say(
            self.i18n
                .t_args("theme-applied", &[("name", name), ("kind", &kind)]),
        )
    }

    /// Saves a theme created by `> thème <colors> <name>`, then uses it.
    fn save_theme(&mut self, theme: &flair_core::theme::Theme) -> Task<Message> {
        let dir = self.paths.themes_dir();
        let file = dir.join(crate::themes::file_name(&theme.name));
        let written = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&file, crate::themes::to_toml(theme)));
        if let Err(e) = written {
            return self.say(
                self.i18n
                    .t_args("setting-failed", &[("error", &e.to_string())]),
            );
        }
        tracing::info!(file = %file.display(), "theme saved");
        crate::themes::reload(&dir);
        self.use_theme(&theme.name)
    }

    /// `> annuler` after a setting change.
    fn undo_setting(&mut self) -> Task<Message> {
        let Some((key, old)) = self.last_setting.take() else {
            return self.say(self.i18n.t("undo-nothing"));
        };
        let Some(desc) = flair_core::settings::registry::find(&key) else {
            return Task::none();
        };
        let Some(value) = old.or_else(|| desc.default_value()) else {
            return Task::none();
        };
        if key == "hotkey"
            && let Some(text) = value.as_str()
        {
            let _ = self.services.try_hotkey(text);
            text.clone_into(&mut self.config.hotkey);
        }
        if key == "autostart"
            && let Some(enabled) = value.as_bool()
        {
            let _ = std::env::current_exe()
                .and_then(|exe| flair_platform::autostart::set(enabled, &exe));
        }
        if let Err(e) = flair_core::settings::edit::set_in_file(&self.config_path, &key, &value) {
            return self.say(
                self.i18n
                    .t_args("setting-failed", &[("error", &e.to_string())]),
            );
        }
        self.reload_config();
        let shown = self.with_command_context(|cx| commands::settings::display(&value, cx));
        self.say(self.i18n.t_args(
            "setting-undone",
            &[("name", desc.name.get(self.i18n.lang())), ("value", &shown)],
        ))
    }

    /// Why updates cannot be installed here, if so.
    fn update_blocker(&self) -> Option<&'static str> {
        if build_info::is_dev() {
            return Some("update-disabled-dev");
        }
        let exe = self.exe()?;
        match flair_platform::install::location(&exe) {
            flair_platform::install::Location::PackageManaged => Some("update-disabled-package"),
            flair_platform::install::Location::Development => Some("update-disabled-dev"),
            _ if !flair_platform::install::can_self_update(&exe) => {
                Some("update-disabled-readonly")
            }
            _ => None,
        }
    }

    fn start_update_check(&mut self) -> Task<Message> {
        // Say it at once rather than offering an update that cannot be installed.
        if let Some(blocker) = self.update_blocker() {
            return self.say(self.i18n.t(blocker));
        }
        let channel = self.config.update.channel;
        let task = background(
            move || {
                updater::latest(&Endpoint::github(), channel)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            },
            Done::Checked,
        );
        Task::batch([self.set_screen(Some(Screen::Checking)), task])
    }

    /// Automatic check: at most once a day, only when Flair is shown.
    pub(super) fn maybe_auto_check(&mut self) -> Task<Message> {
        if !self.config.update.auto_check
            || self.update_blocker().is_some()
            || !updater::check_due(&self.update_files)
        {
            return Task::none();
        }
        updater::record_check(&self.update_files);
        let channel = self.config.update.channel;
        background(
            move || {
                let release = updater::latest(&Endpoint::github(), channel).ok()?;
                (decide(build_info::VERSION, &release.manifest).ok()? == Decision::Available)
                    .then(|| release.manifest.version.clone())
            },
            Done::AutoChecked,
        )
    }

    fn start_update_install(&mut self, reset_confirmed: bool) -> Task<Message> {
        let Some(Screen::UpdateAvailable { release, .. }) = self.screen.clone() else {
            return Task::none();
        };
        if let Some(blocker) = self.update_blocker() {
            return self.say(self.i18n.t(blocker));
        }
        if release.manifest.requires_reset && !reset_confirmed {
            return self.set_screen(Some(Screen::UpdateAvailable {
                release,
                confirm_reset: true,
            }));
        }
        let Some(exe) = self.exe() else {
            return Task::none();
        };
        let layout = self.layout();
        let backups = self.paths.data_dir.join("backups");
        let files = self.update_files.clone();
        let config_file = self.paths.config_file();
        let (tx, rx) = mpsc::unbounded();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                // Same format as `> export`: a backup before any update.
                let origin = archive::Origin::current(build_info::VERSION);
                archive::backup(&layout, &backups, &origin, "update").map_err(|e| e.to_string())?;
                let progress_tx = tx.clone();
                let binary =
                    updater::fetch_verified(&Endpoint::github(), &release, &move |got, total| {
                        if let Some(total) = total.filter(|t| *t > 0) {
                            #[allow(clippy::cast_possible_truncation)]
                            let percent = (got * 100 / total).min(100) as u8;
                            let _ = progress_tx.unbounded_send(Done::Progress(percent));
                        }
                    })
                    .map_err(|e| e.to_string())?;
                updater::install(
                    &binary,
                    &exe,
                    &files,
                    build_info::VERSION,
                    &release.manifest.version,
                )
                .map_err(|e| e.to_string())?;
                if release.manifest.requires_reset {
                    // Backed up above; the new version recreates the defaults.
                    let _ = std::fs::remove_file(&config_file);
                }
                updater::restart(&exe, "--after-update").map_err(|e| e.to_string())
            })();
            let _ = tx.unbounded_send(Done::Installed(result));
        });
        Task::batch([
            self.set_screen(Some(Screen::Downloading { percent: 0 })),
            Task::run(rx, Message::Done),
        ])
    }

    fn rollback(&mut self) -> Task<Message> {
        let Some(exe) = self.exe() else {
            return Task::none();
        };
        if let Some(blocker) = self.update_blocker() {
            return self.say(self.i18n.t(blocker));
        }
        match updater::swap_with_previous(&exe) {
            Ok(()) => {
                let _ = std::fs::remove_file(&self.update_files.health);
                match updater::restart(&exe, "--after-update") {
                    Ok(()) => iced::exit(),
                    Err(e) => self.say(
                        self.i18n
                            .t_args("update-failed", &[("error", &e.to_string())]),
                    ),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.say(self.i18n.t("update-rollback-none"))
            }
            Err(e) => self.say(
                self.i18n
                    .t_args("update-failed", &[("error", &e.to_string())]),
            ),
        }
    }

    fn set_channel(&mut self, name: &str) -> Task<Message> {
        let channel = if name == "stable" {
            UpdateChannel::Stable
        } else {
            UpdateChannel::Main
        };
        let result = flair_core::settings::edit::set_in_file(
            &self.paths.config_file(),
            "update.channel",
            &toml::Value::String(commands::channel_name(channel).into()),
        );
        match result {
            Ok(()) => {
                self.config.update.channel = channel;
                self.say(self.i18n.t_args("update-channel-set", &[("channel", name)]))
            }
            Err(e) => self.say(e.to_string()),
        }
    }

    pub(super) fn layout(&self) -> archive::Layout {
        archive::Layout {
            config_file: self.paths.config_file(),
            themes_dir: self.paths.themes_dir(),
            lexicon_dir: self.paths.lexicon_dir(),
            data_dir: self.paths.data_dir.clone(),
            home: directories::BaseDirs::new()
                .map(|b| b.home_dir().to_path_buf())
                .unwrap_or_default(),
        }
    }

    fn pick_export(&mut self, settings_only: bool) -> Task<Message> {
        self.dialog_open = true;
        let title = self.i18n.t("export-dialog-title");
        let name = format!(
            "flair-export-{}.flair",
            flair_core::time::UtcDateTime::now().date()
        );
        background(
            move || {
                let documents = directories::UserDirs::new()
                    .and_then(|u| u.document_dir().map(Path::to_path_buf));
                let path = flair_platform::dialog::save_file(
                    &title,
                    &name,
                    documents.as_deref(),
                    &flair_filter(),
                );
                (settings_only, path)
            },
            |(settings_only, path)| Done::SavePicked {
                settings_only,
                path,
            },
        )
    }

    fn pick_import(&mut self) -> Task<Message> {
        self.dialog_open = true;
        let title = self.i18n.t("import-dialog-title");
        background(
            move || {
                let documents = directories::UserDirs::new()
                    .and_then(|u| u.document_dir().map(Path::to_path_buf));
                flair_platform::dialog::open_file(&title, documents.as_deref(), &flair_filter())
            },
            Done::OpenPicked,
        )
    }

    /// Reads a `.flair` file and shows its summary (`flair file.flair`,
    /// double-click, backups list).
    pub(super) fn read_import(&mut self, path: PathBuf) -> Task<Message> {
        let home = self.layout().home;
        background(
            move || match archive::read(&path, &home) {
                Ok(archive) => Ok(Arc::new(archive)),
                Err(archive::ArchiveError::Newer) => Err(String::from("newer")),
                Err(e) => Err(e.to_string()),
            },
            Done::ImportRead,
        )
    }

    fn apply_import(&mut self, mode: ImportMode) -> Task<Message> {
        let Some(Screen::ImportSummary { archive }) = self.screen.clone() else {
            return Task::none();
        };
        let layout = self.layout();
        let backups = self.paths.data_dir.join("backups");
        background(
            move || {
                // Backup of the current state first: `> undo` restores it.
                let origin = archive::Origin::current(build_info::VERSION);
                let backup = archive::backup(&layout, &backups, &origin, "import").ok();
                archive::apply(&archive, &layout, mode)
                    .map(|applied| (applied, backup))
                    .map_err(|e| e.to_string())
            },
            Done::Imported,
        )
    }

    fn undo_import(&mut self) -> Task<Message> {
        let Some(backup) = self.last_import_backup.take() else {
            return self.say(self.i18n.t("undo-nothing"));
        };
        let layout = self.layout();
        background(
            move || {
                let archive = archive::read(&backup, &layout.home).map_err(|e| e.to_string())?;
                archive::apply(&archive, &layout, ImportMode::ReplaceAll)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            Done::Undone,
        )
    }

    fn install_app(&mut self) -> Task<Message> {
        let Some(Screen::Install { autostart }) = self.screen else {
            return Task::none();
        };
        let Some(exe) = self.exe() else {
            return Task::none();
        };
        let config_file = self.paths.config_file();
        background(
            move || {
                let svg =
                    crate::assets::LOGO_SVG.replace("currentColor", crate::assets::APP_ICON_COLOR);
                let target = flair_platform::install::install(
                    &exe,
                    flair_platform::install::Options { autostart },
                    build_info::VERSION,
                    &svg,
                )
                .map_err(|e| e.to_string())?;
                let _ = flair_core::settings::edit::set_in_file(
                    &config_file,
                    "autostart",
                    &toml::Value::Boolean(autostart),
                );
                Ok(target)
            },
            Done::InstalledApp,
        )
    }

    fn uninstall(&mut self, remove_data: bool) -> Task<Message> {
        let Some(exe) = self.exe() else {
            return Task::none();
        };
        let dirs = if remove_data {
            vec![
                self.paths.config_dir.clone(),
                self.paths.data_dir.clone(),
                self.paths.cache_dir.clone(),
            ]
        } else {
            Vec::new()
        };
        match flair_platform::install::uninstall(&exe, &dirs) {
            Ok(()) => iced::exit(),
            Err(e) => self.say(
                self.i18n
                    .t_args("uninstall-failed", &[("error", &e.to_string())]),
            ),
        }
    }

    /// Handles the result of background work.
    pub(super) fn on_done(&mut self, done: Done) -> Task<Message> {
        match done {
            Done::Checked(Err(e)) => self.say(self.i18n.t_args("update-failed", &[("error", &e)])),
            Done::Checked(Ok(release)) => match decide(build_info::VERSION, &release.manifest) {
                Ok(Decision::UpToDate) => self.say(
                    self.i18n
                        .t_args("update-up-to-date", &[("version", build_info::VERSION)]),
                ),
                Ok(Decision::Available) => {
                    self.update_available = Some(release.manifest.version.clone());
                    self.set_screen(Some(Screen::UpdateAvailable {
                        release,
                        confirm_reset: false,
                    }))
                }
                Ok(Decision::ManualDownload) => {
                    let mut r = row(
                        "open-releases",
                        self.i18n.t("update-manual"),
                        Some(self.i18n.t("update-manual-hint")),
                        BuiltinIcon::Web,
                    );
                    r.actions = vec![Action::OpenUrl {
                        url: Endpoint::github().releases_page(),
                    }];
                    self.message(vec![r])
                }
                Err(e) => self.say(
                    self.i18n
                        .t_args("update-failed", &[("error", &e.to_string())]),
                ),
            },
            Done::AutoChecked(version) => {
                self.update_available = version;
                Task::none()
            }
            Done::Progress(percent) => {
                if matches!(self.screen, Some(Screen::Downloading { .. })) {
                    self.screen = Some(Screen::Downloading { percent });
                    self.screen_rows = self.compute_screen_rows();
                }
                Task::none()
            }
            // The new version is starting and waits for this process to exit.
            Done::Installed(Ok(())) => iced::exit(),
            Done::Installed(Err(e)) => {
                self.say(self.i18n.t_args("update-failed", &[("error", &e)]))
            }
            Done::SavePicked {
                settings_only,
                path,
            } => {
                self.dialog_open = false;
                let focus = self.refocus();
                let Some(path) = path else {
                    return focus;
                };
                let layout = self.layout();
                let work = background(
                    move || {
                        let origin = archive::Origin::current(build_info::VERSION);
                        archive::export(&layout, &path, settings_only, &origin)
                            .map_err(|e| e.to_string())
                            .map(|_| {
                                let size = std::fs::metadata(&path).map_or(0, |m| m.len());
                                (path, size)
                            })
                    },
                    Done::Exported,
                );
                Task::batch([focus, work])
            }
            Done::Exported(Ok((path, size))) => {
                let shown = path.display().to_string();
                let mut r = row(
                    "export-done",
                    self.i18n
                        .t_args("export-done", &[("path", &shown), ("size", &bytes(size))]),
                    Some(self.i18n.t("export-done-hint")),
                    BuiltinIcon::Folder,
                );
                r.actions = vec![Action::RevealInFolder { path }];
                self.message(vec![r])
            }
            Done::Exported(Err(e)) => self.say(self.i18n.t_args("export-failed", &[("error", &e)])),
            Done::OpenPicked(path) => {
                self.dialog_open = false;
                let focus = self.refocus();
                match path {
                    Some(path) => Task::batch([focus, self.read_import(path)]),
                    None => focus,
                }
            }
            Done::ImportRead(Ok(archive)) => {
                self.set_screen(Some(Screen::ImportSummary { archive }))
            }
            Done::ImportRead(Err(e)) if e == "newer" => self.say(self.i18n.t("import-newer")),
            Done::ImportRead(Err(e)) => {
                self.say(self.i18n.t_args("import-failed", &[("error", &e)]))
            }
            Done::Imported(Ok((applied, backup))) => {
                self.last_import_backup = backup;
                self.reload_config();
                let mut rows = vec![info(self.i18n.t("import-done"), None)];
                rows.extend(applied.renamed.iter().map(|(from, to)| {
                    info(
                        self.i18n
                            .t_args("import-renamed", &[("from", from), ("to", to)]),
                        None,
                    )
                }));
                self.message(rows)
            }
            Done::Imported(Err(e)) => self.say(self.i18n.t_args("import-failed", &[("error", &e)])),
            Done::Undone(Ok(())) => {
                self.reload_config();
                self.say(self.i18n.t("undo-done"))
            }
            Done::Undone(Err(e)) => self.say(self.i18n.t_args("import-failed", &[("error", &e)])),
            Done::InstalledApp(Ok(target)) => match updater::restart(&target, "--after-install") {
                Ok(()) => iced::exit(),
                Err(e) => self.say(
                    self.i18n
                        .t_args("install-failed", &[("error", &e.to_string())]),
                ),
            },
            Done::InstalledApp(Err(e)) => {
                self.say(self.i18n.t_args("install-failed", &[("error", &e)]))
            }
        }
    }
}

fn flair_filter() -> flair_platform::dialog::Filter {
    flair_platform::dialog::Filter {
        name: "Flair".into(),
        extensions: vec!["flair".into()],
    }
}

/// Marker remembering that the user chose portable mode for an executable.
pub fn portable_marker(data_dir: &Path) -> PathBuf {
    data_dir.join("portable")
}

/// True when the installation should be offered for `exe`.
pub fn should_offer_install(exe: &Path, data_dir: &Path) -> bool {
    if build_info::is_dev()
        || flair_platform::install::location(exe) != flair_platform::install::Location::Portable
    {
        return false;
    }
    let accepted = std::fs::read_to_string(portable_marker(data_dir)).unwrap_or_default();
    accepted.trim() != exe.display().to_string()
}

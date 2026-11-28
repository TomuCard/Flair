//! The launcher window: Elm architecture (state, messages, update, view).
//!
//! Searches run as asynchronous tasks on Iced's thread pool, never on the UI
//! thread. Each query has a sequence number; late answers to an older query
//! are dropped, and starting a new query cancels the previous one.

mod screens;
mod view;

pub use screens::{bytes, should_offer_install};

pub use view::INTER;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use flair_core::i18n::I18n;
use flair_core::settings::{Config, ConfigIssue};
use flair_core::theme::Tokens;
use flair_core::{Action, CancellationToken, Provider, ProviderKind, Query, ResultItem};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{Id, operation};
use iced::{Event, Point, Size, Subscription, Task, event, window};

use flair_core::IconSource;
use flair_core::result::BuiltinIcon;
use flair_platform::hotkey::HotkeyError;
use flair_platform::tray::TrayAction;

use crate::design;
use crate::external::{self, External};
use crate::services::Services;
use crate::style::Palette;
use crate::updater;
use screens::{Done, Screen};

/// Id of the search field.
const INPUT_ID: &str = "search";
/// Id of the result list.
const LIST_ID: &str = "results";

pub struct Flair {
    /// The launcher window. Kept alive (hidden) between uses, except where it
    /// cannot be hidden (Wayland): it is then closed and reopened.
    window: Option<window::Id>,
    /// Whether the user sees the window.
    visible: bool,
    services: Services,
    config: Config,
    config_path: PathBuf,
    paths: flair_platform::paths::Paths,
    update_files: updater::Files,
    /// Special list (installation, update, export…) replacing the results.
    screen: Option<Screen>,
    screen_rows: Vec<ResultItem>,
    /// A native dialog is open: losing the focus must not hide Flair.
    dialog_open: bool,
    /// Newer version found by the automatic check.
    update_available: Option<String>,
    /// Backup made before the last import (`> undo`).
    last_import_backup: Option<PathBuf>,
    /// Last setting changed by a command, with its previous value (`> undo`,
    /// before the import backup).
    last_setting: Option<(String, Option<toml::Value>)>,
    /// Words of the `>` commands, from the active languages.
    words: flair_core::command_words::CommandWords,
    /// Input languages available (`> langue`).
    languages: Vec<crate::commands::Language>,
    /// What `Tab` writes for each command result.
    completions: Vec<Option<String>>,
    /// Id of the row waiting for a second `Enter` (shut down, restart…).
    pending_confirm: Option<String>,
    /// A setting is previewed (the look differs from the configuration).
    previewing: bool,
    /// Counts hides: a memory release only applies to the latest one.
    hides: u64,
    i18n: Arc<I18n>,
    tokens: Tokens,
    opacity: f32,
    provider: Arc<dyn Provider>,
    /// Real result sources (`None` in UI tests, which use mock results).
    sources: Option<crate::sources::Sources>,
    /// Image handles of the app icons shown, keyed by their pixel buffer, so
    /// each icon is uploaded to the renderer once.
    icon_handles: view::IconHandles,

    input: String,
    results: Vec<ResultItem>,
    /// Last calculation shown and its input, kept greyed while typing on.
    last_calc: Option<(String, ResultItem)>,
    selected: usize,
    hovered: Option<usize>,
    ctrl_held: bool,
    /// Configuration and shortcut problems, shown when the field is empty.
    notices: Vec<ResultItem>,
    /// Information rows (statistics), shown instead of the notices until hidden.
    info_rows: Vec<ResultItem>,
    /// When the last show was requested, until its first frame is drawn.
    show_requested: Option<Instant>,

    query_seq: u64,
    cancel: CancellationToken,
    /// Vertical scroll offset of the list and height of its viewport.
    scroll_offset: f32,
    was_focused: bool,
    started: Instant,
    dev: Dev,
}

#[derive(Debug, Clone)]
pub enum Message {
    WindowOpened(window::Id),
    /// Native handle of a new window (HWND on Windows), once known.
    WindowReady(window::Id, u64),
    InputChanged(String),
    Results(u64, Vec<ResultItem>),
    Key(Key, Physical, Modifiers),
    ModifiersChanged(Modifiers),
    WindowFocused,
    WindowUnfocused,
    Scrolled(f32),
    Hover(Option<usize>),
    Activate(usize),
    Frame,
    Screenshot(window::Screenshot),
    /// Shortcut, tray menu or another `flair` process.
    External(External),
    /// The window manager asked to close the window (Alt+F4): hide it.
    CloseRequested,
    /// Background work finished (update, export, import…).
    Done(Done),
}

/// Which action of a result to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Primary,
    Secondary,
    /// `Alt+Enter`: raw value (calculator).
    Raw,
}

/// Development options (hidden command line flags).
#[derive(Debug, Clone, Default)]
pub struct Dev {
    pub query: Option<String>,
    pub screenshot: Option<std::path::PathBuf>,
    /// A screenshot will be taken at the next frame.
    pub shot_pending: bool,
}

/// What the list shows first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initial {
    /// The search field.
    Search,
    /// First launch of a portable executable: offer to install.
    Install,
    /// `flair --uninstall` (Windows "Installed apps").
    Uninstall,
    /// `flair file.flair` (double-click): import summary.
    Import(PathBuf),
}

#[derive(Clone)]
pub struct Init {
    pub dev: Dev,
    pub config: Config,
    pub config_path: PathBuf,
    /// Show the window at startup (`--show`, `--toggle`, or a notice to read).
    pub start_visible: bool,
    /// First start on Wayland: explain how to bind a desktop shortcut.
    pub wayland_hint: bool,
    pub paths: flair_platform::paths::Paths,
    /// What to show first.
    pub initial: Initial,
    /// One-off messages (updated, rolled back…), shown at startup.
    pub messages: Vec<String>,
    pub issues: Vec<ConfigIssue>,
    pub i18n: Arc<I18n>,
    pub tokens: Tokens,
    pub provider: Arc<dyn Provider>,
    pub sources: Option<crate::sources::Sources>,
    pub started: Instant,
}

impl Flair {
    pub fn new(init: Init, services: Services) -> (Self, Task<Message>) {
        // One-off messages (installed, updated…): shown once, gone when hidden.
        let messages: Vec<ResultItem> = init
            .messages
            .iter()
            .map(|m| crate::commands::info(m.clone(), None))
            .collect();
        // Lasting problems: shown whenever the field is empty, until fixed.
        let mut notices = notices_for(&init.i18n, &init.issues);
        notices.extend(hotkey_notice(&init, services.hotkey_error.as_ref()));
        // Problems and messages must be seen: show the window if there are any.
        let start_visible = init.start_visible
            || !notices.is_empty()
            || !messages.is_empty()
            || init.initial != Initial::Search;
        let update_files = updater::Files::new(&init.paths.data_dir);
        let lexicon_dir = init.paths.lexicon_dir();
        let words = crate::commands::words(&init.config, &lexicon_dir, init.i18n.lang());
        let languages = crate::commands::languages(&lexicon_dir);
        let mut state = Self {
            window: None,
            visible: false,
            services,
            config_path: init.config_path,
            paths: init.paths,
            update_files,
            screen: None,
            screen_rows: Vec::new(),
            dialog_open: false,
            update_available: None,
            last_import_backup: None,
            last_setting: None,
            words,
            languages,
            completions: Vec::new(),
            pending_confirm: None,
            previewing: false,
            hides: 0,
            i18n: init.i18n,
            tokens: init.tokens,
            #[allow(clippy::cast_possible_truncation)]
            opacity: init.config.theme.opacity as f32,
            config: init.config,
            provider: init.provider,
            sources: init.sources,
            icon_handles: view::IconHandles::default(),
            input: String::new(),
            results: Vec::new(),
            last_calc: None,
            selected: 0,
            hovered: None,
            ctrl_held: false,
            notices,
            info_rows: messages,
            show_requested: None,
            query_seq: 0,
            cancel: CancellationToken::new(),
            scroll_offset: 0.0,
            was_focused: false,
            started: init.started,
            dev: init.dev,
        };
        let initial = match init.initial {
            Initial::Search => Task::none(),
            Initial::Install => state.set_screen(Some(Screen::Install { autostart: true })),
            Initial::Uninstall => state.set_screen(Some(Screen::Uninstall)),
            Initial::Import(path) => state.read_import(path),
        };
        let task = if start_visible {
            Task::batch([initial, state.show()])
        } else if crate::platform_window::can_hide() {
            // Create the window hidden now, so showing it later is instant.
            state.open_window()
        } else {
            // Nothing to wait for: the startup is complete.
            updater::confirm_started(&state.update_files);
            Task::none()
        };
        (state, task)
    }

    pub fn title(&self, _window: window::Id) -> String {
        self.i18n.t("window-title")
    }

    pub fn theme(&self, _window: window::Id) -> iced::Theme {
        Palette::iced_theme(&self.tokens)
    }

    /// Transparent window background: the rounded frame is drawn by the view.
    pub fn style(&self, theme: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: iced::Color::TRANSPARENT,
            text_color: theme.palette().text,
        }
    }

    /// Rows currently displayed: results, or screen / information / notices
    /// when the field is empty.
    fn rows(&self) -> &[ResultItem] {
        if !self.input.trim().is_empty() {
            &self.results
        } else if !self.screen_rows.is_empty() {
            &self.screen_rows
        } else if !self.info_rows.is_empty() {
            &self.info_rows
        } else {
            &self.notices
        }
    }

    pub fn palette(&self) -> Palette {
        Palette::new(&self.tokens, self.opacity)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Windows are always created hidden: first disable the OS open and
            // close animations, then show the window if it is wanted.
            Message::WindowOpened(id) => {
                if self.window != Some(id) {
                    return Task::none();
                }
                window::raw_id::<Message>(id).map(move |raw| Message::WindowReady(id, raw))
            }
            Message::WindowReady(id, raw) => {
                flair_platform::window::disable_animations(raw);
                // Window ready and shortcut registered: a fresh update works.
                updater::confirm_started(&self.update_files);
                if !self.visible || self.window != Some(id) {
                    return Task::none(); // kept hidden and warm
                }
                tracing::info!(
                    since_start_ms = self.started.elapsed().as_millis(),
                    "window opened"
                );
                let mut tasks = vec![
                    window::set_mode(id, window::Mode::Windowed),
                    window::gain_focus(id),
                    operation::focus(Id::new(INPUT_ID)),
                ];
                if let Some(query) = self.dev.query.clone() {
                    tasks.push(self.search(query));
                } else if self.dev.screenshot.is_some() {
                    self.dev.shot_pending = true;
                }
                Task::batch(tasks)
            }
            Message::InputChanged(value) => self.search(value),
            Message::Results(seq, results) => {
                tracing::debug!(seq, count = results.len(), "results");
                if seq != self.query_seq {
                    return Task::none(); // obsolete answer
                }
                self.results = results;
                self.keep_last_calc();
                // Monochrome icons: no handle, the generic tinted icon is drawn.
                if self.config.theme.icons == flair_core::settings::config::IconStyle::Color {
                    self.icon_handles.update(&self.results);
                } else {
                    self.icon_handles.update(&[]);
                }
                self.selected = 0;
                self.hovered = None;
                self.dev.shot_pending = self.dev.screenshot.is_some();
                Task::batch([self.resize(), self.scroll_to(0.0)])
            }
            Message::Key(key, physical, modifiers) => self.on_key(&key, physical, modifiers),
            Message::ModifiersChanged(modifiers) => {
                self.ctrl_held = modifiers.control();
                Task::none()
            }
            Message::WindowFocused => {
                tracing::debug!("focused");
                self.was_focused = true;
                Task::none()
            }
            Message::WindowUnfocused => {
                tracing::debug!("unfocused");
                // Only after a first focus: some compositors send a spurious
                // unfocus while the window is being mapped.
                if self.was_focused && !self.dialog_open {
                    self.hide()
                } else {
                    Task::none()
                }
            }
            Message::Scrolled(offset) => {
                self.scroll_offset = offset;
                Task::none()
            }
            Message::Hover(index) => {
                self.hovered = index;
                Task::none()
            }
            Message::Activate(index) => self.activate(index, Which::Primary),
            Message::Screenshot(shot) => {
                tracing::debug!(size = ?shot.size, "screenshot");
                if let Some(path) = &self.dev.screenshot {
                    let png =
                        crate::png::encode_rgba(shot.size.width, shot.size.height, &shot.rgba);
                    if let Err(e) = std::fs::write(path, png) {
                        tracing::error!(path = %path.display(), "screenshot: {e}");
                    }
                }
                self.hide()
            }
            Message::Frame if self.dev.shot_pending => {
                self.dev.shot_pending = false;
                match self.window {
                    Some(id) => window::screenshot(id).map(Message::Screenshot),
                    None => Task::none(),
                }
            }
            Message::Frame => {
                if let Some(requested) = self.show_requested.take() {
                    let elapsed = requested.elapsed();
                    crate::stats::record_show(elapsed);
                    tracing::debug!(ms = elapsed.as_secs_f64() * 1000.0, "shown");
                }
                Task::none()
            }
            Message::External(event) => self.on_external(event),
            Message::CloseRequested => self.hide(),
            Message::Done(done) => self.on_done(done),
        }
    }

    fn on_external(&mut self, event: External) -> Task<Message> {
        // Rare events, worth a line in the log (helps diagnosing the tray
        // and the global shortcut on a user's machine).
        tracing::info!(?event, visible = self.visible, "external event");
        match event {
            External::Show => self.show(),
            External::Toggle => {
                if self.visible {
                    self.hide()
                } else {
                    self.show()
                }
            }
            External::Quit => iced::exit(),
            External::Uninstall => {
                Task::batch([self.set_screen(Some(Screen::Uninstall)), self.show()])
            }
            External::Reload => {
                self.reload_config();
                Task::none()
            }
            External::Open(path) => Task::batch([self.read_import(path), self.show()]),
            External::Tray(action) => self.on_tray(action),
            External::Release(hide) => {
                if !self.visible && hide == self.hides {
                    self.release_memory();
                }
                Task::none()
            }
            External::SystemTheme(dark) => {
                tracing::info!(dark, "system theme changed");
                crate::themes::set_system_dark(Some(dark));
                self.apply_look();
                Task::none()
            }
        }
    }

    fn on_tray(&mut self, action: TrayAction) -> Task<Message> {
        match action {
            TrayAction::Toggle => self.on_external(External::Toggle),
            TrayAction::Show => self.show(),
            TrayAction::OpenConfig => {
                open(&self.config_path);
                Task::none()
            }
            TrayAction::Autostart => {
                self.toggle_autostart();
                Task::none()
            }
            TrayAction::Stats => {
                let snapshot = crate::stats::snapshot();
                self.info_rows = stats_rows(&self.i18n, &snapshot);
                self.input.clear();
                self.results.clear();
                Task::batch([self.show(), self.resize()])
            }
            TrayAction::Quit => iced::exit(),
            TrayAction::CheckUpdate => Task::batch([self.show(), self.run_command("update")]),
            TrayAction::Reindex => {
                if let Some(sources) = &self.sources {
                    sources.apps.refresh();
                    sources.files.reindex();
                }
                Task::none()
            }
        }
    }

    /// Tray "Launch at startup": register with the OS, then save the setting.
    fn toggle_autostart(&mut self) {
        let enabled = !self.config.autostart;
        let result =
            std::env::current_exe().and_then(|exe| flair_platform::autostart::set(enabled, &exe));
        match result.map_err(|e| e.to_string()).and_then(|()| {
            flair_core::settings::edit::set_in_file(
                &self.config_path,
                "autostart",
                &toml::Value::Boolean(enabled),
            )
            .map_err(|e| e.to_string())
        }) {
            Ok(()) => {
                self.config.autostart = enabled;
                tracing::info!(enabled, "launch at startup changed");
            }
            Err(e) => tracing::error!("cannot change launch at startup: {e}"),
        }
        if let Some(tray) = &mut self.services.tray {
            tray.set_autostart_checked(self.config.autostart);
        }
    }

    /// Shows the window on the screen under the mouse, field focused.
    fn show(&mut self) -> Task<Message> {
        if self.visible {
            return match self.window {
                Some(id) => {
                    Task::batch([window::gain_focus(id), operation::focus(Id::new(INPUT_ID))])
                }
                None => Task::none(),
            };
        }
        self.visible = true;
        crate::stats::set_visible(true);
        // A pending memory release is dropped; the efficiency mode ends.
        self.hides += 1;
        if self.config.resources.efficiency_mode {
            flair_platform::memory::wake();
        }
        if let Some(sources) = &self.sources {
            sources.files.on_show();
        }
        self.show_requested = Some(Instant::now());
        self.was_focused = false;
        let auto_check = self.maybe_auto_check();
        let keep = self.config.keep_last_query && !self.input.is_empty();
        if !keep && self.screen.is_none() {
            self.input.clear();
            self.results.clear();
            self.selected = 0;
        }

        let Some(id) = self.window else {
            return Task::batch([self.open_window(), auto_check]);
        };
        let mut tasks = Vec::new();
        if let Some(position) = position_under_mouse() {
            tasks.push(window::move_to(id, position));
        }
        tasks.push(self.resize());
        tasks.push(window::set_mode(id, window::Mode::Windowed));
        tasks.push(window::gain_focus(id));
        tasks.push(operation::focus(Id::new(INPUT_ID)));
        if keep {
            tasks.push(operation::select_all(Id::new(INPUT_ID)));
        }
        tasks.push(auto_check);
        Task::batch(tasks)
    }

    /// Gives the focus back to the window (after a native dialog).
    fn refocus(&self) -> Task<Message> {
        match self.window {
            Some(id) => Task::batch([window::gain_focus(id), operation::focus(Id::new(INPUT_ID))]),
            None => Task::none(),
        }
    }

    /// Re-reads `config.toml` and applies it (after an import or undo).
    fn reload_config(&mut self) {
        let text = std::fs::read_to_string(&self.config_path).unwrap_or_default();
        let loaded = flair_core::settings::parse_config(&text);
        let invalid_themes = crate::themes::reload(&self.paths.themes_dir());
        // A file that cannot be read keeps the last valid configuration.
        let config = if loaded.is_syntax_error() {
            self.config.clone()
        } else {
            loaded.config
        };
        if config.hotkey != self.config.hotkey {
            self.services.reregister_hotkey(&config.hotkey);
        }
        let lang = flair_core::i18n::Lang::resolve(config.ui_language);
        if lang != self.i18n.lang() {
            self.i18n = Arc::new(I18n::new(lang));
        }
        if let Some(sources) = &self.sources {
            sources.reload(&config, &self.i18n);
            self.provider = sources.providers(&self.i18n, &config);
        }
        self.notices = notices_for(&self.i18n, &loaded.issues);
        for (file, error) in invalid_themes {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.notices.push(crate::commands::info(
                self.i18n.t_args("theme-invalid", &[("file", &name)]),
                Some(error),
            ));
        }
        let lexicon_dir = self.paths.lexicon_dir();
        self.words = crate::commands::words(&config, &lexicon_dir, self.i18n.lang());
        self.languages = crate::commands::languages(&lexicon_dir);
        self.config = config;
        self.apply_look();
        tracing::info!("configuration reloaded");
    }

    fn open_window(&mut self) -> Task<Message> {
        let height = if crate::platform_window::can_resize() {
            self.frame_height()
        } else {
            design::max_window_height()
        };
        let (id, open) = window::open(window_settings(height));
        self.window = Some(id);
        open.map(Message::WindowOpened)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Both sleep until an event arrives: no wake-up while hidden.
        let mut subscriptions = vec![
            event::listen_with(on_event),
            external::subscription().map(Message::External),
        ];
        // A frame is only requested right after a show (to measure it), never
        // at rest.
        if self.show_requested.is_some() {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        // Development screenshots wait for the next frame, so the view is up to date.
        if self.dev.shot_pending {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        Subscription::batch(subscriptions)
    }

    fn search(&mut self, value: String) -> Task<Message> {
        if !value.is_empty() {
            // Typing leaves any screen.
            self.screen = None;
            self.screen_rows.clear();
        }
        self.input = value;
        self.pending_confirm = None;
        self.completions.clear();
        self.cancel.cancel();
        self.cancel = CancellationToken::new();
        self.query_seq += 1;

        let query = Query::parse(&self.input);
        if query.filter == Some(ProviderKind::Command) {
            // Flair's own commands: computed at once, no search needed.
            let portable = std::env::current_exe().is_ok_and(|exe| {
                flair_platform::install::location(&exe)
                    == flair_platform::install::Location::Portable
            });
            let cx = crate::commands::Context {
                i18n: &self.i18n,
                words: &self.words,
                config: &self.config,
                languages: &self.languages,
                portable,
                update_available: self.update_available.as_deref(),
            };
            let rows = crate::commands::rows(&query.text, &cx);
            self.completions = rows.iter().map(|r| r.completion.clone()).collect();
            self.results = rows.into_iter().map(|r| r.item).collect();
            self.selected = 0;
            self.update_preview();
            self.dev.shot_pending = self.dev.screenshot.is_some();
            return Task::batch([self.resize(), self.scroll_to(0.0)]);
        }
        self.end_preview();
        // `=` alone shows the calculation history; otherwise nothing to search.
        if query.is_empty() && query.filter != Some(ProviderKind::Calculator) {
            self.results.clear();
            self.last_calc = None;
            self.selected = 0;
            return self.resize();
        }
        let provider = Arc::clone(&self.provider);
        let token = self.cancel.clone();
        let seq = self.query_seq;
        Task::perform(
            async move { provider.query(&query, &token) },
            move |results| Message::Results(seq, results),
        )
    }

    fn on_key(&mut self, key: &Key, physical: Physical, modifiers: Modifiers) -> Task<Message> {
        let ctrl = modifiers.control();
        let len = self.rows().len();
        match key.as_ref() {
            Key::Named(Named::ArrowDown) => self.select(self.selected.saturating_add(1)),
            Key::Named(Named::ArrowUp) => self.select(self.selected.saturating_sub(1)),
            Key::Character("n") if ctrl => self.select(self.selected.saturating_add(1)),
            Key::Character("p") if ctrl => self.select(self.selected.saturating_sub(1)),
            Key::Named(Named::Enter) if len > 0 => {
                let which = if ctrl {
                    Which::Secondary
                } else if modifiers.alt() {
                    Which::Raw
                } else {
                    Which::Primary
                };
                self.activate(self.selected, which)
            }
            Key::Named(Named::Tab) => self.complete(),
            Key::Character("c") if ctrl && len > 0 => {
                let text = self.rows()[self.selected].copy_text();
                iced::clipboard::write(text)
            }
            Key::Named(Named::Escape) => {
                if self.input.is_empty() && self.screen.is_some() {
                    self.set_screen(None)
                } else if self.input.is_empty() {
                    self.hide()
                } else {
                    // The text field drops its focus on Escape: take it back.
                    Task::batch([
                        self.search(String::new()),
                        operation::focus(Id::new(INPUT_ID)),
                    ])
                }
            }
            // Ctrl+1…9 by physical key, so it works on AZERTY keyboards too
            // (where the digit row types & é " ' ( - è _ ç).
            _ if ctrl => match digit(physical) {
                Some(n) if n <= len => self.activate(n - 1, Which::Primary),
                _ => Task::none(),
            },
            _ => Task::none(),
        }
    }

    fn select(&mut self, index: usize) -> Task<Message> {
        let len = self.rows().len();
        if len == 0 {
            return Task::none();
        }
        self.selected = index.min(len - 1);
        self.pending_confirm = None;
        self.update_preview();
        self.ensure_visible()
    }

    /// Scrolls the list so the selected row is visible.
    fn ensure_visible(&self) -> Task<Message> {
        let heights: Vec<f32> = self.rows().iter().map(view::row_height).collect();
        let top: f32 = heights[..self.selected].iter().sum();
        let bottom = top + heights[self.selected];
        let viewport =
            design::list_viewport_height(heights.iter().copied()) - 2.0 * design::LIST_PADDING;
        if top < self.scroll_offset {
            self.scroll_to(top)
        } else if bottom > self.scroll_offset + viewport {
            self.scroll_to(bottom - viewport)
        } else {
            Task::none()
        }
    }

    fn scroll_to(&self, y: f32) -> Task<Message> {
        operation::scroll_to(
            Id::new(LIST_ID),
            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
        )
    }

    /// `Tab`: fills the field with the selected result (its path for files).
    fn complete(&mut self) -> Task<Message> {
        let Some(item) = self.rows().get(self.selected) else {
            return Task::none();
        };
        let showing_results = std::ptr::eq(self.rows().as_ptr(), self.results.as_ptr());
        let command_completion = showing_results
            .then(|| self.completions.get(self.selected).cloned().flatten())
            .flatten();
        let text = match (item.provider, item.primary_action()) {
            (ProviderKind::File, _) => file_completion(&item.copy_text(), &self.input),
            // Commands keep their `>` prefix: `> ver` + Tab gives `> version`.
            (ProviderKind::Command | ProviderKind::System, _) if command_completion.is_some() => {
                command_completion.unwrap_or_default()
            }
            // Information rows (notices, statistics) cannot be completed.
            (ProviderKind::Command, None) => return Task::none(),
            _ => item.title.clone(),
        };
        Task::batch([
            self.search(text),
            operation::move_cursor_to_end(Id::new(INPUT_ID)),
        ])
    }

    fn activate(&mut self, index: usize, which: Which) -> Task<Message> {
        let Some(item) = self.rows().get(index).cloned() else {
            return Task::none();
        };
        let action = match which {
            Which::Primary => item.primary_action(),
            Which::Secondary => item.secondary_action().or(item.primary_action()),
            // `Alt+Enter`: the raw value (calculator: third action).
            Which::Raw => item.actions.get(2).or(item.primary_action()),
        };
        let Some(action) = action.cloned() else {
            return Task::none();
        };
        // Shut down, restart, log out, empty the trash, clear the history:
        // a second `Enter` confirms.
        if crate::commands::needs_confirmation(&action)
            && self.pending_confirm.as_deref() != Some(item.id.as_str())
        {
            let id = item.id.clone();
            let hint = self.i18n.t("confirm-again");
            if let Some(row) = self.results.iter_mut().find(|r| r.id == id) {
                row.subtitle = Some(hint);
            }
            self.pending_confirm = Some(id);
            return Task::none();
        }
        self.pending_confirm = None;
        tracing::info!(id = %item.id, ?action, "activate");
        // Copied calculations go to the history (`=` alone lists them).
        if item.provider == ProviderKind::Calculator
            && matches!(action, Action::CopyToClipboard { .. })
            && let Some(sources) = &self.sources
        {
            let typed = Query::parse(&self.input).text;
            let input = if typed.is_empty() {
                item.subtitle.clone().unwrap_or_default()
            } else {
                typed
            };
            sources.record_calculation(&input);
        }
        if matches!(
            action,
            Action::Launch { .. } | Action::LaunchAsAdmin { .. } | Action::Open { .. }
        ) && let Some(sources) = &self.sources
        {
            sources.record_launch(&item.id);
        }
        match action {
            Action::CopyToClipboard { text } => {
                let text = if which == Which::Raw {
                    text.replace(',', ".")
                } else {
                    text
                };
                Task::batch([iced::clipboard::write(text), self.hide()])
            }
            Action::Open { path } => {
                open(&path);
                self.hide()
            }
            Action::RevealInFolder { path } => {
                if let Err(e) = flair_platform::open::reveal_path(&path) {
                    tracing::error!(path = %path.display(), "cannot show in its folder: {e}");
                }
                self.hide()
            }
            Action::OpenUrl { url } => {
                open(std::path::Path::new(&url));
                self.hide()
            }
            Action::Internal { command } => self.run_command(&command),
            Action::Launch { target } => {
                launch(target, false);
                self.hide()
            }
            Action::LaunchAsAdmin { target } => {
                launch(target, true);
                self.hide()
            }
            Action::SystemCommand(command) => {
                // Off the interface thread: logind or the shell may take time.
                std::thread::spawn(move || {
                    if let Err(e) = flair_platform::system::run(command) {
                        tracing::error!(?command, "system command failed: {e}");
                    }
                });
                self.hide()
            }
        }
    }

    /// While a calculation is being typed (`5h36 + 6`), an input that does
    /// not compute yet keeps the last valid result, greyed.
    fn keep_last_calc(&mut self) {
        let text = Query::parse(&self.input).text;
        let main = flair_providers::calc::MAIN_ID;
        if let Some(item) = self.results.iter().find(|r| r.id == main) {
            // An error (`Cannot divide by zero`) is shown, never kept.
            if !item.actions.is_empty() {
                self.last_calc = Some((text, item.clone()));
            }
            return;
        }
        let continues = self.last_calc.as_ref().is_some_and(|(previous, _)| {
            text.starts_with(previous.as_str()) && text.len() > previous.len()
        });
        if continues && flair_providers::calc::looks_like_calculation(&text) {
            if let Some((_, item)) = &self.last_calc {
                let mut stale = item.clone();
                stale.id = flair_providers::calc::STALE_ID.into();
                self.results.insert(0, stale);
            }
        } else {
            self.last_calc = None;
        }
    }

    /// Hides the window. Flair keeps running in the background.
    fn hide(&mut self) -> Task<Message> {
        if !self.visible {
            return Task::none();
        }
        tracing::debug!("hide");
        self.visible = false;
        crate::stats::set_visible(false);
        self.cancel.cancel();
        self.show_requested = None;
        self.info_rows.clear();
        // Results of actions are not shown again; ongoing ones keep their screen.
        if matches!(self.screen, Some(Screen::Rows(_))) {
            self.screen = None;
            self.screen_rows.clear();
        }
        self.hovered = None;
        self.ctrl_held = false;
        self.pending_confirm = None;
        self.end_preview();
        self.schedule_release();
        if !self.config.keep_last_query {
            self.input.clear();
            self.results.clear();
            self.last_calc = None;
        }
        // Development screenshots: one capture, then quit.
        if self.dev.screenshot.is_some() {
            return iced::exit();
        }
        let Some(id) = self.window else {
            return Task::none();
        };
        if crate::platform_window::can_hide() {
            window::set_mode(id, window::Mode::Hidden)
        } else {
            self.window = None;
            window::close(id)
        }
    }

    /// Height of the visible frame for the current rows.
    /// Previews the setting of the selected row when it only changes the
    /// look (`> config theme.mode dark`); `Enter` keeps it, anything else
    /// goes back.
    fn update_preview(&mut self) {
        let command = std::ptr::eq(self.rows().as_ptr(), self.results.as_ptr())
            .then(|| self.results.get(self.selected))
            .flatten()
            .and_then(|row| match row.primary_action() {
                Some(Action::Internal { command }) => Some(command.clone()),
                _ => None,
            });
        // A theme of `> thème`, listed or being created.
        if let Some(theme) = command
            .as_deref()
            .and_then(crate::commands::theme::previewed)
        {
            self.tokens = theme.tokens();
            self.previewing = true;
            return;
        }
        let preview = command
            .as_deref()
            .and_then(crate::commands::settings::decode_set)
            .filter(|(key, _)| crate::commands::settings::PREVIEWED.contains(&key.as_str()))
            .and_then(|(key, value)| {
                crate::commands::settings::with_value(&self.config, &key, &value)
            });
        match preview {
            Some(config) => {
                self.tokens = crate::themes::active(&config).tokens();
                #[allow(clippy::cast_possible_truncation)]
                {
                    self.opacity = config.theme.opacity as f32;
                }
                self.previewing = true;
            }
            None => self.end_preview(),
        }
    }

    /// The look of the configuration (theme, system preference, opacity),
    /// and the tray icon in its color.
    fn apply_look(&mut self) {
        self.previewing = false;
        self.tokens = crate::themes::active(&self.config).tokens();
        #[allow(clippy::cast_possible_truncation)]
        {
            self.opacity = self.config.theme.opacity as f32;
        }
        if let Some(tray) = &mut self.services.tray
            && let Ok(icon) = crate::services::tray_icon(&self.config, &self.tokens)
        {
            tray.set_icon(icon);
        }
    }

    /// One wake-up, `trim_memory_after` seconds after hiding (0: never).
    fn schedule_release(&mut self) {
        self.hides += 1;
        let delay = self.config.resources.trim_memory_after;
        if delay == 0 || self.dev.screenshot.is_some() {
            return;
        }
        let hide = self.hides;
        let spawned = std::thread::Builder::new()
            .name("release-memory".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(u64::from(delay)));
                crate::external::send(External::Release(hide));
            });
        if let Err(e) = spawned {
            tracing::debug!(error = %e, "cannot schedule the memory release");
        }
    }

    /// Hidden for a while: caches emptied, memory given back to the system.
    fn release_memory(&mut self) {
        let before = flair_platform::memory::resident_bytes();
        self.icon_handles = view::IconHandles::default();
        if !self.config.keep_last_query {
            self.results = Vec::new();
            self.completions = Vec::new();
        }
        self.screen_rows.shrink_to_fit();
        flair_platform::memory::release(self.config.resources.efficiency_mode);
        let after = flair_platform::memory::resident_bytes();
        tracing::info!(
            before_mb = before.map(|b| b / 1_048_576),
            after_mb = after.map(|b| b / 1_048_576),
            "memory released"
        );
    }

    /// Back to the look of the configuration.
    fn end_preview(&mut self) {
        if self.previewing {
            self.tokens = crate::themes::active(&self.config).tokens();
            #[allow(clippy::cast_possible_truncation)]
            {
                self.opacity = self.config.theme.opacity as f32;
            }
            self.previewing = false;
        }
    }

    fn frame_height(&self) -> f32 {
        design::window_height(self.rows().iter().map(view::row_height))
    }

    fn resize(&self) -> Task<Message> {
        let Some(id) = self.window else {
            return Task::none();
        };
        if !crate::platform_window::can_resize() {
            // Fixed-size window: the view draws the frame at the right height.
            return Task::none();
        }
        window::resize(id, Size::new(design::WINDOW_WIDTH, self.frame_height()))
    }
}

/// Keyboard and window events, filtered without capturing state.
fn on_event(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    crate::stats::count_wakeup();
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) => Some(Message::Key(key, physical_key, modifiers)),
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
        }
        Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
        Event::Window(window::Event::Unfocused) => Some(Message::WindowUnfocused),
        Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested),
        _ => None,
    }
}

/// Digit 1–9 of a physical key (main row or numeric keypad).
fn digit(physical: Physical) -> Option<usize> {
    let Physical::Code(code) = physical else {
        return None;
    };
    Some(match code {
        Code::Digit1 | Code::Numpad1 => 1,
        Code::Digit2 | Code::Numpad2 => 2,
        Code::Digit3 | Code::Numpad3 => 3,
        Code::Digit4 | Code::Numpad4 => 4,
        Code::Digit5 | Code::Numpad5 => 5,
        Code::Digit6 | Code::Numpad6 => 6,
        Code::Digit7 | Code::Numpad7 => 7,
        Code::Digit8 | Code::Numpad8 => 8,
        Code::Digit9 | Code::Numpad9 => 9,
        _ => return None,
    })
}

/// Rows explaining configuration problems (shown while the field is empty).
fn notices_for(i18n: &I18n, issues: &[ConfigIssue]) -> Vec<ResultItem> {
    issues
        .iter()
        .enumerate()
        .map(|(i, issue)| {
            let detail = match issue {
                ConfigIssue::Io(e) => i18n.t_args("config-error-io", &[("error", e)]),
                ConfigIssue::Syntax(e) => i18n.t_args("config-error-syntax", &[("error", e)]),
                ConfigIssue::InvalidValue { key, error } => i18n.t_args(
                    "config-error-invalid",
                    &[("key", key), ("error", &i18n.value_error(error))],
                ),
                ConfigIssue::UnknownKey(key) => {
                    i18n.t_args("config-error-unknown", &[("key", key)])
                }
            };
            ResultItem {
                id: format!("notice:{i}"),
                title: i18n.t("config-error-title"),
                subtitle: Some(detail),
                icon: IconSource::Builtin(BuiltinIcon::Settings),
                score: 0.0,
                match_indices: vec![],
                actions: vec![],
                provider: ProviderKind::Command,
            }
        })
        .collect()
}

/// Starts an application off the UI thread (the elevation prompt of Windows
/// may take a while); errors are logged.
fn launch(target: flair_core::LaunchTarget, elevated: bool) {
    let spawned = std::thread::Builder::new()
        .name("launch".into())
        .spawn(move || {
            if let Err(e) = flair_platform::apps::launch(&target, elevated) {
                tracing::error!(?target, elevated, "cannot launch: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start a launch thread: {e}");
    }
}

/// Opens a file, folder or URL with the default application; errors are logged.
/// `Tab` on a file: its path, with `~` kept when typed, and a separator after
/// a folder so that the next `Tab` goes inside.
fn file_completion(path: &str, typed: &str) -> String {
    let mut text = path.to_owned();
    if typed.trim_start().starts_with('~')
        && let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_owned())
        && let Ok(rest) = std::path::Path::new(path).strip_prefix(&home)
    {
        text = if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
        };
    }
    if std::path::Path::new(path).is_dir() && !text.ends_with(['/', '\\']) {
        text.push(std::path::MAIN_SEPARATOR);
    }
    text
}

fn open(path: &std::path::Path) {
    if let Err(e) = flair_platform::open::open_path(path) {
        tracing::error!(path = %path.display(), "cannot open: {e}");
    }
}

/// Top-left corner placing the window centered, at 25 % of the height of the
/// screen under the mouse (logical coordinates). `None` where unknown.
fn position_under_mouse() -> Option<Point> {
    let area = flair_platform::window::cursor_work_area()?;
    #[allow(clippy::cast_precision_loss)]
    let (x, y, w, h) = (
        area.x as f32,
        area.y as f32,
        area.width as f32,
        area.height as f32,
    );
    let width = design::WINDOW_WIDTH * area.scale;
    Some(Point::new(
        (x + (w - width) / 2.0) / area.scale,
        (y + h * design::WINDOW_TOP_FRACTION) / area.scale,
    ))
}

/// Row explaining why the global shortcut is not available, if needed.
fn hotkey_notice(init: &Init, error: Option<&HotkeyError>) -> Option<ResultItem> {
    let i18n = &init.i18n;
    let hotkey = init.config.hotkey.as_str();
    let open_config = Action::Open {
        path: init.config_path.clone(),
    };
    let (title, subtitle, action) = match error? {
        HotkeyError::Taken(_) => (
            i18n.t_args("hotkey-taken-title", &[("hotkey", hotkey)]),
            i18n.t("hotkey-taken-detail"),
            open_config,
        ),
        HotkeyError::Failed(_, e) => (
            i18n.t_args("hotkey-failed-title", &[("hotkey", hotkey)]),
            i18n.t_args("hotkey-failed-detail", &[("error", e)]),
            open_config,
        ),
        HotkeyError::Unsupported if init.wayland_hint => (
            i18n.t("hotkey-wayland-title"),
            i18n.t("hotkey-wayland-detail"),
            Action::CopyToClipboard {
                text: toggle_command(),
            },
        ),
        HotkeyError::Unsupported => return None,
    };
    Some(ResultItem {
        id: "notice:hotkey".into(),
        title,
        subtitle: Some(subtitle),
        icon: IconSource::Builtin(BuiltinIcon::Settings),
        score: 0.0,
        match_indices: vec![],
        actions: vec![action],
        provider: ProviderKind::Command,
    })
}

/// Command to bind to a desktop shortcut: `<path to flair> --toggle`.
fn toggle_command() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "flair".into());
    if exe.contains(' ') {
        format!("\"{exe}\" --toggle")
    } else {
        format!("{exe} --toggle")
    }
}

/// Statistics as information rows (title = value, subtitle = label).
fn stats_rows(i18n: &I18n, snapshot: &crate::stats::Snapshot) -> Vec<ResultItem> {
    crate::stats::lines(snapshot, i18n)
        .into_iter()
        .enumerate()
        .map(|(i, (label, value))| ResultItem {
            id: format!("stats:{i}"),
            title: value.clone(),
            subtitle: Some(label),
            icon: IconSource::Builtin(BuiltinIcon::Settings),
            score: 0.0,
            match_indices: vec![],
            actions: vec![Action::CopyToClipboard { text: value }],
            provider: ProviderKind::Command,
        })
        .collect()
}

fn window_settings(height: f32) -> window::Settings {
    window::Settings {
        size: Size::new(design::WINDOW_WIDTH, height),
        // Centered horizontally, at 25 % of the height of the primary screen.
        // Moved to the screen under the mouse when shown (where supported).
        position: window::Position::SpecificWith(|window, monitor| {
            Point::new(
                ((monitor.width - window.width) / 2.0).max(0.0),
                monitor.height * design::WINDOW_TOP_FRACTION,
            )
        }),
        // Shown once the OS animations are disabled (see `WindowReady`).
        visible: false,
        resizable: crate::platform_window::RESIZABLE,
        minimizable: false,
        decorations: false,
        transparent: true,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        platform_specific: crate::platform_window::settings(),
        ..window::Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;
    use flair_core::i18n::Lang;
    use flair_core::theme::builtin;

    fn app() -> Flair {
        let i18n = Arc::new(I18n::new(Lang::Fr));
        let init = Init {
            dev: Dev::default(),
            config: Config::default(),
            config_path: PathBuf::from("config.toml"),
            start_visible: true,
            wayland_hint: false,
            paths: flair_platform::paths::Paths::rooted(
                &std::env::temp_dir().join("flair-ui-tests"),
            ),
            initial: Initial::Search,
            messages: vec![],
            issues: vec![],
            i18n: Arc::clone(&i18n),
            tokens: builtin::dark().tokens(),
            provider: Arc::new(MockProvider::new(i18n, 8)),
            sources: None,
            started: Instant::now(),
        };
        let app = Flair::new(init, Services::none()).0;
        assert!(app.visible);
        app
    }

    /// Types `text` and delivers the results synchronously.
    fn type_text(app: &mut Flair, text: &str) {
        let _ = app.update(Message::InputChanged(text.into()));
        let results = app
            .provider
            .query(&Query::parse(text), &CancellationToken::new());
        let _ = app.update(Message::Results(app.query_seq, results));
    }

    fn press(app: &mut Flair, key: Key, code: Code, modifiers: Modifiers) {
        let _ = app.update(Message::Key(key, Physical::Code(code), modifiers));
    }

    fn named(app: &mut Flair, named: Named, code: Code) {
        press(app, Key::Named(named), code, Modifiers::empty());
    }

    #[test]
    fn last_calculation_stays_greyed_while_typing() {
        let mut app = app();
        type_text(&mut app, "15% de 240");
        assert_eq!(app.results[0].id, flair_providers::calc::MAIN_ID);
        // Not a calculation yet: the last result stays, greyed.
        type_text(&mut app, "15% de 240 +");
        assert_eq!(app.results[0].id, flair_providers::calc::STALE_ID);
        assert_eq!(app.results[0].title, "36");
        // Another input: gone.
        type_text(&mut app, "firefox");
        assert!(
            app.results
                .iter()
                .all(|r| r.id != flair_providers::calc::STALE_ID)
        );
    }

    #[test]
    fn obsolete_results_are_ignored() {
        let mut app = app();
        let _ = app.update(Message::InputChanged("v".into()));
        let old_seq = app.query_seq;
        let _ = app.update(Message::InputChanged("vsc".into()));
        let stale = app
            .provider
            .query(&Query::parse("v"), &CancellationToken::new());
        let _ = app.update(Message::Results(old_seq, stale));
        assert!(app.results.is_empty(), "stale answer displayed");
        let fresh = app
            .provider
            .query(&Query::parse("vsc"), &CancellationToken::new());
        let _ = app.update(Message::Results(app.query_seq, fresh));
        assert_eq!(app.results[0].title, "Visual Studio Code");
    }

    #[test]
    fn new_query_cancels_the_previous_one() {
        let mut app = app();
        let _ = app.update(Message::InputChanged("v".into()));
        let first = app.cancel.clone();
        let _ = app.update(Message::InputChanged("vs".into()));
        assert!(first.is_cancelled());
        assert!(!app.cancel.is_cancelled());
    }

    #[test]
    fn arrows_and_ctrl_n_p_move_the_selection_within_bounds() {
        let mut app = app();
        type_text(&mut app, "vsc"); // app, file, web
        assert_eq!(app.results.len(), 3);
        named(&mut app, Named::ArrowDown, Code::ArrowDown);
        assert_eq!(app.selected, 1);
        named(&mut app, Named::ArrowDown, Code::ArrowDown);
        named(&mut app, Named::ArrowDown, Code::ArrowDown);
        assert_eq!(app.selected, 2, "stops at the last row");
        press(
            &mut app,
            Key::Character("p".into()),
            Code::KeyP,
            Modifiers::CTRL,
        );
        assert_eq!(app.selected, 1);
        named(&mut app, Named::ArrowUp, Code::ArrowUp);
        named(&mut app, Named::ArrowUp, Code::ArrowUp);
        assert_eq!(app.selected, 0, "stops at the first row");
        press(
            &mut app,
            Key::Character("n".into()),
            Code::KeyN,
            Modifiers::CTRL,
        );
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn escape_clears_then_hides() {
        let mut app = app();
        type_text(&mut app, "vsc");
        named(&mut app, Named::Escape, Code::Escape);
        assert!(app.input.is_empty());
        assert!(app.results.is_empty());
        assert!(app.visible, "first Escape only clears");
        named(&mut app, Named::Escape, Code::Escape);
        assert!(!app.visible, "second Escape hides");
    }

    #[test]
    fn ctrl_digit_uses_the_physical_key_and_checks_bounds() {
        let mut app = app();
        type_text(&mut app, "vsc");
        // AZERTY: Ctrl + the "é" key is Ctrl+2.
        press(
            &mut app,
            Key::Character("é".into()),
            Code::Digit9,
            Modifiers::CTRL,
        );
        assert!(app.visible, "no 9th result: nothing happens");
        press(
            &mut app,
            Key::Character("é".into()),
            Code::Digit2,
            Modifiers::CTRL,
        );
        assert!(!app.visible, "2nd result launched, window hidden");
    }

    #[test]
    fn tab_completes_title_or_path() {
        let mut app = app();
        type_text(&mut app, "vsc");
        named(&mut app, Named::Tab, Code::Tab);
        assert_eq!(app.input, "Visual Studio Code");

        type_text(&mut app, "vsc");
        named(&mut app, Named::ArrowDown, Code::ArrowDown);
        named(&mut app, Named::Tab, Code::Tab);
        assert!(app.input.ends_with("vsc-backup.zip"), "{}", app.input);
        assert!(
            app.input.starts_with("~/Documents/archives"),
            "{}",
            app.input
        );
    }

    /// An app writing its configuration in a fresh temporary file.
    fn app_with_config(tag: &str) -> (Flair, PathBuf) {
        let mut app = app();
        let dir =
            std::env::temp_dir().join(format!("flair-ui-config-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("config.toml");
        std::fs::write(&path, "# mine\nmax_results = 8 # kept\n").expect("write");
        app.config_path = path.clone();
        (app, dir)
    }

    #[test]
    fn config_commands_write_apply_and_undo() {
        let (mut app, dir) = app_with_config("set");
        let _ = app.update(Message::InputChanged("> config max_results 12".into()));
        assert!(
            app.results[0].title.contains("12"),
            "{:?}",
            app.results[0].title
        );
        named(&mut app, Named::Enter, Code::Enter);
        assert_eq!(app.config.max_results, 12);
        let text = std::fs::read_to_string(app.config_path.clone()).expect("read");
        assert_eq!(text, "# mine\nmax_results = 12 # kept\n");
        // Invalid: a message, nothing written.
        let _ = app.update(Message::InputChanged("> config max_results 99".into()));
        assert!(app.results[0].actions.is_empty());
        named(&mut app, Named::Enter, Code::Enter);
        assert_eq!(app.config.max_results, 12);
        // `> annuler` restores the previous value.
        let _ = app.update(Message::InputChanged("> annuler".into()));
        named(&mut app, Named::Enter, Code::Enter);
        assert_eq!(app.config.max_results, 8);
        assert!(
            std::fs::read_to_string(app.config_path.clone())
                .expect("read")
                .contains("max_results = 8 # kept")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn looks_are_previewed_then_restored() {
        let (mut app, dir) = app_with_config("preview");
        let before = app.opacity;
        let _ = app.update(Message::InputChanged("> transparence 0.9".into()));
        assert!((app.opacity - 0.9).abs() < 1e-6, "previewed");
        // Escape clears the field: back to the configured look.
        named(&mut app, Named::Escape, Code::Escape);
        assert!((app.opacity - before).abs() < 1e-6, "restored");
        // Browsing the choices previews each; another command ends it.
        let _ = app.update(Message::InputChanged("> config theme.mode ".into()));
        assert!(!app.previewing, "first row: the setting itself");
        named(&mut app, Named::ArrowDown, Code::ArrowDown);
        assert!(app.previewing);
        let _ = app.update(Message::InputChanged("> stats".into()));
        assert!(!app.previewing);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn themes_are_created_previewed_and_applied() {
        let (mut app, dir) = app_with_config("theme");
        app.paths = flair_platform::paths::Paths::rooted(&dir);
        let before = app.tokens;
        let _ = app.update(Message::InputChanged(
            "> thème 1E1B2E E6E1F5 B79CFF lavande".into(),
        ));
        assert!(app.previewing);
        assert_ne!(app.tokens, before, "previewed at once");
        named(&mut app, Named::Enter, Code::Enter);
        let file = dir.join("config/themes/lavande.toml");
        assert!(
            std::fs::read_to_string(&file)
                .expect("saved")
                .contains("#1E1B2E")
        );
        assert_eq!(app.config.theme.dark, "lavande");
        let text = std::fs::read_to_string(app.config_path.clone()).expect("config");
        assert!(text.contains("dark = \"lavande\""), "{text}");
        // Undo goes back to the previous dark theme.
        let _ = app.update(Message::InputChanged("> annuler".into()));
        named(&mut app, Named::Enter, Code::Enter);
        assert_eq!(app.config.theme.dark, "Flair sombre");
        crate::themes::reload(std::path::Path::new("/nonexistent"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn destructive_commands_need_a_second_enter() {
        let mut app = app();
        let _ = app.update(Message::InputChanged("> historique effacer".into()));
        assert_eq!(app.results[0].id, "cmd:history_clear");
        named(&mut app, Named::Enter, Code::Enter);
        assert_eq!(app.pending_confirm.as_deref(), Some("cmd:history_clear"));
        assert_eq!(
            app.results[0].subtitle.as_deref(),
            Some("Entrée à nouveau pour confirmer")
        );
        // Moving away cancels the confirmation.
        named(&mut app, Named::ArrowDown, Code::ArrowDown);
        assert_eq!(app.pending_confirm, None);
        named(&mut app, Named::Enter, Code::Enter);
        named(&mut app, Named::Enter, Code::Enter);
        assert_eq!(app.pending_confirm, None, "confirmed and run");
    }

    #[test]
    fn tab_on_a_command_keeps_the_prefix() {
        let mut app = app();
        // `> ver` also finds `verrouiller`; `> versi` only the version.
        let _ = app.update(Message::InputChanged("> ver".into()));
        assert_eq!(app.results.len(), 2);
        let _ = app.update(Message::InputChanged("> versi".into()));
        assert_eq!(app.results.len(), 1);
        named(&mut app, Named::Tab, Code::Tab);
        assert_eq!(app.input, "> version");
        // Still a command search: the same command is listed.
        assert_eq!(app.results.len(), 1);
        assert!(
            matches!(&app.results[0].actions[0], Action::Internal { command } if command == "version")
        );
    }

    #[test]
    fn startup_messages_are_shown_once() {
        let i18n = Arc::new(I18n::new(Lang::En));
        let init = Init {
            dev: Dev::default(),
            config: Config::default(),
            config_path: PathBuf::from("config.toml"),
            start_visible: false,
            wayland_hint: false,
            paths: flair_platform::paths::Paths::rooted(
                &std::env::temp_dir().join("flair-ui-tests"),
            ),
            initial: Initial::Search,
            messages: vec!["Flair is installed".into()],
            issues: vec![],
            i18n: Arc::clone(&i18n),
            tokens: builtin::dark().tokens(),
            provider: Arc::new(MockProvider::new(i18n, 8)),
            sources: None,
            started: Instant::now(),
        };
        let mut app = Flair::new(init, Services::none()).0;
        assert!(app.visible, "a message opens the window");
        assert_eq!(app.rows()[0].title, "Flair is installed");
        let _ = app.update(Message::CloseRequested);
        let _ = app.update(Message::External(External::Show));
        assert!(app.rows().is_empty(), "not shown again");
    }

    #[test]
    fn ctrl_numbers_follow_the_ctrl_key() {
        let mut app = app();
        let _ = app.update(Message::ModifiersChanged(Modifiers::CTRL));
        assert!(app.ctrl_held);
        let _ = app.update(Message::ModifiersChanged(Modifiers::empty()));
        assert!(!app.ctrl_held);
    }

    #[test]
    fn config_issues_are_shown_only_when_the_field_is_empty() {
        let i18n = Arc::new(I18n::new(Lang::Fr));
        let issues = vec![ConfigIssue::UnknownKey("foo".into())];
        let rows = notices_for(&i18n, &issues);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].subtitle.as_deref(), Some("réglage inconnu : foo"));
    }

    #[test]
    fn toggle_shows_and_hides_without_quitting() {
        let mut app = app();
        type_text(&mut app, "vsc");
        let _ = app.update(Message::External(External::Toggle));
        assert!(!app.visible);
        assert!(app.input.is_empty(), "field emptied for the next show");
        let _ = app.update(Message::External(External::Toggle));
        assert!(app.visible);
        let _ = app.update(Message::External(External::Show));
        assert!(app.visible, "show on a visible window keeps it");
    }

    #[test]
    fn keep_last_query_survives_hiding() {
        let mut app = app();
        app.config.keep_last_query = true;
        type_text(&mut app, "vsc");
        let _ = app.update(Message::CloseRequested);
        assert!(!app.visible);
        let _ = app.update(Message::External(External::Show));
        assert_eq!(app.input, "vsc");
    }

    #[test]
    fn focus_loss_hides_only_after_a_first_focus() {
        let mut app = app();
        let _ = app.update(Message::WindowUnfocused);
        assert!(app.visible, "spurious unfocus while mapping");
        let _ = app.update(Message::WindowFocused);
        let _ = app.update(Message::WindowUnfocused);
        assert!(!app.visible);
    }

    #[test]
    fn tray_statistics_show_information_rows() {
        let mut app = app();
        let _ = app.update(Message::External(External::Tray(TrayAction::Stats)));
        assert!(app.visible);
        assert!(
            app.rows()
                .iter()
                .any(|r| r.subtitle.as_deref() == Some("Réveils depuis le démarrage"))
        );
        let _ = app.update(Message::CloseRequested);
        assert!(app.info_rows.is_empty(), "cleared when hidden");
    }

    #[test]
    fn hotkey_problems_become_notices_and_show_the_window() {
        let i18n = Arc::new(I18n::new(Lang::Fr));
        let init = Init {
            dev: Dev::default(),
            config: Config::default(),
            config_path: PathBuf::from("config.toml"),
            start_visible: false,
            wayland_hint: false,
            paths: flair_platform::paths::Paths::rooted(
                &std::env::temp_dir().join("flair-ui-tests"),
            ),
            initial: Initial::Search,
            messages: vec![],
            issues: vec![],
            i18n: Arc::clone(&i18n),
            tokens: builtin::dark().tokens(),
            provider: Arc::new(MockProvider::new(i18n, 8)),
            sources: None,
            started: Instant::now(),
        };
        let mut services = Services::none();
        services.hotkey_error = Some(HotkeyError::Taken("Alt+Space".into()));
        let app = Flair::new(init.clone(), services).0;
        assert!(app.visible, "a problem to read opens the window");
        assert_eq!(app.notices.len(), 1);
        assert_eq!(app.notices[0].title, "Raccourci Alt+Space indisponible");
        assert!(matches!(app.notices[0].actions[0], Action::Open { .. }));

        // Wayland without the first-start hint: nothing to show, stays hidden.
        let mut services = Services::none();
        services.hotkey_error = Some(HotkeyError::Unsupported);
        let app = Flair::new(init, services).0;
        assert!(!app.visible);
        assert!(app.notices.is_empty());
    }
}

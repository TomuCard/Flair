//! The launcher window: Elm architecture (state, messages, update, view).
//!
//! Searches run as asynchronous tasks on Iced's thread pool, never on the UI
//! thread. Each query has a sequence number; late answers to an older query
//! are dropped, and starting a new query cancels the previous one.

mod view;

pub use view::INTER;

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

use crate::design;
use crate::style::Palette;

/// Id of the search field.
const INPUT_ID: &str = "search";
/// Id of the result list.
const LIST_ID: &str = "results";

pub struct Flair {
    window: Option<window::Id>,
    i18n: Arc<I18n>,
    tokens: Tokens,
    opacity: f32,
    animations: bool,
    provider: Arc<dyn Provider>,

    input: String,
    results: Vec<ResultItem>,
    selected: usize,
    hovered: Option<usize>,
    ctrl_held: bool,
    /// Configuration problems, shown when the field is empty.
    notices: Vec<ResultItem>,

    query_seq: u64,
    cancel: CancellationToken,
    /// Vertical scroll offset of the list and height of its viewport.
    scroll_offset: f32,
    /// Fade-in start; `None` once finished or when animations are off.
    fade_started: Option<Instant>,
    was_focused: bool,
    started: Instant,
    dev: Dev,
}

#[derive(Debug, Clone)]
pub enum Message {
    WindowOpened(window::Id),
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

#[derive(Clone)]
pub struct Init {
    pub dev: Dev,
    pub config: Config,
    pub issues: Vec<ConfigIssue>,
    pub i18n: Arc<I18n>,
    pub tokens: Tokens,
    pub provider: Arc<dyn Provider>,
    pub started: Instant,
}

impl Flair {
    pub fn new(init: Init) -> (Self, Task<Message>) {
        let notices = notices(&init.i18n, &init.issues);
        let rows = notices
            .iter()
            .map(|n| design::row_height(n.subtitle.is_some()));
        let height = if crate::platform_window::can_resize() {
            design::window_height(rows)
        } else {
            design::max_window_height()
        };
        let (id, open) = window::open(window_settings(height));
        let animations = init.config.theme.animations;
        let state = Self {
            window: Some(id),
            i18n: init.i18n,
            tokens: init.tokens,
            #[allow(clippy::cast_possible_truncation)]
            opacity: init.config.theme.opacity as f32,
            animations,
            provider: init.provider,
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            hovered: None,
            ctrl_held: false,
            notices,
            query_seq: 0,
            cancel: CancellationToken::new(),
            scroll_offset: 0.0,
            fade_started: None,
            was_focused: false,
            started: init.started,
            dev: init.dev,
        };
        (state, open.map(Message::WindowOpened))
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

    /// Rows currently displayed: results, or configuration notices when empty.
    fn rows(&self) -> &[ResultItem] {
        if self.input.trim().is_empty() {
            &self.notices
        } else {
            &self.results
        }
    }

    fn fade(&self) -> f32 {
        match self.fade_started {
            #[allow(clippy::cast_precision_loss)]
            Some(start) => {
                (start.elapsed().as_millis() as f32 / design::FADE_IN_MS as f32).min(1.0)
            }
            None => 1.0,
        }
    }

    pub fn palette(&self) -> Palette {
        Palette::new(&self.tokens, self.opacity, self.fade())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(id) => {
                tracing::info!(
                    elapsed_ms = self.started.elapsed().as_millis(),
                    "window shown"
                );
                if self.animations {
                    self.fade_started = Some(Instant::now());
                }
                let mut tasks = vec![window::gain_focus(id), operation::focus(Id::new(INPUT_ID))];
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
                if self.was_focused {
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
                if self.fade() >= 1.0 {
                    self.fade_started = None;
                }
                Task::none()
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![event::listen_with(on_event)];
        // Frames are only requested during the fade-in, never at rest.
        if self.fade_started.is_some() {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        // Development screenshots wait for the next frame, so the view is up to date.
        if self.dev.shot_pending {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        Subscription::batch(subscriptions)
    }

    fn search(&mut self, value: String) -> Task<Message> {
        self.input = value;
        self.cancel.cancel();
        self.cancel = CancellationToken::new();
        self.query_seq += 1;

        let query = Query::parse(&self.input);
        if query.is_empty() {
            self.results.clear();
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
                if self.input.is_empty() {
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
        let text = match item.provider {
            ProviderKind::File => item.copy_text(),
            _ => item.title.clone(),
        };
        Task::batch([
            self.search(text),
            operation::move_cursor_to_end(Id::new(INPUT_ID)),
        ])
    }

    fn activate(&mut self, index: usize, which: Which) -> Task<Message> {
        let Some(item) = self.rows().get(index) else {
            return Task::none();
        };
        let action = match which {
            Which::Primary => item.primary_action(),
            Which::Secondary => item.secondary_action().or(item.primary_action()),
            Which::Raw => item.primary_action(),
        };
        let Some(action) = action.cloned() else {
            return Task::none();
        };
        tracing::info!(id = %item.id, ?action, "activate");
        match action {
            Action::CopyToClipboard { text } => {
                let text = if which == Which::Raw {
                    text.replace(',', ".")
                } else {
                    text
                };
                Task::batch([iced::clipboard::write(text), self.hide()])
            }
            // Milestone 1: no real launching yet (providers come in milestone 4+).
            _ => self.hide(),
        }
    }

    /// Hides the window. Milestone 1 has no resident process yet (tray icon
    /// and IPC come with milestone 2), so hiding quits.
    fn hide(&mut self) -> Task<Message> {
        tracing::debug!("hide");
        self.cancel.cancel();
        match self.window.take() {
            Some(id) => window::close(id).chain(iced::exit()),
            None => Task::none(),
        }
    }

    /// Height of the visible frame for the current rows.
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
fn notices(i18n: &I18n, issues: &[ConfigIssue]) -> Vec<ResultItem> {
    use flair_core::IconSource;
    use flair_core::result::BuiltinIcon;
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

fn window_settings(height: f32) -> window::Settings {
    window::Settings {
        size: Size::new(design::WINDOW_WIDTH, height),
        // Centered horizontally, at 25 % of the screen height. Milestone 2
        // will use the screen under the mouse.
        position: window::Position::SpecificWith(|window, monitor| {
            Point::new(
                ((monitor.width - window.width) / 2.0).max(0.0),
                monitor.height * design::WINDOW_TOP_FRACTION,
            )
        }),
        visible: true,
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
            issues: vec![],
            i18n: Arc::clone(&i18n),
            tokens: builtin::dark().tokens(),
            provider: Arc::new(MockProvider::new(i18n, 8)),
            started: Instant::now(),
        };
        Flair::new(init).0
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
        assert!(app.window.is_some(), "first Escape only clears");
        named(&mut app, Named::Escape, Code::Escape);
        assert!(app.window.is_none(), "second Escape hides");
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
        assert!(app.window.is_some(), "no 9th result: nothing happens");
        press(
            &mut app,
            Key::Character("é".into()),
            Code::Digit2,
            Modifiers::CTRL,
        );
        assert!(app.window.is_none(), "2nd result launched, window hidden");
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
        let rows = notices(&i18n, &issues);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].subtitle.as_deref(), Some("réglage inconnu : foo"));
    }
}

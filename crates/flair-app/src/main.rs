//! Flair: a keyboard launcher for Windows and Linux.
// No console window in release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod design;
mod external;
mod logging;
mod mock;
mod platform_window;
mod png;
mod services;
mod stats;
mod style;
mod ui;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use clap::Parser;
use flair_core::i18n::{I18n, Lang};
use flair_core::settings::config::{RendererSetting, ThemeMode, UiLanguageSetting};
use flair_core::settings::{self, Config};
use flair_core::theme::{Theme, builtin};
use flair_platform::ipc::{self, Command};
use flair_platform::paths::Paths;
use flair_platform::renderer::{self, Renderer};

use crate::external::External;

/// Flair — launcher clavier façon Spotlight / keyboard launcher.
#[derive(Debug, Parser)]
#[command(name = "flair", version, about)]
struct Cli {
    /// Show the window right away (also where the global shortcut is
    /// unavailable, e.g. WSLg).
    #[arg(long)]
    show: bool,

    /// Show or hide the window of the running instance (bind this to a
    /// desktop shortcut on Wayland).
    #[arg(long, conflicts_with_all = ["show", "stats", "quit"])]
    toggle: bool,

    /// Print resource statistics of the running instance.
    #[arg(long, conflicts_with_all = ["show", "quit"])]
    stats: bool,

    /// Quit the running instance.
    #[arg(long, conflicts_with = "show")]
    quit: bool,

    /// Start without showing the window (used when launched at session start).
    #[arg(long, conflicts_with_all = ["show", "toggle"])]
    background: bool,

    /// Development: type this query once the window is shown.
    #[arg(long, hide = true)]
    dev_query: Option<String>,

    /// Development: save a PNG screenshot of the window to this file, then quit.
    #[arg(long, hide = true)]
    dev_screenshot: Option<std::path::PathBuf>,
}

fn main() -> ExitCode {
    let started = Instant::now();
    let cli = Cli::parse();
    match run(cli, started) {
        Ok(code) => code,
        Err(e) => {
            tracing::error!("{e:#}");
            eprintln!("flair: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli, started: Instant) -> anyhow::Result<ExitCode> {
    let paths = Paths::new().context("cannot locate the user directories")?;
    let dev = cli.dev_screenshot.is_some();

    // 1. Is Flair already running? Then hand the command over and exit.
    let command = if cli.stats {
        Command::Stats
    } else if cli.quit {
        Command::Quit
    } else if cli.toggle {
        Command::Toggle
    } else if cli.background {
        // Already running: nothing to do.
        Command::Stats
    } else {
        Command::Show
    };
    if !dev {
        match ipc::send(command) {
            Ok(_) if cli.background => return Ok(ExitCode::SUCCESS),
            Ok(reply) => return Ok(client_output(command, &reply, &paths)),
            Err(e) => tracing::debug!("no running instance: {e}"),
        }
        if cli.stats || cli.quit {
            flair_platform::console::attach_parent();
            let i18n = I18n::new(ui_lang(&paths));
            flair_platform::console::eprintln(&i18n.t("not-running"));
            return Ok(ExitCode::FAILURE);
        }
    }

    // 2. Become the instance.
    let server = if dev {
        None
    } else {
        Some(ipc::Server::bind().context("cannot take the single instance lock")?)
    };

    // The default config file is written in the system language: the config
    // (and thus `ui_language`) does not exist yet.
    let system_lang = Lang::resolve(UiLanguageSetting::System);
    let loaded = settings::load_or_create(&paths.config_file(), system_lang);
    let mut config = loaded.config;

    // Must happen before any other thread is started (logging spawns one).
    renderer::select(match config.resources.renderer {
        RendererSetting::Auto => Renderer::Auto,
        RendererSetting::Gpu => Renderer::Gpu,
        RendererSetting::Cpu => Renderer::Cpu,
    });

    let _log_guard = logging::init(&paths.log_dir(), config.log_level.as_str());
    stats::mark_started(started);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        ?command,
        "Flair starting"
    );
    for issue in &loaded.issues {
        tracing::warn!(path = %paths.config_file().display(), "{issue}");
    }

    if let Some(server) = server {
        server
            .spawn(|command| match command {
                Command::Stats => serde_json::to_string(&stats::snapshot())
                    .unwrap_or_else(|e| format!("error: {e}")),
                Command::Show => reply_ok(External::Show),
                Command::Toggle => reply_ok(External::Toggle),
                Command::Quit => reply_ok(External::Quit),
            })
            .context("cannot start the IPC thread")?;
    }
    sync_autostart(&config);

    let lang = Lang::resolve(config.ui_language);
    let i18n = Arc::new(I18n::new(lang));
    let theme = active_theme(&config);
    let tokens = theme.tokens();
    if tokens.adjusted {
        tracing::info!(theme = %theme.name, "contrast adjusted");
    }
    tracing::info!(renderer = %renderer::active_name(), theme = %theme.name, ?lang, "ready");

    let provider = Arc::new(mock::MockProvider::new(
        Arc::clone(&i18n),
        config.max_results as usize,
    ));
    if dev {
        config.theme.animations = false; // capture the final look, not the fade
    }
    let init = ui::Init {
        dev: ui::Dev {
            query: cli.dev_query,
            screenshot: cli.dev_screenshot,
            shot_pending: false,
        },
        start_visible: dev || !cli.background,
        wayland_hint: !dev && first_wayland_start(&paths),
        config_path: paths.config_file(),
        config,
        issues: loaded.issues,
        i18n,
        tokens,
        provider,
        started,
    };

    let mut app = iced::daemon(
        move || {
            // On the UI thread: Windows pumps tray and shortcut messages there.
            let services = if dev {
                services::Services::none()
            } else {
                services::Services::start(&init.config, &init.i18n, &init.tokens)
            };
            ui::Flair::new(init.clone(), services)
        },
        ui::Flair::update,
        ui::Flair::view,
    )
    .title(ui::Flair::title)
    .theme(ui::Flair::theme)
    .style(|state: &ui::Flair, theme| state.style(theme))
    .subscription(ui::Flair::subscription)
    .default_font(ui::INTER)
    .antialiasing(true);
    for font in assets::fonts() {
        app = app.font(font);
    }
    app.run().context("the user interface failed")?;
    tracing::info!("Flair stopped");
    Ok(ExitCode::SUCCESS)
}

/// Forwards an IPC command to the UI.
fn reply_ok(event: External) -> String {
    external::send(event);
    "ok".into()
}

/// What a client process prints after handing its command over.
fn client_output(command: Command, reply: &str, paths: &Paths) -> ExitCode {
    if command != Command::Stats {
        return ExitCode::SUCCESS;
    }
    flair_platform::console::attach_parent();
    let i18n = I18n::new(ui_lang(paths));
    match serde_json::from_str::<stats::Snapshot>(reply) {
        Ok(snapshot) => {
            flair_platform::console::println(&i18n.t("stats-title"));
            let lines = stats::lines(&snapshot, &i18n);
            let width = lines
                .iter()
                .map(|(l, _)| l.chars().count())
                .max()
                .unwrap_or(0);
            for (label, value) in lines {
                let pad = width - label.chars().count();
                flair_platform::console::println(&format!("  {label}{:pad$}  {value}", ""));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            flair_platform::console::eprintln(&format!("flair: {reply} ({e})"));
            ExitCode::FAILURE
        }
    }
}

/// Interface language without starting the app (client mode).
fn ui_lang(paths: &Paths) -> Lang {
    let setting = std::fs::read_to_string(paths.config_file())
        .map(|text| settings::parse_config(&text).config.ui_language)
        .unwrap_or(UiLanguageSetting::System);
    Lang::resolve(setting)
}

/// Keeps the OS registration in line with the `autostart` setting.
fn sync_autostart(config: &Config) {
    if flair_platform::autostart::is_enabled() == config.autostart {
        return;
    }
    let result = std::env::current_exe()
        .and_then(|exe| flair_platform::autostart::set(config.autostart, &exe));
    if let Err(e) = result {
        tracing::warn!("cannot update launch at startup: {e}");
    }
}

/// True the first time Flair starts on Wayland (records it in the data dir).
fn first_wayland_start(paths: &Paths) -> bool {
    if !flair_platform::hotkey::is_wayland() {
        return false;
    }
    let marker = paths.data_dir.join("wayland-hint-shown");
    if marker.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(&paths.data_dir);
    let _ = std::fs::write(&marker, b"");
    true
}

/// Theme for the configured mode. Only the built-in themes for now; following
/// the system theme live and custom themes come with milestone 8.
fn active_theme(config: &Config) -> Theme {
    let name = match config.theme.mode {
        ThemeMode::Light => &config.theme.light,
        ThemeMode::Dark | ThemeMode::System => &config.theme.dark,
    };
    builtin::all()
        .into_iter()
        .find(|t| &t.name == name)
        .unwrap_or_else(|| {
            tracing::warn!(theme = %name, "unknown theme, using the built-in one");
            if config.theme.mode == ThemeMode::Light {
                builtin::light()
            } else {
                builtin::dark()
            }
        })
}

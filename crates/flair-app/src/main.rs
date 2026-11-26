//! Flair: a keyboard launcher for Windows and Linux.
// No console window in release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod design;
mod logging;
mod mock;
mod platform_window;
mod png;
mod style;
mod ui;

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use clap::Parser;
use flair_core::i18n::{I18n, Lang};
use flair_core::settings::config::{RendererSetting, ThemeMode};
use flair_core::settings::{self, Config};
use flair_core::theme::{Theme, builtin};
use flair_platform::paths::Paths;
use flair_platform::renderer::{self, Renderer};

/// Flair — launcher clavier façon Spotlight / keyboard launcher.
#[derive(Debug, Parser)]
#[command(name = "flair", version, about)]
struct Cli {
    /// Show the window right away (for testing the UI where the global
    /// shortcut is unavailable, e.g. WSLg).
    #[arg(long)]
    show: bool,

    /// Development: type this query once the window is shown.
    #[arg(long, hide = true)]
    dev_query: Option<String>,

    /// Development: save a PNG screenshot of the window to this file, then quit.
    #[arg(long, hide = true)]
    dev_screenshot: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let started = Instant::now();
    let cli = Cli::parse();

    let paths = Paths::new().context("cannot locate the user directories")?;
    // The default config file is written in the system language: the config
    // (and thus `ui_language`) does not exist yet.
    let system_lang = Lang::resolve(settings::config::UiLanguageSetting::System);
    let loaded = settings::load_or_create(&paths.config_file(), system_lang);
    let config = loaded.config;

    // Must happen before any thread is started (logging spawns one).
    renderer::select(match config.resources.renderer {
        RendererSetting::Auto => Renderer::Auto,
        RendererSetting::Gpu => Renderer::Gpu,
        RendererSetting::Cpu => Renderer::Cpu,
    });

    let _log_guard = logging::init(&paths.log_dir(), config.log_level.as_str());
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        show = cli.show,
        "Flair starting"
    );
    for issue in &loaded.issues {
        tracing::warn!(path = %paths.config_file().display(), "{issue}");
    }

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
    let mut config = config;
    if cli.dev_screenshot.is_some() {
        config.theme.animations = false; // capture the final look, not the fade
    }
    let init = ui::Init {
        dev: ui::Dev {
            query: cli.dev_query,
            screenshot: cli.dev_screenshot,
            shot_pending: false,
        },
        config,
        issues: loaded.issues,
        i18n,
        tokens,
        provider,
        started,
    };

    let mut app = iced::daemon(
        move || ui::Flair::new(init_once(&init)),
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
    Ok(())
}

/// Iced's boot function may be called more than once by the runtime: it
/// receives a fresh copy of the initial state each time.
fn init_once(init: &ui::Init) -> ui::Init {
    init.clone()
}

/// Theme for the configured mode. Milestone 1 knows the built-in themes only;
/// following the system theme live comes with milestone 8.
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

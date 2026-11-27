//! Flair: a keyboard launcher for Windows and Linux.
// No console window in release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod build_info;
mod commands;
mod design;
mod external;
mod logging;
#[cfg(test)]
mod mock;
mod platform_window;
mod png;
mod services;
mod sources;
mod stats;
mod style;
mod themes;
mod ui;
mod updater;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use clap::Parser;
use flair_core::archive;
use flair_core::i18n::{I18n, Lang};
use flair_core::settings::config::{RendererSetting, UiLanguageSetting};
use flair_core::settings::{self, Config};
use flair_core::update::{Decision, Manifest, decide};
use flair_platform::console;
use flair_platform::ipc::{self, Command};
use flair_platform::paths::Paths;
use flair_platform::renderer::{self, Renderer};

use crate::external::External;

/// Flair — launcher clavier façon Spotlight / keyboard launcher.
#[derive(Debug, Parser)]
#[command(name = "flair", version = build_info::VERSION, long_version = long_version(), about)]
struct Cli {
    /// A `.flair` file to import (double-click on an export).
    file: Option<PathBuf>,

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

    /// Look for an update and install it.
    #[arg(long)]
    update: bool,

    /// Export everything to a `.flair` file.
    #[arg(long, value_name = "PATH")]
    export: Option<PathBuf>,

    /// With --export: settings only (no history nor usage data).
    #[arg(long, requires = "export")]
    settings_only: bool,

    /// Import a `.flair` file (a backup of the current state is made first).
    #[arg(long, value_name = "PATH")]
    import: Option<PathBuf>,

    /// With --import: only add the themes and lexicons.
    #[arg(long, requires = "import")]
    merge: bool,

    /// Show the uninstallation screen.
    #[arg(long)]
    uninstall: bool,

    /// Internal: started by the previous version after an update.
    #[arg(long, hide = true)]
    after_update: bool,

    /// Internal: started by the installer.
    #[arg(long, hide = true)]
    after_install: bool,

    /// Internal: wait for this process to exit first.
    #[arg(long, hide = true, value_name = "PID")]
    wait_pid: Option<u32>,

    /// Release check: read a manifest with this version's code.
    #[arg(long, hide = true, value_name = "FILE")]
    check_manifest: Option<PathBuf>,

    /// Development: type this query once the window is shown.
    #[arg(long, hide = true)]
    dev_query: Option<String>,

    /// Development: save a PNG screenshot of the window to this file, then quit.
    #[arg(long, hide = true)]
    dev_screenshot: Option<PathBuf>,
}

/// `flair --version` output (clap needs a `'static` string).
fn long_version() -> &'static str {
    Box::leak(
        format!(
            "{} ({}, {}, {})",
            build_info::VERSION,
            build_info::COMMIT,
            build_info::date(),
            build_info::CHANNEL
        )
        .into_boxed_str(),
    )
}

fn main() -> ExitCode {
    let started = Instant::now();

    // The very first thing: a freshly updated version that keeps crashing at
    // startup is replaced by the previous one.
    let paths = Paths::new().ok();
    if let (Ok(exe), Some(paths)) = (std::env::current_exe(), &paths)
        && updater::startup_guard(&exe, &updater::Files::new(&paths.data_dir))
            == updater::Guard::RolledBack
    {
        return ExitCode::SUCCESS;
    }

    let cli = Cli::parse();
    let Some(paths) = paths else {
        eprintln!("flair: cannot locate the user directories");
        return ExitCode::FAILURE;
    };
    match run(cli, &paths, started) {
        Ok(code) => code,
        Err(e) => {
            tracing::error!("{e:#}");
            console::attach_parent();
            console::eprintln(&format!("flair: {e:#}"));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli, paths: &Paths, started: Instant) -> anyhow::Result<ExitCode> {
    let dev = cli.dev_screenshot.is_some();

    if let Some(file) = &cli.check_manifest {
        return check_manifest(file);
    }
    if let Some(pid) = cli.wait_pid {
        // The previous instance holds the single-instance lock until it exits.
        flair_platform::process::wait_for_exit(pid, Duration::from_secs(20));
    }
    if cli.export.is_some() || cli.import.is_some() || cli.update {
        return command_line_tool(&cli, paths);
    }

    // 1. Is Flair already running? Then hand the command over and exit.
    let command = if cli.stats {
        Command::Stats
    } else if cli.quit {
        Command::Quit
    } else if cli.toggle {
        Command::Toggle
    } else if cli.uninstall {
        Command::Uninstall
    } else if let Some(file) = &cli.file {
        Command::Open(file.display().to_string())
    } else if cli.background {
        // Already running: nothing to do.
        Command::Stats
    } else {
        Command::Show
    };
    if !dev {
        match ipc::send(&command) {
            Ok(_) if cli.background => return Ok(ExitCode::SUCCESS),
            Ok(reply) => return Ok(client_output(&command, &reply, paths)),
            Err(e) => tracing::debug!("no running instance: {e}"),
        }
        if cli.stats || cli.quit {
            console::attach_parent();
            let i18n = I18n::new(ui_lang(paths));
            console::eprintln(&i18n.t("not-running"));
            return Ok(ExitCode::FAILURE);
        }
    }

    // 2. Become the instance.
    let server = if dev {
        None
    } else {
        Some(ipc::Server::bind().context("cannot take the single instance lock")?)
    };

    // Older configuration format: back it up, then migrate it.
    let system_lang = Lang::resolve(UiLanguageSetting::System);
    let migration = migrate_config(paths);
    let loaded = settings::load_or_create(&paths.config_file(), system_lang);
    let config = loaded.config;

    // Must happen before any other thread is started (logging spawns one).
    renderer::select(match config.resources.renderer {
        RendererSetting::Auto => Renderer::Auto,
        RendererSetting::Gpu => Renderer::Gpu,
        RendererSetting::Cpu => Renderer::Cpu,
    });

    let _log_guard = logging::init(&paths.log_dir(), config.log_level.as_str());
    stats::mark_started(started);
    tracing::info!(
        version = build_info::VERSION,
        commit = build_info::COMMIT,
        ?command,
        "Flair starting"
    );
    if let Some(message) = &migration {
        tracing::info!("{message}");
    }
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
                Command::Uninstall => reply_ok(External::Uninstall),
                Command::Reload => reply_ok(External::Reload),
                Command::Open(path) => reply_ok(External::Open(PathBuf::from(path))),
            })
            .context("cannot start the IPC thread")?;
    }
    sync_autostart(&config);

    let lang = Lang::resolve(config.ui_language);
    let i18n = Arc::new(I18n::new(lang));
    let theme = themes::active(&config);
    let tokens = theme.tokens();
    tracing::info!(renderer = %renderer::active_name(), theme = %theme.name, ?lang, "ready");

    // One-off messages for this start.
    let mut messages = Vec::new();
    let files = updater::Files::new(&paths.data_dir);
    if let Some(rolled) = updater::take_rollback_notice(&files) {
        messages.push(i18n.t_args("update-rolled-back", &[("version", &rolled.to)]));
    } else if cli.after_update {
        messages.push(i18n.t_args("update-done", &[("version", build_info::VERSION)]));
    }
    if cli.after_install {
        messages.push(i18n.t("install-done"));
    }

    let exe = std::env::current_exe().ok();
    let initial = if cli.uninstall {
        ui::Initial::Uninstall
    } else if let Some(file) = cli.file.clone() {
        ui::Initial::Import(file)
    } else if !cli.after_install
        && exe
            .as_deref()
            .is_some_and(|e| ui::should_offer_install(e, &paths.data_dir))
    {
        ui::Initial::Install
    } else {
        ui::Initial::Search
    };

    let sources = sources::Sources::start(paths, &i18n, !dev);
    let provider = sources.providers(&i18n, &config);
    let init = ui::Init {
        dev: ui::Dev {
            query: cli.dev_query,
            screenshot: cli.dev_screenshot,
            shot_pending: false,
        },
        start_visible: dev || !cli.background || cli.after_update,
        wayland_hint: !dev && first_wayland_start(paths),
        config_path: paths.config_file(),
        paths: paths.clone(),
        initial,
        messages,
        config,
        issues: loaded.issues,
        i18n,
        tokens,
        provider,
        sources: Some(sources),
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

fn layout(paths: &Paths) -> archive::Layout {
    archive::Layout {
        config_file: paths.config_file(),
        themes_dir: paths.themes_dir(),
        lexicon_dir: paths.lexicon_dir(),
        data_dir: paths.data_dir.clone(),
        home: directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .unwrap_or_default(),
    }
}

/// Migrates an older `config.toml` (after a backup). Returns a log message.
fn migrate_config(paths: &Paths) -> Option<String> {
    let path = paths.config_file();
    let text = std::fs::read_to_string(&path).ok()?;
    match settings::migrate::migrate(&text) {
        Ok(Some(migrated)) => {
            let origin = archive::Origin::current(build_info::VERSION);
            let backup = archive::backup(
                &layout(paths),
                &paths.data_dir.join("backups"),
                &origin,
                "migration",
            );
            if let Err(e) = &backup {
                // Never migrate without a backup.
                return Some(format!("configuration not migrated, backup failed: {e}"));
            }
            match std::fs::write(&path, &migrated.text) {
                Ok(()) => Some(format!(
                    "configuration migrated from v{} to v{}",
                    migrated.from, migrated.to
                )),
                Err(e) => Some(format!("configuration migration not written: {e}")),
            }
        }
        Ok(None) => None,
        Err(e) => Some(format!("configuration not migrated: {e}")),
    }
}

/// `--export`, `--import`, `--update`: work without the UI, print the result.
fn command_line_tool(cli: &Cli, paths: &Paths) -> anyhow::Result<ExitCode> {
    console::attach_parent();
    let i18n = I18n::new(ui_lang(paths));
    let origin = archive::Origin::current(build_info::VERSION);
    let layout = layout(paths);

    if let Some(dest) = &cli.export {
        archive::export(&layout, dest, cli.settings_only, &origin)?;
        let size = std::fs::metadata(dest).map_or(0, |m| m.len());
        console::println(&i18n.t_args(
            "export-done",
            &[
                ("path", &dest.display().to_string()),
                ("size", &ui::bytes(size)),
            ],
        ));
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(source) = &cli.import {
        let archive = match archive::read(source, &layout.home) {
            Ok(a) => a,
            Err(archive::ArchiveError::Newer) => {
                console::eprintln(&i18n.t("import-newer"));
                return Ok(ExitCode::FAILURE);
            }
            Err(e) => anyhow::bail!(i18n.t_args("import-failed", &[("error", &e.to_string())])),
        };
        archive::backup(&layout, &paths.data_dir.join("backups"), &origin, "import")?;
        let mode = if cli.merge {
            archive::ImportMode::AddThemesAndLexicons
        } else {
            archive::ImportMode::ReplaceAll
        };
        let applied = archive::apply(&archive, &layout, mode)?;
        for (from, to) in &applied.renamed {
            console::println(&i18n.t_args("import-renamed", &[("from", from), ("to", to)]));
        }
        // A running instance applies the new configuration at once.
        let _ = ipc::send(&Command::Reload);
        console::println(&i18n.t("import-done"));
        return Ok(ExitCode::SUCCESS);
    }

    // --update
    let text = std::fs::read_to_string(paths.config_file()).unwrap_or_default();
    let config = settings::parse_config(&text).config;
    let exe = std::env::current_exe()?;
    if build_info::is_dev() || !flair_platform::install::can_self_update(&exe) {
        console::eprintln(&i18n.t("update-disabled-dev"));
        return Ok(ExitCode::FAILURE);
    }
    console::println(&i18n.t("update-checking"));
    let endpoint = updater::Endpoint::github();
    let release = updater::latest(&endpoint, config.update.channel)?;
    match decide(build_info::VERSION, &release.manifest)? {
        Decision::UpToDate => {
            console::println(
                &i18n.t_args("update-up-to-date", &[("version", build_info::VERSION)]),
            );
            return Ok(ExitCode::SUCCESS);
        }
        Decision::ManualDownload => {
            console::println(&format!(
                "{} — {}",
                i18n.t("update-manual"),
                endpoint.releases_page()
            ));
            return Ok(ExitCode::FAILURE);
        }
        Decision::Available => {}
    }
    console::println(&i18n.t_args(
        "update-available",
        &[
            ("version", &release.manifest.version),
            ("current", build_info::VERSION),
        ],
    ));
    archive::backup(&layout, &paths.data_dir.join("backups"), &origin, "update")?;
    let binary = updater::fetch_verified(&endpoint, &release, &|_, _| {})?;
    let files = updater::Files::new(&paths.data_dir);
    updater::install(
        &binary,
        &exe,
        &files,
        build_info::VERSION,
        &release.manifest.version,
    )?;
    if release.manifest.requires_reset {
        let _ = std::fs::remove_file(paths.config_file());
    }
    // Restart the running instance on the new version.
    if ipc::send(&Command::Quit).is_ok() {
        std::thread::sleep(Duration::from_millis(500));
        flair_platform::process::spawn_detached(&exe, &["--background", "--after-update"])?;
    }
    console::println(&i18n.t_args("update-done", &[("version", &release.manifest.version)]));
    Ok(ExitCode::SUCCESS)
}

/// Release workflow check: the updater of this version can read `file`.
fn check_manifest(file: &Path) -> anyhow::Result<ExitCode> {
    console::attach_parent();
    let manifest = Manifest::parse(&std::fs::read_to_string(file)?)?;
    let decision = decide(build_info::VERSION, &manifest)?;
    console::println(&format!(
        "manifest {} read by Flair {}: {decision:?}",
        manifest.version,
        build_info::VERSION
    ));
    Ok(ExitCode::SUCCESS)
}

/// What a client process prints after handing its command over.
fn client_output(command: &Command, reply: &str, paths: &Paths) -> ExitCode {
    if *command != Command::Stats {
        return ExitCode::SUCCESS;
    }
    console::attach_parent();
    let i18n = I18n::new(ui_lang(paths));
    match serde_json::from_str::<stats::Snapshot>(reply) {
        Ok(snapshot) => {
            console::println(&i18n.t("stats-title"));
            let lines = stats::lines(&snapshot, &i18n);
            let width = lines
                .iter()
                .map(|(l, _)| l.chars().count())
                .max()
                .unwrap_or(0);
            for (label, value) in lines {
                let pad = width - label.chars().count();
                console::println(&format!("  {label}{:pad$}  {value}", ""));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            console::eprintln(&format!("flair: {reply} ({e})"));
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

/// Keeps the OS registration in line with the `autostart` setting, for the
/// executable that is running.
fn sync_autostart(config: &Config) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    if flair_platform::install::location(&exe) == flair_platform::install::Location::Development {
        return;
    }
    if flair_platform::autostart::is_enabled() == config.autostart {
        return;
    }
    if let Err(e) = flair_platform::autostart::set(config.autostart, &exe) {
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

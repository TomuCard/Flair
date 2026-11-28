//! Flair's `>` commands: settings (`> config`, shortcuts like
//! `> transparence 0.9`), languages, system commands, Flair's own actions and
//! the generated help.
//!
//! Every word typed after `>` comes from the language files (`[commands]`,
//! `[arguments]`): they follow the active input languages, except `> help`,
//! `> config` and `> langue`, which are in `common.toml`. This module only
//! builds rows; running their actions is the interface's job:
//!
//! | Action | Meaning |
//! |---|---|
//! | `Internal("fill:<text>")` | write `<text>` in the field |
//! | `Internal("set:<key>\u{1f}<TOML value>")` | apply and save a setting |
//! | `Internal("help:<id>")` | details of a command |
//! | `Internal("<id>")` | one of Flair's actions (`stats`, `reindex`…) |
//! | `SystemCommand(…)` | lock, shut down… (destructive ones confirmed) |

pub mod help;
pub mod settings;
pub mod theme;

use flair_core::command_words::CommandWords;
use flair_core::i18n::{I18n, Lang};
use flair_core::lexicon::{self, Lexicon};
use flair_core::result::BuiltinIcon;
use flair_core::settings::Config;
use flair_core::settings::config::UpdateChannel;
use flair_core::{Action, IconSource, ProviderKind, ResultItem, SystemCommand};

/// Separates the key from the value in `set:` actions.
pub const SET_SEPARATOR: char = '\u{1f}';

/// Groups of the help, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Settings,
    Appearance,
    Languages,
    Files,
    Web,
    Calc,
    System,
    Flair,
}

impl Category {
    pub const ALL: [Self; 8] = [
        Self::Settings,
        Self::Appearance,
        Self::Languages,
        Self::Files,
        Self::Web,
        Self::Calc,
        Self::System,
        Self::Flair,
    ];

    pub fn label_id(self) -> &'static str {
        match self {
            Self::Settings => "help-cat-settings",
            Self::Appearance => "help-cat-appearance",
            Self::Languages => "help-cat-languages",
            Self::Files => "help-cat-files",
            Self::Web => "help-cat-web",
            Self::Calc => "help-cat-calc",
            Self::System => "help-cat-system",
            Self::Flair => "help-cat-flair",
        }
    }
}

/// What a command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// One of Flair's actions, run by `Enter` (no argument).
    Action,
    /// A system command.
    System(SystemCommand),
    /// Takes arguments: rows depend on what follows.
    Args,
    /// Shortcut to one setting (`> transparence 0.9`).
    Setting(&'static str),
}

/// One command. Its title, description and example are the i18n messages
/// `cmd-<id>`, `cmd-<id>-desc` and `cmd-<id>-example` (the arguments of the
/// example; the command word comes from the language files).
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub id: &'static str,
    pub category: Category,
    pub run: Run,
}

const fn cmd(id: &'static str, category: Category, run: Run) -> Command {
    Command { id, category, run }
}

/// Every command, in help order.
pub const CATALOG: &[Command] = &[
    cmd("config", Category::Settings, Run::Args),
    cmd("undo", Category::Settings, Run::Action),
    cmd("hotkey", Category::Settings, Run::Setting("hotkey")),
    cmd("autostart", Category::Settings, Run::Setting("autostart")),
    cmd("results", Category::Settings, Run::Setting("max_results")),
    cmd("battery", Category::Settings, Run::Args),
    cmd("theme", Category::Appearance, Run::Args),
    cmd(
        "opacity",
        Category::Appearance,
        Run::Setting("theme.opacity"),
    ),
    cmd("icons", Category::Appearance, Run::Setting("theme.icons")),
    cmd(
        "renderer",
        Category::Appearance,
        Run::Setting("resources.renderer"),
    ),
    cmd("language", Category::Languages, Run::Args),
    cmd("files", Category::Files, Run::Args),
    cmd("reindex", Category::Files, Run::Action),
    cmd("web", Category::Web, Run::Args),
    cmd("calc", Category::Calc, Run::Args),
    cmd("history_clear", Category::Calc, Run::Action),
    cmd("lock", Category::System, Run::System(SystemCommand::Lock)),
    cmd("sleep", Category::System, Run::System(SystemCommand::Sleep)),
    cmd(
        "shutdown",
        Category::System,
        Run::System(SystemCommand::Shutdown),
    ),
    cmd(
        "restart",
        Category::System,
        Run::System(SystemCommand::Restart),
    ),
    cmd(
        "logout",
        Category::System,
        Run::System(SystemCommand::LogOut),
    ),
    cmd(
        "empty_trash",
        Category::System,
        Run::System(SystemCommand::EmptyTrash),
    ),
    cmd("help", Category::Flair, Run::Args),
    cmd("stats", Category::Flair, Run::Action),
    cmd("logs", Category::Flair, Run::Action),
    cmd("update", Category::Flair, Run::Args),
    cmd("version", Category::Flair, Run::Action),
    cmd("export", Category::Flair, Run::Action),
    cmd("import", Category::Flair, Run::Action),
    cmd("restart_flair", Category::Flair, Run::Action),
    cmd("install", Category::Flair, Run::Action),
    cmd("uninstall", Category::Flair, Run::Action),
    cmd("quit", Category::Flair, Run::Action),
];

/// Shown by `>` alone, before `> help`.
const FAVORITES: &[&str] = &["config", "language", "reindex", "stats", "update", "lock"];

pub fn find(id: &str) -> Option<&'static Command> {
    CATALOG.iter().find(|c| c.id == id)
}

/// Commands whose `Enter` must be pressed twice.
pub fn needs_confirmation(action: &Action) -> bool {
    match action {
        Action::SystemCommand(c) => c.needs_confirmation(),
        Action::Internal { command } => command == "history_clear",
        _ => false,
    }
}

/// An input language that can be enabled: code and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub code: String,
    pub name: String,
}

/// Every language file: built-in ones, replaced or completed by the user's.
fn all_lexicons(lexicon_dir: &std::path::Path) -> Vec<Lexicon> {
    let (user, _) = lexicon::load_dir(lexicon_dir);
    let mut all: Vec<Lexicon> = lexicon::BUILTIN
        .iter()
        .filter_map(|(code, _)| lexicon::builtin(code))
        .filter(|l| !user.iter().any(|u| u.meta.code == l.meta.code))
        .collect();
    all.extend(user);
    all
}

/// Input languages available (not `common`).
pub fn languages(lexicon_dir: &std::path::Path) -> Vec<Language> {
    let mut out: Vec<Language> = all_lexicons(lexicon_dir)
        .into_iter()
        .filter(|l| l.meta.code != lexicon::COMMON)
        .map(|l| Language {
            name: if l.meta.name.is_empty() {
                l.meta.code.clone()
            } else {
                l.meta.name.clone()
            },
            code: l.meta.code,
        })
        .collect();
    out.sort_by(|a, b| a.code.cmp(&b.code));
    out
}

/// Command words of the active languages (`common` always included).
pub fn words(config: &Config, lexicon_dir: &std::path::Path, ui: Lang) -> CommandWords {
    let all = all_lexicons(lexicon_dir);
    let mut active: Vec<&Lexicon> = all
        .iter()
        .filter(|l| l.meta.code == lexicon::COMMON)
        .collect();
    for code in &config.input_languages {
        active.extend(all.iter().filter(|l| &l.meta.code == code));
    }
    let ui = match ui {
        Lang::Fr => "fr",
        Lang::En => "en",
    };
    CommandWords::new(&active, ui)
}

/// What the rows depend on.
pub struct Context<'a> {
    pub i18n: &'a I18n,
    pub words: &'a CommandWords,
    pub config: &'a Config,
    pub languages: &'a [Language],
    pub portable: bool,
    /// Version found by the last automatic check, if newer.
    pub update_available: Option<&'a str>,
}

impl Context<'_> {
    pub fn t(&self, id: &str) -> String {
        self.i18n.t(id)
    }

    pub fn ta(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.i18n.t_args(id, args)
    }

    /// `> verrouiller`: the command as typed in the active languages.
    pub fn typed(&self, id: &str) -> Option<String> {
        self.words.shown(id).map(|w| format!("> {w}"))
    }

    fn available(&self, c: &Command) -> bool {
        self.words.has(c.id) && (c.id != "install" || self.portable)
    }
}

/// A row and what `Tab` writes for it.
#[derive(Debug, Clone)]
pub struct Row {
    pub item: ResultItem,
    pub completion: Option<String>,
}

pub fn item(
    id: String,
    title: String,
    subtitle: Option<String>,
    icon: BuiltinIcon,
    action: Option<Action>,
) -> ResultItem {
    ResultItem {
        id,
        title,
        subtitle,
        icon: IconSource::Builtin(icon),
        score: 0.0,
        match_indices: vec![],
        actions: action.into_iter().collect(),
        provider: ProviderKind::Command,
    }
}

/// A row running an internal command.
pub fn row(id: &str, title: String, subtitle: Option<String>, icon: BuiltinIcon) -> ResultItem {
    item(
        format!("cmd:{id}"),
        title,
        subtitle,
        icon,
        Some(Action::Internal {
            command: id.to_owned(),
        }),
    )
}

/// An information row (no action).
pub fn info(title: String, subtitle: Option<String>) -> ResultItem {
    item(
        format!("info:{title}"),
        title,
        subtitle,
        BuiltinIcon::Settings,
        None,
    )
}

pub fn fill(text: &str) -> Action {
    Action::Internal {
        command: format!("fill:{text}"),
    }
}

pub fn channel_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Main => "main",
        UpdateChannel::Stable => "stable",
    }
}

fn icon_of(c: &Command) -> BuiltinIcon {
    match c.run {
        Run::System(s) => flair_providers::system::icon(s),
        Run::Setting(_) => BuiltinIcon::Settings,
        _ if c.category == Category::Settings => BuiltinIcon::Settings,
        _ => BuiltinIcon::Command,
    }
}

/// The row of a command (`Enter` runs it, or writes it when it takes
/// arguments).
pub fn command_row(c: &Command, cx: &Context<'_>) -> Row {
    let typed = cx.typed(c.id).unwrap_or_else(|| format!("> {}", c.id));
    let takes_args = matches!(c.run, Run::Args | Run::Setting(_));
    let action = match c.run {
        Run::Action => Action::Internal {
            command: c.id.to_owned(),
        },
        Run::System(s) => Action::SystemCommand(s),
        Run::Args | Run::Setting(_) => fill(&format!("{typed} ")),
    };
    let subtitle = format!("{typed} — {}", cx.t(&format!("cmd-{}-desc", c.id)));
    Row {
        item: item(
            format!("cmd:{}", c.id),
            cx.t(&format!("cmd-{}", c.id)),
            Some(subtitle),
            icon_of(c),
            Some(action),
        ),
        completion: Some(if takes_args {
            format!("{typed} ")
        } else {
            typed
        }),
    }
}

/// Rows for the text typed after `>`.
pub fn rows(text: &str, cx: &Context<'_>) -> Vec<Row> {
    let text = text.trim_start();
    let mut out = Vec::new();
    if text.trim().is_empty() {
        if let Some(version) = cx.update_available {
            out.push(Row {
                item: row(
                    "update",
                    cx.ta("update-available-line", &[("version", version)]),
                    None,
                    BuiltinIcon::Command,
                ),
                completion: cx.typed("update"),
            });
        }
        for c in CATALOG.iter().filter(|c| cx.available(c)) {
            if FAVORITES.contains(&c.id) || (c.id == "install" && cx.portable) {
                out.push(command_row(c, cx));
            }
        }
        if let Some(c) = find("help") {
            out.push(command_row(c, cx));
        }
        return out;
    }
    // The command typed, then its arguments.
    if let Some((id, rest)) = cx.words.parse(text)
        && let Some(c) = find(id).filter(|c| cx.available(c))
    {
        out.extend(command_rows(c, &rest, cx));
    }
    // Other commands starting like this (`> redémarrer` → `redémarrer flair`).
    let parsed = cx.words.parse(text).map(|(id, _)| id.to_owned());
    for (id, _) in cx.words.complete(text) {
        if Some(id) == parsed.as_deref() {
            continue;
        }
        if let Some(c) = find(id).filter(|c| cx.available(c)) {
            out.push(command_row(c, cx));
        }
    }
    // Nothing recognized: search the help.
    if out.is_empty() {
        out = help::search(text, cx);
    }
    out
}

/// Rows of a recognized command and its arguments.
fn command_rows(c: &Command, rest: &str, cx: &Context<'_>) -> Vec<Row> {
    match c.run {
        Run::Action | Run::System(_) => vec![command_row(c, cx)],
        Run::Setting(key) => settings::setting_command(c, key, rest, cx),
        Run::Args => match c.id {
            "help" => help::rows(rest, cx),
            "config" => settings::config(rest, cx),
            "language" => settings::language(rest, cx),
            "files" => settings::files(rest, cx),
            "web" => settings::web(rest, cx),
            "calc" => settings::calc(rest, cx),
            "battery" => settings::battery(rest, cx),
            "theme" => theme::rows(rest, cx),
            "update" => update(c, rest, cx),
            _ => vec![command_row(c, cx)],
        },
    }
}

/// `> mise à jour`, `> mise à jour annuler`, `> mise à jour canal stable`.
fn update(c: &Command, rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed(c.id).unwrap_or_default();
    let Some((arg, value)) = cx.words.argument(rest) else {
        let mut r = command_row(c, cx);
        r.item.actions = vec![Action::Internal {
            command: "update".into(),
        }];
        return vec![r];
    };
    match arg {
        "rollback" => vec![Row {
            item: row(
                "update:rollback",
                cx.t("cmd-update-rollback"),
                Some(cx.t("cmd-update-rollback-desc")),
                BuiltinIcon::Command,
            ),
            completion: Some(format!(
                "{typed} {}",
                cx.words.argument_shown("rollback").unwrap_or_default()
            )),
        }],
        "channel" => ["main", "stable"]
            .iter()
            .filter(|name| value.is_empty() || name.starts_with(&value.to_lowercase()))
            .map(|name| Row {
                item: row(
                    &format!("update:channel:{name}"),
                    cx.ta("cmd-update-channel", &[("channel", name)]),
                    Some(cx.t("cmd-update-channel-desc")),
                    BuiltinIcon::Settings,
                ),
                completion: Some(format!(
                    "{typed} {} {name}",
                    cx.words.argument_shown("channel").unwrap_or_default()
                )),
            })
            .collect(),
        _ => vec![command_row(c, cx)],
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn context_parts(langs: &[&str], ui: Lang) -> (I18n, CommandWords, Config, Vec<Language>) {
        let config = Config {
            input_languages: langs.iter().map(|l| (*l).to_owned()).collect(),
            ..Config::default()
        };
        let none = std::path::Path::new("/nonexistent-flair-lexicons");
        let words = words(&config, none, ui);
        (I18n::new(ui), words, config, languages(none))
    }

    pub fn with<R>(langs: &[&str], ui: Lang, f: impl FnOnce(&Context<'_>) -> R) -> R {
        let (i18n, words, config, languages) = context_parts(langs, ui);
        let cx = Context {
            i18n: &i18n,
            words: &words,
            config: &config,
            languages: &languages,
            portable: false,
            update_available: None,
        };
        f(&cx)
    }

    pub fn actions(text: &str) -> Vec<String> {
        with(&["fr", "en"], Lang::Fr, |cx| {
            rows(text, cx)
                .into_iter()
                .filter_map(|r| match r.item.actions.first() {
                    Some(Action::Internal { command }) => Some(command.clone()),
                    Some(Action::SystemCommand(s)) => Some(format!("system:{s:?}")),
                    _ => None,
                })
                .collect()
        })
    }

    #[test]
    fn every_command_has_texts_and_words_in_both_languages() {
        for lang in [Lang::Fr, Lang::En] {
            let i18n = I18n::new(lang);
            for c in CATALOG {
                for suffix in ["", "-desc"] {
                    let key = format!("cmd-{}{suffix}", c.id);
                    let text = i18n.t(&key);
                    assert!(!text.is_empty() && text != key, "{lang:?}: {key}");
                }
                let key = format!("cmd-{}-example", c.id);
                assert!(i18n.has(&key), "{lang:?}: {key}");
            }
            for cat in Category::ALL {
                assert_ne!(i18n.t(cat.label_id()), cat.label_id());
            }
        }
        for lang in ["fr", "en"] {
            with(&[lang], Lang::Fr, |cx| {
                for c in CATALOG {
                    assert!(cx.words.has(c.id), "{lang}: no words for {}", c.id);
                }
            });
        }
    }

    #[test]
    fn commands_in_both_languages() {
        assert_eq!(actions("verrouiller"), ["system:Lock"]);
        assert_eq!(actions("lock"), ["system:Lock"]);
        assert_eq!(actions("stats"), ["stats"]);
        assert_eq!(actions("statistiques"), ["stats"]);
        assert_eq!(actions("réindexer"), ["reindex"]);
        assert_eq!(actions("mise à jour")[0], "update");
        assert_eq!(actions("update channel stable"), ["update:channel:stable"]);
        assert_eq!(actions("mise à jour canal main"), ["update:channel:main"]);
        assert_eq!(actions("mise a jour annuler"), ["update:rollback"]);
        let restart = actions("redémarrer");
        assert_eq!(restart[0], "system:Restart");
        assert!(restart.contains(&"restart_flair".to_owned()));
    }

    #[test]
    fn partial_words_complete_and_tab_writes_the_command() {
        let found = actions("verr");
        assert_eq!(found, ["system:Lock"]);
        with(&["fr", "en"], Lang::Fr, |cx| {
            let r = rows("transp", cx);
            assert_eq!(r[0].completion.as_deref(), Some("> transparence "));
            let r = rows("", cx);
            assert_eq!(r.last().map(|r| r.item.id.as_str()), Some("cmd:help"));
        });
        with(&["fr", "en"], Lang::En, |cx| {
            assert_eq!(rows("verr", cx)[0].completion.as_deref(), Some("> lock"));
        });
    }

    #[test]
    fn disabled_language_words_are_gone_but_help_stays() {
        with(&["en"], Lang::Fr, |cx| {
            let r = rows("verrouiller", cx);
            assert!(
                r.iter().all(|r| r.item.id != "cmd:lock"),
                "{:?}",
                r.iter().map(|r| &r.item.id).collect::<Vec<_>>()
            );
            assert_eq!(rows("lock", cx)[0].item.id, "cmd:lock");
            assert!(!rows("aide", cx).is_empty());
            assert!(!rows("langue", cx).is_empty());
        });
    }

    #[test]
    fn confirmation_is_asked_for_destructive_actions() {
        assert!(needs_confirmation(&Action::SystemCommand(
            SystemCommand::Shutdown
        )));
        assert!(!needs_confirmation(&Action::SystemCommand(
            SystemCommand::Lock
        )));
        assert!(needs_confirmation(&Action::Internal {
            command: "history_clear".into()
        }));
    }
}

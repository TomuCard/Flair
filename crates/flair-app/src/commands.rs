//! Flair's own `>` commands available so far (`> update`, `> export`…).
//!
//! The full command system (generic `> config`, `> help`, words from the
//! language files) comes with milestone 7; until then the French and English
//! words of these few commands are listed here.

use flair_core::i18n::I18n;
use flair_core::result::BuiltinIcon;
use flair_core::settings::config::UpdateChannel;
use flair_core::{Action, IconSource, ProviderKind, ResultItem};

/// One command: the id run by `Enter`, its words, and its texts.
struct Entry {
    id: &'static str,
    title: &'static str,
    description: Option<&'static str>,
    /// Words typed after `>`, French and English (accents optional).
    words: &'static [&'static str],
    /// Only listed when installing makes sense (portable mode).
    portable_only: bool,
}

const CATALOG: &[Entry] = &[
    Entry {
        id: "update",
        title: "cmd-update",
        description: Some("cmd-update-desc"),
        words: &["mise à jour", "maj", "mettre à jour", "update"],
        portable_only: false,
    },
    Entry {
        id: "update:rollback",
        title: "cmd-update-rollback",
        description: Some("cmd-update-rollback-desc"),
        words: &["mise à jour annuler", "update rollback", "rollback"],
        portable_only: false,
    },
    Entry {
        id: "version",
        title: "cmd-version",
        description: None,
        words: &["version"],
        portable_only: false,
    },
    Entry {
        id: "export",
        title: "cmd-export",
        description: Some("cmd-export-desc"),
        words: &["exporter", "export"],
        portable_only: false,
    },
    Entry {
        id: "import",
        title: "cmd-import",
        description: Some("cmd-import-desc"),
        words: &["importer", "import"],
        portable_only: false,
    },
    Entry {
        id: "undo",
        title: "cmd-undo",
        description: Some("cmd-undo-desc"),
        words: &["annuler", "undo"],
        portable_only: false,
    },
    Entry {
        id: "install",
        title: "cmd-install",
        description: None,
        words: &["installer", "install"],
        portable_only: true,
    },
    Entry {
        id: "uninstall",
        title: "cmd-uninstall",
        description: Some("cmd-uninstall-desc"),
        words: &["désinstaller", "uninstall"],
        portable_only: false,
    },
    Entry {
        id: "quit",
        title: "cmd-quit",
        description: None,
        words: &["quitter", "quit"],
        portable_only: false,
    },
];

/// Lowercase, accents removed, single spaces: `Mise à  JOUR` → `mise a jour`.
pub fn normalize(text: &str) -> String {
    let folded: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `> update channel stable` / `> mise à jour canal stable`.
fn channel_argument(text: &str) -> Option<UpdateChannel> {
    let rest = ["update channel ", "mise a jour canal ", "maj canal "]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))?;
    match rest.trim() {
        "main" => Some(UpdateChannel::Main),
        "stable" => Some(UpdateChannel::Stable),
        _ => None,
    }
}

pub fn channel_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Main => "main",
        UpdateChannel::Stable => "stable",
    }
}

/// A row running an internal command.
pub fn row(id: &str, title: String, subtitle: Option<String>, icon: BuiltinIcon) -> ResultItem {
    ResultItem {
        id: format!("cmd:{id}"),
        title,
        subtitle,
        icon: IconSource::Builtin(icon),
        score: 0.0,
        match_indices: vec![],
        actions: vec![Action::Internal {
            command: id.to_owned(),
        }],
        provider: ProviderKind::Command,
    }
}

/// An information row (no action).
pub fn info(title: String, subtitle: Option<String>) -> ResultItem {
    ResultItem {
        id: format!("info:{title}"),
        title,
        subtitle,
        icon: IconSource::Builtin(BuiltinIcon::Settings),
        score: 0.0,
        match_indices: vec![],
        actions: vec![],
        provider: ProviderKind::Command,
    }
}

/// Context deciding which commands are listed.
pub struct Context<'a> {
    pub i18n: &'a I18n,
    pub portable: bool,
    /// Version found by the last automatic check, if newer.
    pub update_available: Option<&'a str>,
}

/// Rows for the text typed after `>`.
pub fn rows(text: &str, cx: &Context<'_>) -> Vec<ResultItem> {
    let text = normalize(text);
    let mut rows = Vec::new();
    if text.is_empty()
        && let Some(version) = cx.update_available
    {
        rows.push(row(
            "update",
            cx.i18n
                .t_args("update-available-line", &[("version", version)]),
            None,
            BuiltinIcon::Command,
        ));
    }
    if let Some(channel) = channel_argument(&text) {
        let name = channel_name(channel);
        rows.push(row(
            &format!("update:channel:{name}"),
            cx.i18n.t_args("cmd-update-channel", &[("channel", name)]),
            Some(cx.i18n.t("cmd-update-channel-desc")),
            BuiltinIcon::Settings,
        ));
        return rows;
    }
    for entry in CATALOG {
        if entry.portable_only && !cx.portable {
            continue;
        }
        let matches = text.is_empty()
            || entry
                .words
                .iter()
                .map(|w| normalize(w))
                .any(|w| w.starts_with(&text) || text.starts_with(&w) && text.len() == w.len());
        if matches {
            rows.push(row(
                entry.id,
                cx.i18n.t(entry.title),
                entry.description.map(|d| cx.i18n.t(d)),
                BuiltinIcon::Command,
            ));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use flair_core::i18n::Lang;

    fn ids(text: &str, portable: bool) -> Vec<String> {
        let i18n = I18n::new(Lang::Fr);
        let cx = Context {
            i18n: &i18n,
            portable,
            update_available: None,
        };
        rows(text, &cx)
            .into_iter()
            .filter_map(|r| match r.actions.first() {
                Some(Action::Internal { command }) => Some(command.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn both_languages_and_optional_accents() {
        assert_eq!(ids("mise à jour", false)[0], "update");
        assert_eq!(ids("mise a jour", false)[0], "update");
        assert_eq!(ids("update", false)[0], "update");
        assert_eq!(ids("export", false), ["export"]);
        assert_eq!(ids("exporter", false), ["export"]);
        assert_eq!(ids("désinst", false), ["uninstall"]);
        assert_eq!(ids("desinst", false), ["uninstall"]);
        assert!(ids("zzz", false).is_empty());
    }

    #[test]
    fn channel_argument_in_both_languages() {
        assert_eq!(
            ids("update channel stable", false),
            ["update:channel:stable"]
        );
        assert_eq!(
            ids("mise à jour canal main", false),
            ["update:channel:main"]
        );
    }

    #[test]
    fn install_only_in_portable_mode_and_update_line_first() {
        assert!(!ids("", false).contains(&"install".to_owned()));
        assert!(ids("", true).contains(&"install".to_owned()));
        let i18n = I18n::new(Lang::En);
        let cx = Context {
            i18n: &i18n,
            portable: false,
            update_available: Some("0.5.0"),
        };
        let all = rows("", &cx);
        assert!(all[0].title.starts_with("Update available: 0.5.0"));
    }
}

//! Words of the `>` commands and of their arguments, read from the active
//! language files (`[commands]`, `[arguments]`). With French disabled,
//! `> verrouiller` is no longer a command but `> lock` still is; `> help`,
//! `> config` and `> langue` live in `common.toml` and always work.

use crate::lexicon::{Lexicon, fold};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Form {
    /// Folded words (`mise`, `a`, `jour`).
    words: Vec<String>,
    shown: String,
    id: String,
    /// 0 for the interface language, then the other files in order.
    rank: usize,
    /// Position in its file's list (the first one is the usual word).
    order: usize,
}

/// The command and argument words of the active languages.
#[derive(Debug, Clone, Default)]
pub struct CommandWords {
    commands: Vec<Form>,
    arguments: Vec<Form>,
}

fn words_of(text: &str) -> Vec<String> {
    text.split_whitespace().map(fold).collect()
}

fn forms(lexicons: &[&Lexicon], ui: &str, arguments: bool) -> Vec<Form> {
    let mut out = Vec::new();
    for (index, lexicon) in lexicons.iter().enumerate() {
        let rank = if lexicon.meta.code == ui {
            0
        } else {
            index + 1
        };
        let table = if arguments {
            &lexicon.arguments
        } else {
            &lexicon.commands
        };
        for (id, list) in table {
            for (order, shown) in list.iter().enumerate() {
                out.push(Form {
                    words: words_of(shown),
                    shown: shown.clone(),
                    id: id.clone(),
                    rank,
                    order,
                });
            }
        }
    }
    // Longest forms first: `redémarrer flair` before `redémarrer`.
    out.sort_by(|a, b| b.words.len().cmp(&a.words.len()).then(a.rank.cmp(&b.rank)));
    out
}

/// Longest form starting `typed` (whole words): `(id, words used)`.
fn longest<'a>(forms: &'a [Form], typed: &[String]) -> Option<(&'a str, usize)> {
    forms
        .iter()
        .find(|f| typed.len() >= f.words.len() && typed[..f.words.len()] == f.words[..])
        .map(|f| (f.id.as_str(), f.words.len()))
}

impl CommandWords {
    /// `lexicons`: `common` and the active input languages; `ui`: code of
    /// the interface language (its words are shown first).
    pub fn new(lexicons: &[&Lexicon], ui: &str) -> Self {
        Self {
            commands: forms(lexicons, ui, false),
            arguments: forms(lexicons, ui, true),
        }
    }

    /// The command typed and what follows it: `réglages transparence 0.9` →
    /// `("config", "transparence 0.9")`.
    pub fn parse(&self, text: &str) -> Option<(&str, String)> {
        let original: Vec<&str> = text.split_whitespace().collect();
        let typed: Vec<String> = original.iter().map(|w| fold(w)).collect();
        let (id, used) = longest(&self.commands, &typed)?;
        Some((id, original[used..].join(" ")))
    }

    /// An argument at the start of `text`: `(id, rest)`.
    pub fn argument(&self, text: &str) -> Option<(&str, String)> {
        let original: Vec<&str> = text.split_whitespace().collect();
        let typed: Vec<String> = original.iter().map(|w| fold(w)).collect();
        let (id, used) = longest(&self.arguments, &typed)?;
        Some((id, original[used..].join(" ")))
    }

    /// True when `text` is exactly one of the forms of argument `id`.
    pub fn is_argument(&self, text: &str, id: &str) -> bool {
        self.argument(text)
            .is_some_and(|(found, rest)| found == id && rest.is_empty())
    }

    /// Commands one of whose forms starts with `text` (autocompletion):
    /// `(id, form)`, one per command, interface language first.
    pub fn complete(&self, text: &str) -> Vec<(&str, &str)> {
        let typed = fold(text.trim());
        let mut out: Vec<(&Form, bool)> = Vec::new();
        for f in &self.commands {
            let joined = f.words.join(" ");
            if !joined.starts_with(&typed) {
                continue;
            }
            match out.iter_mut().find(|(o, _)| o.id == f.id) {
                Some(existing) => {
                    if (f.rank, f.order) < (existing.0.rank, existing.0.order) {
                        existing.0 = f;
                    }
                }
                None => out.push((f, true)),
            }
        }
        out.iter()
            .map(|(f, _)| (f.id.as_str(), f.shown.as_str()))
            .collect()
    }

    /// The form shown for a command (interface language first, shortest).
    pub fn shown(&self, id: &str) -> Option<&str> {
        best(&self.commands, id)
    }

    pub fn argument_shown(&self, id: &str) -> Option<&str> {
        best(&self.arguments, id)
    }

    /// Every form of a command (for help and searching).
    pub fn all_forms(&self, id: &str) -> Vec<&str> {
        self.commands
            .iter()
            .filter(|f| f.id == id)
            .map(|f| f.shown.as_str())
            .collect()
    }

    pub fn has(&self, id: &str) -> bool {
        self.commands.iter().any(|f| f.id == id)
    }
}

/// First form of `id` in the best-ranked file (files list their usual word first).
fn best<'a>(forms: &'a [Form], id: &str) -> Option<&'a str> {
    forms
        .iter()
        .filter(|f| f.id == id)
        .min_by_key(|f| (f.rank, f.order))
        .map(|f| f.shown.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexicon::{COMMON, builtin};

    fn words(langs: &[&str], ui: &str) -> CommandWords {
        let all: Vec<Lexicon> = std::iter::once(COMMON)
            .chain(langs.iter().copied())
            .filter_map(builtin)
            .collect();
        let refs: Vec<&Lexicon> = all.iter().collect();
        CommandWords::new(&refs, ui)
    }

    #[test]
    fn parses_commands_in_both_languages() {
        let w = words(&["fr", "en"], "fr");
        assert_eq!(w.parse("verrouiller"), Some(("lock", String::new())));
        assert_eq!(w.parse("lock"), Some(("lock", String::new())));
        assert_eq!(
            w.parse("Réglages Transparence 0.9"),
            Some(("config", "Transparence 0.9".into()))
        );
        assert_eq!(
            w.parse("redemarrer flair"),
            Some(("restart_flair", String::new()))
        );
        assert_eq!(w.parse("redémarrer"), Some(("restart", String::new())));
        assert_eq!(
            w.parse("mise à jour canal stable"),
            Some(("update", "canal stable".into()))
        );
        assert_eq!(w.parse("verrouil"), None);
    }

    #[test]
    fn disabled_languages_lose_their_words_but_not_the_common_ones() {
        let en = words(&["en"], "fr");
        assert_eq!(en.parse("verrouiller"), None);
        assert_eq!(en.parse("lock"), Some(("lock", String::new())));
        assert_eq!(en.parse("aide"), Some(("help", String::new())));
        assert_eq!(
            en.parse("langue fr activer"),
            Some(("language", "fr activer".into()))
        );
        assert_eq!(en.parse("config"), Some(("config", String::new())));
    }

    #[test]
    fn arguments_and_completion() {
        let w = words(&["fr", "en"], "fr");
        assert_eq!(w.argument("par défaut"), Some(("default", String::new())));
        assert_eq!(w.argument("ajouter .venv"), Some(("add", ".venv".into())));
        assert!(w.is_argument("on", "on"));
        assert!(w.is_argument("Activer", "on"));
        assert!(w.is_argument("disable", "off"));
        let ids: Vec<&str> = w.complete("ver").iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"lock") && ids.contains(&"version"), "{ids:?}");
        assert_eq!(w.shown("lock"), Some("verrouiller"));
        assert_eq!(words(&["fr", "en"], "en").shown("lock"), Some("lock"));
        assert_eq!(w.argument_shown("on"), Some("oui"));
    }
}

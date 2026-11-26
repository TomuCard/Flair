//! Generation of documents from the registry: the commented default
//! `config.toml` and the Markdown settings table of the README.

use std::fmt::Write as _;

use super::config::{CONFIG_VERSION, Config};
use super::registry::{ListItem, Localized, REGISTRY, SettingDescriptor, SettingKind};
use crate::i18n::Lang;

/// Column where trailing comments start, for readability.
const COMMENT_COLUMN: usize = 28;

const HEADER: Localized = Localized::new(
    "# Configuration de Flair.\n\
     # Chaque réglage est aussi modifiable depuis Flair : tapez « > config ».\n\
     # Ce fichier est relu automatiquement quand il change. Une erreur ne bloque\n\
     # jamais Flair : la valeur fautive est remplacée par sa valeur par défaut.",
    "# Flair configuration.\n\
     # Every setting can also be changed from Flair: type \"> config\".\n\
     # This file is reloaded automatically when it changes. An error never blocks\n\
     # Flair: the faulty value is replaced by its default.",
);

const VERSION_COMMENT: Localized = Localized::new(
    "géré par Flair pour les migrations, ne pas modifier",
    "managed by Flair for migrations, do not edit",
);

const RESTART: Localized =
    Localized::new("appliqué au prochain démarrage", "applied at next start");

const WEB_COMMENT: Localized = Localized::new(
    "# Moteurs de recherche web : tapez le mot-clé puis votre recherche (« g rust iced »).\n\
     # {query} est remplacé par la recherche.",
    "# Web search engines: type the keyword then your search (\"g rust iced\").\n\
     # {query} is replaced by the search.",
);

/// Human readable constraint appended to a setting's comment.
fn constraint(desc: &SettingDescriptor, lang: Lang) -> Option<String> {
    match desc.kind {
        SettingKind::Choice(choices) => Some(choices.join(" | ")),
        SettingKind::Integer { min, max } => Some(match lang {
            Lang::Fr => format!("{min} à {max}"),
            Lang::En => format!("{min} to {max}"),
        }),
        SettingKind::Float { min, max } => Some(match lang {
            Lang::Fr => format!("{min} à {max}"),
            Lang::En => format!("{min} to {max}"),
        }),
        SettingKind::List {
            item: ListItem::Language,
            ..
        } => Some("> langue / > language".into()),
        _ => None,
    }
}

fn line(out: &mut String, key: &str, value: &toml::Value, comment: &str) {
    let assignment = format!("{key} = {value}");
    let pad = COMMENT_COLUMN
        .saturating_sub(assignment.chars().count())
        .max(1);
    let _ = writeln!(out, "{assignment}{:pad$}# {comment}", "");
}

/// Commented default configuration file, in the given interface language.
/// Parsing it yields exactly [`Config::default`] (tested).
pub fn default_config_text(lang: Lang) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}\n", HEADER.get(lang));
    line(
        &mut out,
        "config_version",
        &toml::Value::Integer(i64::from(CONFIG_VERSION)),
        VERSION_COMMENT.get(lang),
    );

    let mut current_section: Option<&str> = None;
    let mut web = None;
    for desc in REGISTRY {
        if desc.kind == SettingKind::WebEngines {
            web = Some(desc);
            continue;
        }
        let Some(value) = desc.default_value() else {
            continue;
        };
        if desc.section() != current_section {
            current_section = desc.section();
            if let Some(section) = current_section {
                let _ = writeln!(out, "\n[{section}]");
            }
        }
        let mut comment = desc.description.get(lang).to_owned();
        if let Some(c) = constraint(desc, lang) {
            let _ = write!(comment, " ({c})");
        }
        if desc.requires_restart {
            let _ = write!(comment, " — {}", RESTART.get(lang));
        }
        line(&mut out, desc.leaf(), &value, &comment);
    }

    if web.is_some() {
        let _ = writeln!(out, "\n{}", WEB_COMMENT.get(lang));
        for engine in Config::default().web {
            let _ = writeln!(
                out,
                "[[web]]\nkeyword = {}\nname = {}\nurl = {}\n",
                toml::Value::String(engine.keyword),
                toml::Value::String(engine.name),
                toml::Value::String(engine.url),
            );
        }
    }
    out
}

/// Markdown table of all settings, for the README.
pub fn settings_markdown(lang: Lang) -> String {
    let mut out = String::new();
    let (key, default, desc) = match lang {
        Lang::Fr => ("Clé", "Défaut", "Description"),
        Lang::En => ("Key", "Default", "Description"),
    };
    let _ = writeln!(out, "| {key} | {default} | {desc} |\n|---|---|---|");
    for d in REGISTRY {
        let default = match d.kind {
            SettingKind::WebEngines => "g, ddg, wiki, yt".to_owned(),
            _ => d.default_value().map(|v| v.to_string()).unwrap_or_default(),
        };
        let mut description = d.description.get(lang).to_owned();
        if let Some(c) = constraint(d, lang) {
            let _ = write!(description, " ({c})");
        }
        let _ = writeln!(
            out,
            "| `{}` | `{}` | {} |",
            d.key,
            default.replace('|', "\\|"),
            description.replace('|', "\\|")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::parse_config;

    #[test]
    fn default_file_parses_back_to_defaults_in_both_languages() {
        for lang in [Lang::Fr, Lang::En] {
            let text = default_config_text(lang);
            let out = parse_config(&text);
            assert!(out.issues.is_empty(), "{lang:?}: {:?}\n{text}", out.issues);
            assert_eq!(out.config, Config::default(), "{lang:?}");
        }
    }

    #[test]
    fn default_file_is_commented_in_the_requested_language() {
        assert!(default_config_text(Lang::Fr).contains("Langue de l'interface"));
        assert!(default_config_text(Lang::En).contains("Language of the interface"));
    }

    #[test]
    fn markdown_lists_every_setting() {
        let md = settings_markdown(Lang::En);
        for d in REGISTRY {
            assert!(md.contains(&format!("`{}`", d.key)), "{}", d.key);
        }
    }
}

//! Settings: typed configuration, the single settings registry, loading and
//! generation of the commented default `config.toml`.

pub mod config;
pub mod edit;
pub mod generate;
pub mod hotkey;
pub mod migrate;
pub mod registry;

use std::path::Path;

pub use config::Config;
pub use hotkey::Hotkey;
pub use registry::{REGISTRY, SettingDescriptor, SettingKind, ValueError};

use crate::i18n::Lang;

/// A problem found while loading the configuration. Never fatal: the faulty
/// part falls back to its default value.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ConfigIssue {
    #[error("cannot read the configuration file: {0}")]
    Io(String),
    #[error("syntax error in the configuration: {0}")]
    Syntax(String),
    #[error("invalid value for `{key}`: {error}")]
    InvalidValue { key: String, error: ValueError },
    #[error("unknown setting `{0}` (ignored)")]
    UnknownKey(String),
}

/// Result of loading: always a usable config, plus the issues to report.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    pub config: Config,
    pub issues: Vec<ConfigIssue>,
}

impl LoadOutcome {
    /// True if the file could not be parsed at all (the caller should keep the
    /// last valid configuration instead of `config`).
    pub fn is_syntax_error(&self) -> bool {
        self.issues
            .iter()
            .any(|i| matches!(i, ConfigIssue::Syntax(_) | ConfigIssue::Io(_)))
    }
}

/// Parses a configuration text. Invalid values are replaced by their defaults
/// and reported; a syntax error yields the default configuration.
pub fn parse_config(text: &str) -> LoadOutcome {
    let mut issues = Vec::new();
    let mut table: toml::Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(e) => {
            return LoadOutcome {
                config: Config::default(),
                issues: vec![ConfigIssue::Syntax(e.message().to_owned())],
            };
        }
    };

    // Validate every known setting with the registry rules.
    for desc in REGISTRY {
        let value = registry::lookup_table(&table, desc.key);
        if let Some(value) = value
            && let Err(error) = desc.validate(value)
        {
            issues.push(ConfigIssue::InvalidValue {
                key: desc.key.to_owned(),
                error,
            });
            registry::remove_key(&mut table, desc.key);
        }
    }

    // Report unknown keys (typos), then drop them.
    for key in registry::unknown_keys(&table) {
        registry::remove_key(&mut table, &key);
        issues.push(ConfigIssue::UnknownKey(key));
    }

    match toml::Value::Table(table).try_into::<Config>() {
        Ok(config) => LoadOutcome { config, issues },
        Err(e) => {
            issues.push(ConfigIssue::Syntax(e.message().to_owned()));
            LoadOutcome {
                config: Config::default(),
                issues,
            }
        }
    }
}

/// Loads the configuration file, creating it with commented defaults (in the
/// interface language) if it does not exist yet.
pub fn load_or_create(path: &Path, lang: Lang) -> LoadOutcome {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_config(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let text = generate::default_config_text(lang);
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(path, text));
            let issues = match written {
                Ok(()) => {
                    tracing::info!(path = %path.display(), "default configuration created");
                    Vec::new()
                }
                Err(e) => vec![ConfigIssue::Io(e.to_string())],
            };
            LoadOutcome {
                config: Config::default(),
                issues,
            }
        }
        Err(e) => LoadOutcome {
            config: Config::default(),
            issues: vec![ConfigIssue::Io(e.to_string())],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let out = parse_config("");
        assert_eq!(out.config, Config::default());
        assert!(out.issues.is_empty());
    }

    #[test]
    fn syntax_error_never_prevents_startup() {
        let out = parse_config("max_results = = 3\n[theme");
        assert_eq!(out.config, Config::default());
        assert!(out.is_syntax_error());
    }

    #[test]
    fn invalid_values_fall_back_to_defaults_individually() {
        let out = parse_config(
            "max_results = 99\nkeep_last_query = true\n[theme]\nopacity = 0.5\nmode = \"dark\"\n",
        );
        assert_eq!(out.config.max_results, 8);
        assert!(out.config.keep_last_query);
        assert_eq!(out.config.theme.opacity, 1.0);
        assert_eq!(out.config.theme.mode, config::ThemeMode::Dark);
        assert_eq!(out.issues.len(), 2);
        assert!(!out.is_syntax_error());
    }

    #[test]
    fn unknown_keys_are_reported_and_ignored() {
        let out = parse_config("max_resultz = 3\n[theme]\nopacityy = 1\n[foo]\nbar = 1\n");
        assert_eq!(out.config, Config::default());
        let unknown: Vec<_> = out
            .issues
            .iter()
            .filter_map(|i| match i {
                ConfigIssue::UnknownKey(k) => Some(k.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(unknown, ["foo", "max_resultz", "theme.opacityy"]);
    }

    #[test]
    fn web_engines_are_parsed() {
        let out = parse_config(
            "[[web]]\nkeyword = \"gh\"\nname = \"GitHub\"\nurl = \"https://github.com/search?q={query}\"\n",
        );
        assert!(out.issues.is_empty(), "{:?}", out.issues);
        assert_eq!(out.config.web.len(), 1);
        assert_eq!(out.config.web[0].keyword, "gh");
    }

    #[test]
    fn creates_the_default_file() {
        let dir = std::env::temp_dir().join(format!("flair-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_file(&path);
        let out = load_or_create(&path, Lang::Fr);
        assert!(out.issues.is_empty());
        let reread = load_or_create(&path, Lang::Fr);
        assert_eq!(reread.config, Config::default());
        assert!(reread.issues.is_empty(), "{:?}", reread.issues);
        let _ = std::fs::remove_dir_all(dir);
    }
}

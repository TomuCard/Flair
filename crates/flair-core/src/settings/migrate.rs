//! Configuration migrations: `config_version` tells the format of a file;
//! older files go through every migration in order (v1 → v2 → v3…), each one
//! written here and tested. They edit the TOML with `toml_edit`, so comments
//! and layout are kept. Never a reinstallation for a format change.

use toml_edit::{DocumentMut, Item, value};

use super::config::CONFIG_VERSION;

/// One step: transforms a document of version `from` into `from + 1`.
pub struct Migration {
    pub from: u32,
    pub description: &'static str,
    pub apply: fn(&mut DocumentMut),
}

/// All migrations, in order.
pub static MIGRATIONS: &[Migration] = &[Migration {
    from: 1,
    description: "remove theme.animations (window animations removed)",
    apply: |doc| {
        if let Some(theme) = doc.get_mut("theme").and_then(Item::as_table_mut) {
            theme.remove("animations");
        }
    },
}];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MigrateError {
    #[error("the configuration cannot be parsed: {0}")]
    Syntax(String),
    /// Written by a more recent Flair: left untouched.
    #[error("configuration version {found} is newer than this Flair ({supported})")]
    Newer { found: u32, supported: u32 },
}

/// Outcome of a migration that changed something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migrated {
    pub text: String,
    pub from: u32,
    pub to: u32,
}

/// Version declared by a configuration text (1 when absent: first format).
pub fn version_of(doc: &DocumentMut) -> u32 {
    doc.get("config_version")
        .and_then(Item::as_integer)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1)
}

/// Brings `text` to the current version. `Ok(None)`: already up to date.
pub fn migrate(text: &str) -> Result<Option<Migrated>, MigrateError> {
    migrate_with(text, MIGRATIONS, CONFIG_VERSION)
}

/// Same as [`migrate`] with an explicit chain (tests use longer chains).
pub fn migrate_with(
    text: &str,
    migrations: &[Migration],
    target: u32,
) -> Result<Option<Migrated>, MigrateError> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| MigrateError::Syntax(e.to_string()))?;
    let from = version_of(&doc);
    if from > target {
        return Err(MigrateError::Newer {
            found: from,
            supported: target,
        });
    }
    if from == target {
        return Ok(None);
    }
    for version in from..target {
        if let Some(m) = migrations.iter().find(|m| m.from == version) {
            tracing::info!(
                from = version,
                to = version + 1,
                "config migration: {}",
                m.description
            );
            (m.apply)(&mut doc);
        }
    }
    // Keep the existing comment next to `config_version`.
    match doc.get_mut("config_version").and_then(Item::as_value_mut) {
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = i64::from(target).into();
            *existing.decor_mut() = decor;
        }
        None => {
            doc.insert("config_version", value(i64::from(target)));
        }
    }
    Ok(Some(Migrated {
        text: doc.to_string(),
        from,
        to: target,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::settings::{generate, parse_config};

    const V1: &str = "# my config\nconfig_version = 1   # managed by Flair\nmax_results = 5\n\n[theme]\nmode = \"dark\" # dark please\nanimations = false\n";

    #[test]
    fn v1_to_current_removes_animations_and_keeps_comments() {
        let out = migrate(V1).expect("ok").expect("changed");
        assert_eq!((out.from, out.to), (1, CONFIG_VERSION));
        assert!(!out.text.contains("animations"));
        assert!(out.text.contains("# my config"));
        assert!(out.text.contains("mode = \"dark\" # dark please"));
        assert!(out.text.contains(&format!(
            "config_version = {CONFIG_VERSION}   # managed by Flair"
        )));
        let parsed = parse_config(&out.text);
        assert!(parsed.issues.is_empty(), "{:?}", parsed.issues);
        assert_eq!(parsed.config.max_results, 5);
    }

    #[test]
    fn up_to_date_and_missing_version() {
        let current = generate::default_config_text(Lang::Fr);
        assert_eq!(migrate(&current), Ok(None));
        // No `config_version` at all: treated as version 1.
        let out = migrate("[theme]\nanimations = true\n")
            .expect("ok")
            .expect("changed");
        assert_eq!(out.from, 1);
        assert!(
            out.text
                .contains(&format!("config_version = {CONFIG_VERSION}"))
        );
    }

    #[test]
    fn newer_config_is_left_alone() {
        let text = format!("config_version = {}\n", CONFIG_VERSION + 1);
        assert!(matches!(migrate(&text), Err(MigrateError::Newer { .. })));
    }

    #[test]
    fn chains_v1_to_v3() {
        static CHAIN: &[Migration] = &[
            Migration {
                from: 1,
                description: "rename a → b",
                apply: |doc| {
                    if let Some(v) = doc.remove("a") {
                        doc.insert("b", v);
                    }
                },
            },
            Migration {
                from: 2,
                description: "b doubles",
                apply: |doc| {
                    let b = doc.get("b").and_then(Item::as_integer).unwrap_or(0);
                    doc.insert("b", value(b * 2));
                },
            },
        ];
        let out = migrate_with("config_version = 1\na = 21 # kept\n", CHAIN, 3)
            .expect("ok")
            .expect("changed");
        assert_eq!((out.from, out.to), (1, 3));
        let doc: DocumentMut = out.text.parse().expect("valid");
        assert_eq!(doc["b"].as_integer(), Some(42));
        assert_eq!(version_of(&doc), 3);
        // From v2, only the second step runs.
        let out = migrate_with("config_version = 2\nb = 5\n", CHAIN, 3)
            .expect("ok")
            .expect("changed");
        assert!(out.text.contains("b = 10"));
    }

    #[test]
    fn every_version_below_current_has_a_migration() {
        for version in 1..CONFIG_VERSION {
            assert!(
                MIGRATIONS.iter().any(|m| m.from == version),
                "missing migration from v{version}"
            );
        }
    }
}

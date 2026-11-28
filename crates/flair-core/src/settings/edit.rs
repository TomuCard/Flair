//! Writing a setting into `config.toml` while keeping comments and layout
//! (`toml_edit`). Values are validated with the registry first: an invalid
//! value is refused and nothing is written.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use super::registry::{self, ValueError};

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("unknown setting `{0}`")]
    UnknownKey(String),
    #[error(transparent)]
    Invalid(#[from] ValueError),
    #[error("the configuration file cannot be parsed: {0}")]
    Syntax(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn to_edit_value(value: &toml::Value) -> Option<Value> {
    Some(match value {
        toml::Value::String(s) => Value::from(s.as_str()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::from(*f),
        toml::Value::Boolean(b) => Value::from(*b),
        toml::Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(to_edit_value(item)?);
            }
            Value::Array(array)
        }
        // Tables and datetimes are not used by simple settings.
        _ => return None,
    })
}

/// Sets `key` to `value` in the configuration text; returns the new text.
pub fn set_in_text(text: &str, key: &str, value: &toml::Value) -> Result<String, EditError> {
    let desc = registry::find(key).ok_or_else(|| EditError::UnknownKey(key.to_owned()))?;
    desc.validate(value)?;
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| EditError::Syntax(e.to_string()))?;
    // `[[web]]` blocks: an array of tables, rewritten whole.
    if desc.kind == registry::SettingKind::WebEngines {
        let mut blocks = toml_edit::ArrayOfTables::new();
        for item in value.as_array().into_iter().flatten() {
            let mut table = Table::new();
            for (k, v) in item.as_table().into_iter().flatten() {
                if let Some(v) = to_edit_value(v) {
                    table.insert(k, Item::Value(v));
                }
            }
            blocks.push(table);
        }
        doc.insert(key, Item::ArrayOfTables(blocks));
        return Ok(doc.to_string());
    }
    let new_value = to_edit_value(value).ok_or_else(|| EditError::UnknownKey(key.to_owned()))?;

    let table: &mut Table = match desc.section() {
        None => doc.as_table_mut(),
        Some(section) => {
            if !doc.contains_key(section) {
                doc.insert(section, Item::Table(Table::new()));
            }
            doc[section]
                .as_table_mut()
                .ok_or_else(|| EditError::Syntax(format!("[{section}] is not a table")))?
        }
    };
    let leaf = desc.leaf();
    match table.get_mut(leaf).and_then(Item::as_value_mut) {
        // Keep the comment and spacing around the existing value.
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = new_value;
            *existing.decor_mut() = decor;
        }
        None => {
            table.insert(leaf, Item::Value(new_value));
        }
    }
    Ok(doc.to_string())
}

/// Sets `key` to `value` in the configuration file (created if missing).
pub fn set_in_file(path: &Path, key: &str, value: &toml::Value) -> Result<(), EditError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let updated = set_in_text(&text, key, value)?;
    // Write to a temporary file, then rename: never a half-written config.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, updated)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::settings::{generate, parse_config};

    #[test]
    fn keeps_comments_and_layout() {
        let text = generate::default_config_text(Lang::Fr);
        let updated = set_in_text(&text, "autostart", &toml::Value::Boolean(true)).expect("ok");
        let line = updated
            .lines()
            .find(|l| l.starts_with("autostart"))
            .expect("line kept");
        assert!(line.starts_with("autostart = true "), "{line}");
        assert!(
            line.ends_with("# Lancer Flair à l'ouverture de la session"),
            "{line}"
        );
        assert!(updated.contains("# Configuration de Flair."));
        assert_eq!(updated.lines().count(), text.lines().count());
        assert!(parse_config(&updated).config.autostart);
    }

    #[test]
    fn sets_nested_keys_and_creates_missing_sections() {
        let updated = set_in_text("", "theme.opacity", &toml::Value::Float(0.9)).expect("ok");
        assert_eq!(parse_config(&updated).config.theme.opacity, 0.9);
        let updated = set_in_text(&updated, "theme.mode", &"dark".into()).expect("ok");
        let config = parse_config(&updated).config;
        assert_eq!(config.theme.opacity, 0.9);
        assert_eq!(config.theme.mode, crate::settings::config::ThemeMode::Dark);
    }

    #[test]
    fn refuses_invalid_values_and_unknown_keys() {
        assert!(matches!(
            set_in_text("", "max_results", &toml::Value::Integer(99)),
            Err(EditError::Invalid(ValueError::OutOfRange { .. }))
        ));
        assert!(matches!(
            set_in_text("", "nope", &toml::Value::Boolean(true)),
            Err(EditError::UnknownKey(_))
        ));
        assert!(matches!(
            set_in_text("x = = 1", "autostart", &toml::Value::Boolean(true)),
            Err(EditError::Syntax(_))
        ));
    }

    #[test]
    fn rewrites_web_engines() {
        let text = "# mine\nmax_results = 8\n\n[[web]]\nkeyword = \"g\"\nname = \"Google\"\nurl = \"https://g/?q={query}\"\n";
        let engines: toml::Value = toml::from_str::<toml::Table>(
            "web = [{keyword = \"g\", name = \"Google\", url = \"https://g/?q={query}\"}, {keyword = \"yt\", name = \"YouTube\", url = \"https://yt/?q={query}\"}]",
        )
        .expect("toml")
        .remove("web")
        .expect("web");
        let updated = set_in_text(text, "web", &engines).expect("ok");
        assert!(updated.starts_with("# mine\nmax_results = 8"));
        let config = parse_config(&updated).config;
        assert_eq!(config.web.len(), 2);
        assert_eq!(config.web[1].keyword, "yt");
    }

    #[test]
    fn writes_the_file_atomically() {
        let dir = std::env::temp_dir().join(format!("flair-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("config.toml");
        std::fs::write(&path, "# mine\nautostart = false # keep me\n").expect("write");
        set_in_file(&path, "autostart", &toml::Value::Boolean(true)).expect("ok");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(text, "# mine\nautostart = true # keep me\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}

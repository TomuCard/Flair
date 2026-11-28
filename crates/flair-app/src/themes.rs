//! The themes: the two built-in ones plus the user's files (`themes/*.toml`),
//! and the choice of the active one (mode `system` follows the system's
//! dark or light preference, live).

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU8, Ordering};

use flair_core::settings::Config;
use flair_core::settings::config::ThemeMode;
use flair_core::theme::{Color, Theme, builtin};

/// System preference: 0 unknown, 1 dark, 2 light.
static SYSTEM: AtomicU8 = AtomicU8::new(0);
/// Every theme known (built-in first), reloaded with the theme folder.
static THEMES: RwLock<Vec<Theme>> = RwLock::new(Vec::new());

pub fn set_system_dark(dark: Option<bool>) {
    SYSTEM.store(
        match dark {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        Ordering::Relaxed,
    );
}

/// True when the system prefers dark (unknown: dark, Flair's default).
pub fn system_dark() -> bool {
    SYSTEM.load(Ordering::Relaxed) != 2
}

/// Reads the theme folder. Invalid files are skipped: returned with the
/// reason, they never break the active theme.
pub fn reload(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut all: Vec<Theme> = builtin::all().to_vec();
    let mut invalid = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    for file in files {
        match std::fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| Theme::from_toml(&text).map_err(|e| e.to_string()))
        {
            Ok(theme) => {
                // A file with the name of a theme replaces it.
                all.retain(|t| t.name != theme.name);
                all.push(theme);
            }
            Err(e) => {
                tracing::warn!(file = %file.display(), "theme ignored: {e}");
                invalid.push((file, e));
            }
        }
    }
    if let Ok(mut themes) = THEMES.write() {
        *themes = all;
    }
    invalid
}

/// Every theme, built-in first.
pub fn all() -> Vec<Theme> {
    let themes = THEMES.read().map(|t| t.clone()).unwrap_or_default();
    if themes.is_empty() {
        builtin::all().to_vec()
    } else {
        themes
    }
}

pub fn find(name: &str) -> Option<Theme> {
    all().into_iter().find(|t| t.name == name)
}

/// True when the configuration shows its light theme now.
pub fn light_now(config: &Config) -> bool {
    match config.theme.mode {
        ThemeMode::Light => true,
        ThemeMode::Dark => false,
        ThemeMode::System => !system_dark(),
    }
}

/// The theme shown for the configuration and the system preference.
pub fn active(config: &Config) -> Theme {
    let light = light_now(config);
    let name = if light {
        &config.theme.light
    } else {
        &config.theme.dark
    };
    find(name).unwrap_or_else(|| {
        tracing::warn!(theme = %name, "unknown theme, using the built-in one");
        if light {
            builtin::light()
        } else {
            builtin::dark()
        }
    })
}

/// `perso-1`, `perso-2`… : the first free name.
pub fn free_name(prefix: &str) -> String {
    let taken = all();
    (1..)
        .map(|n| format!("{prefix}-{n}"))
        .find(|name| !taken.iter().any(|t| &t.name == name))
        .unwrap_or_else(|| prefix.to_owned())
}

/// File name of a theme: lowercase, letters, digits and `-`.
pub fn file_name(name: &str) -> String {
    let slug: String = flair_core::lexicon::fold(name)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_owned();
    format!(
        "{}.toml",
        if slug.is_empty() {
            "theme".into()
        } else {
            slug
        }
    )
}

/// The file of a new theme.
pub fn to_toml(theme: &Theme) -> String {
    let hex = |c: Color| c.to_hex();
    let mut text = format!(
        "name = {:?}\nbackground = \"{}\"\nforeground = \"{}\"\n",
        theme.name,
        hex(theme.background),
        hex(theme.foreground)
    );
    if let Some(accent) = theme.accent {
        text.push_str(&format!("accent = \"{}\"\n", hex(accent)));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_add_themes_and_invalid_ones_are_skipped() {
        let dir = std::env::temp_dir().join(format!("flair-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(
            dir.join("lavande.toml"),
            "name = \"Lavande\"\nbackground = \"#1E1B2E\"\nforeground = \"#E6E1F5\"\naccent = \"#B79CFF\"\n",
        )
        .expect("write");
        std::fs::write(
            dir.join("bad.toml"),
            "name = \"Bad\"\nbackground = \"nope\"\n",
        )
        .expect("write");
        let invalid = reload(&dir);
        assert_eq!(invalid.len(), 1);
        assert!(find("Lavande").is_some_and(|t| !t.is_light()));
        let config = Config {
            theme: flair_core::settings::config::ThemeConfig {
                mode: ThemeMode::Dark,
                dark: "Lavande".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(active(&config).name, "Lavande");
        let round = Theme::from_toml(&to_toml(&find("Lavande").expect("theme"))).expect("valid");
        assert_eq!(round.name, "Lavande");
        assert_eq!(file_name("Été Doux!"), "ete-doux.toml");
        let _ = std::fs::remove_dir_all(dir);
        reload(Path::new("/nonexistent"));
    }

    #[test]
    fn system_mode_follows_the_preference() {
        let config = Config::default();
        set_system_dark(Some(false));
        assert!(light_now(&config));
        set_system_dark(Some(true));
        assert!(!light_now(&config));
        set_system_dark(None);
        assert!(!light_now(&config));
    }
}

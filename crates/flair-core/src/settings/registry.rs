//! The single settings registry.
//!
//! Each setting is described **once**, here. The default config file, value
//! validation, `> config`, `> help` and the README settings table are all
//! generated from this list. Adding a setting = adding a `Config` field and
//! one entry below (a test checks both sides stay in sync).

use super::config::Config;
use super::hotkey::Hotkey;
use crate::i18n::Lang;

/// A text in both interface languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Localized {
    pub fr: &'static str,
    pub en: &'static str,
}

impl Localized {
    pub const fn new(fr: &'static str, en: &'static str) -> Self {
        Self { fr, en }
    }

    pub fn get(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::Fr => self.fr,
            Lang::En => self.en,
        }
    }
}

/// Type of the items of a [`SettingKind::List`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListItem {
    Text,
    Path,
    /// Input language code (`fr`, `en`…).
    Language,
}

/// Value type of a setting, with its constraints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingKind {
    Bool,
    Integer {
        min: i64,
        max: i64,
    },
    Float {
        min: f64,
        max: f64,
    },
    Text,
    Choice(&'static [&'static str]),
    List {
        item: ListItem,
        non_empty: bool,
    },
    Hotkey,
    Color,
    Path,
    /// `[[web]]` blocks: keyword, name and URL template.
    WebEngines,
}

/// Description of one setting.
#[derive(Debug, Clone, Copy)]
pub struct SettingDescriptor {
    /// Dotted TOML key (`theme.opacity`).
    pub key: &'static str,
    pub kind: SettingKind,
    pub name: Localized,
    pub description: Localized,
    pub aliases_fr: &'static [&'static str],
    pub aliases_en: &'static [&'static str],
    pub requires_restart: bool,
}

/// Why a value was rejected. Rendered to text by the UI through i18n.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ValueError {
    #[error("expected a boolean")]
    NotABool,
    #[error("expected an integer")]
    NotAnInteger,
    #[error("expected a number")]
    NotANumber,
    #[error("must be between {min} and {max}")]
    OutOfRange { min: String, max: String },
    #[error("expected a text")]
    NotAText,
    #[error("must be one of: {}", allowed.join(", "))]
    NotAChoice { allowed: Vec<&'static str> },
    #[error("expected a list")]
    NotAList,
    #[error("the list must not be empty")]
    EmptyList,
    #[error("invalid language code: {0}")]
    InvalidLanguage(String),
    #[error("invalid keyboard shortcut: {0}")]
    InvalidHotkey(String),
    #[error("invalid color: {0}")]
    InvalidColor(String),
    #[error("invalid web search engine: {0}")]
    InvalidWebEngine(String),
}

impl SettingDescriptor {
    /// Section of the key (`theme` for `theme.opacity`), `None` at top level.
    pub fn section(&self) -> Option<&'static str> {
        self.key.split_once('.').map(|(section, _)| section)
    }

    /// Key inside its section (`opacity` for `theme.opacity`).
    pub fn leaf(&self) -> &'static str {
        self.key.rsplit_once('.').map_or(self.key, |(_, leaf)| leaf)
    }

    /// Default value, read from [`Config::default`] (single source of truth).
    pub fn default_value(&self) -> Option<toml::Value> {
        let defaults = toml::Value::try_from(Config::default()).ok()?;
        lookup(&defaults, self.key).cloned()
    }

    /// Allowed values, for `Choice` settings.
    pub fn choices(&self) -> &'static [&'static str] {
        match self.kind {
            SettingKind::Choice(choices) => choices,
            _ => &[],
        }
    }

    /// Checks a value with the same rules as the TOML file and `> config`.
    pub fn validate(&self, value: &toml::Value) -> Result<(), ValueError> {
        use toml::Value as V;
        match (self.kind, value) {
            (SettingKind::Bool, V::Boolean(_)) => Ok(()),
            (SettingKind::Bool, _) => Err(ValueError::NotABool),
            (SettingKind::Integer { min, max }, V::Integer(n)) => {
                if (min..=max).contains(n) {
                    Ok(())
                } else {
                    Err(ValueError::OutOfRange {
                        min: min.to_string(),
                        max: max.to_string(),
                    })
                }
            }
            (SettingKind::Integer { .. }, _) => Err(ValueError::NotAnInteger),
            (SettingKind::Float { min, max }, V::Float(_) | V::Integer(_)) => {
                let n = match value {
                    V::Float(f) => *f,
                    #[allow(clippy::cast_precision_loss)]
                    V::Integer(i) => *i as f64,
                    _ => unreachable!(),
                };
                if n.is_finite() && (min..=max).contains(&n) {
                    Ok(())
                } else {
                    Err(ValueError::OutOfRange {
                        min: min.to_string(),
                        max: max.to_string(),
                    })
                }
            }
            (SettingKind::Float { .. }, _) => Err(ValueError::NotANumber),
            (SettingKind::Text | SettingKind::Path, V::String(_)) => Ok(()),
            (SettingKind::Text | SettingKind::Path, _) => Err(ValueError::NotAText),
            (SettingKind::Choice(allowed), V::String(s)) => {
                if allowed.contains(&s.as_str()) {
                    Ok(())
                } else {
                    Err(ValueError::NotAChoice {
                        allowed: allowed.to_vec(),
                    })
                }
            }
            (SettingKind::Choice(allowed), _) => Err(ValueError::NotAChoice {
                allowed: allowed.to_vec(),
            }),
            (SettingKind::List { item, non_empty }, V::Array(items)) => {
                if non_empty && items.is_empty() {
                    return Err(ValueError::EmptyList);
                }
                for v in items {
                    let V::String(s) = v else {
                        return Err(ValueError::NotAText);
                    };
                    if item == ListItem::Language && !is_language_code(s) {
                        return Err(ValueError::InvalidLanguage(s.clone()));
                    }
                }
                Ok(())
            }
            (SettingKind::List { .. }, _) => Err(ValueError::NotAList),
            (SettingKind::Hotkey, V::String(s)) => Hotkey::parse(s)
                .map(|_| ())
                .map_err(|_| ValueError::InvalidHotkey(s.clone())),
            (SettingKind::Hotkey, _) => Err(ValueError::InvalidHotkey(value.to_string())),
            (SettingKind::Color, V::String(s)) => crate::theme::Color::from_hex(s)
                .map(|_| ())
                .map_err(|_| ValueError::InvalidColor(s.clone())),
            (SettingKind::Color, _) => Err(ValueError::InvalidColor(value.to_string())),
            (SettingKind::WebEngines, V::Array(items)) => {
                for item in items {
                    validate_web_engine(item)?;
                }
                Ok(())
            }
            (SettingKind::WebEngines, _) => Err(ValueError::NotAList),
        }
    }
}

fn validate_web_engine(item: &toml::Value) -> Result<(), ValueError> {
    let table = item
        .as_table()
        .ok_or_else(|| ValueError::InvalidWebEngine(item.to_string()))?;
    let field = |name: &str| table.get(name).and_then(toml::Value::as_str);
    match (field("keyword"), field("name"), field("url")) {
        (Some(k), Some(_), Some(url))
            if !k.trim().is_empty()
                && !k.contains(char::is_whitespace)
                && url.contains("{query}") =>
        {
            Ok(())
        }
        _ => Err(ValueError::InvalidWebEngine(item.to_string())),
    }
}

/// Input language codes: 2 or 3 lowercase ASCII letters (ISO 639).
fn is_language_code(code: &str) -> bool {
    (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase())
}

/// Reads a dotted key in a TOML value.
pub fn lookup<'a>(root: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    key.split('.').try_fold(root, |v, part| v.get(part))
}

/// Finds a setting by exact key.
pub fn find(key: &str) -> Option<&'static SettingDescriptor> {
    REGISTRY.iter().find(|d| d.key == key)
}

macro_rules! setting {
    ($key:literal, $kind:expr, ($nfr:literal, $nen:literal), ($dfr:literal, $den:literal),
     fr: [$($afr:literal),*], en: [$($aen:literal),*] $(, restart: $restart:literal)?) => {
        SettingDescriptor {
            key: $key,
            kind: $kind,
            name: Localized::new($nfr, $nen),
            description: Localized::new($dfr, $den),
            aliases_fr: &[$($afr),*],
            aliases_en: &[$($aen),*],
            requires_restart: false $(|| $restart)?,
        }
    };
}

/// All user settings (everything in `config.toml` except `config_version`).
pub static REGISTRY: &[SettingDescriptor] = &[
    setting!("hotkey", SettingKind::Hotkey,
        ("Raccourci global", "Global shortcut"),
        ("Raccourci qui affiche ou cache Flair", "Shortcut that shows or hides Flair"),
        fr: ["raccourci", "touche"], en: ["shortcut", "key"]),
    setting!("ui_language", SettingKind::Choice(&["system", "fr", "en"]),
        ("Langue de l'interface", "Interface language"),
        ("Langue de l'interface et des résultats", "Language of the interface and results"),
        fr: ["langue interface"], en: ["interface language"]),
    setting!("input_languages", SettingKind::List { item: ListItem::Language, non_empty: true },
        ("Langues de saisie", "Input languages"),
        ("Langues de saisie actives, dans l'ordre de préférence", "Active input languages, in order of preference"),
        fr: ["langues", "langue"], en: ["languages", "language"]),
    setting!("decimal_separator", SettingKind::Choice(&["auto", ",", "."]),
        ("Séparateur décimal", "Decimal separator"),
        ("Virgule ou point pour les nombres décimaux", "Comma or dot for decimal numbers"),
        fr: ["virgule", "décimale"], en: ["decimal", "comma"]),
    setting!("date_format", SettingKind::Choice(&["auto", "dmy", "mdy", "ymd"]),
        ("Format des dates", "Date format"),
        ("Ordre jour, mois, année des dates numériques", "Day, month, year order of numeric dates"),
        fr: ["date"], en: ["date"]),
    setting!("max_results", SettingKind::Integer { min: 1, max: 20 },
        ("Nombre de résultats", "Number of results"),
        ("Nombre maximal de résultats affichés", "Maximum number of results shown"),
        fr: ["résultats"], en: ["results"]),
    setting!("keep_last_query", SettingKind::Bool,
        ("Garder la dernière recherche", "Keep last query"),
        ("Garder la dernière recherche quand Flair réapparaît", "Keep the last query when Flair shows up again"),
        fr: ["garder recherche"], en: ["keep query"]),
    setting!("autostart", SettingKind::Bool,
        ("Lancement au démarrage", "Launch at startup"),
        ("Lancer Flair à l'ouverture de la session", "Start Flair when the session opens"),
        fr: ["démarrage"], en: ["startup"]),
    setting!("log_level", SettingKind::Choice(&["error", "warn", "info", "debug", "trace"]),
        ("Niveau des logs", "Log level"),
        ("Détail des journaux (FLAIR_LOG est prioritaire)", "Log verbosity (FLAIR_LOG takes precedence)"),
        fr: ["logs", "journaux"], en: ["logs"], restart: true),
    // [calc]
    setting!("calc.angle_unit", SettingKind::Choice(&["deg", "rad"]),
        ("Unité d'angle", "Angle unit"),
        ("Unité par défaut des fonctions trigonométriques", "Default unit of trigonometric functions"),
        fr: ["angle", "degrés", "radians"], en: ["angle", "degrees", "radians"]),
    setting!("calc.significant_digits", SettingKind::Integer { min: 1, max: 100 },
        ("Chiffres significatifs", "Significant digits"),
        ("Chiffres significatifs affichés", "Significant digits shown"),
        fr: ["chiffres", "précision"], en: ["digits", "precision"]),
    setting!("calc.binary_prefixes", SettingKind::Bool,
        ("Préfixes binaires", "Binary prefixes"),
        ("Ko/Mo/Go en 1024 (comme l'Explorateur Windows) au lieu de 1000", "KB/MB/GB as 1024 (like Windows Explorer) instead of 1000"),
        fr: ["octets binaire"], en: ["binary bytes"]),
    setting!("calc.currency", SettingKind::Bool,
        ("Devises", "Currencies"),
        ("Conversions de devises (taux téléchargés et mis en cache)", "Currency conversions (rates downloaded and cached)"),
        fr: ["devises", "monnaie"], en: ["currency", "money"]),
    setting!("calc.history_size", SettingKind::Integer { min: 0, max: 1000 },
        ("Taille de l'historique", "History size"),
        ("Nombre de calculs gardés dans l'historique", "Number of calculations kept in history"),
        fr: ["historique"], en: ["history"]),
    // [theme]
    setting!("theme.mode", SettingKind::Choice(&["system", "light", "dark"]),
        ("Mode du thème", "Theme mode"),
        ("Suivre le système, ou forcer le thème clair ou sombre", "Follow the system, or force the light or dark theme"),
        fr: ["mode", "clair", "sombre"], en: ["mode", "light", "dark"]),
    setting!("theme.light", SettingKind::Text,
        ("Thème clair", "Light theme"),
        ("Thème utilisé en mode clair", "Theme used in light mode"),
        fr: ["thème clair"], en: ["light theme"]),
    setting!("theme.dark", SettingKind::Text,
        ("Thème sombre", "Dark theme"),
        ("Thème utilisé en mode sombre", "Theme used in dark mode"),
        fr: ["thème sombre"], en: ["dark theme"]),
    setting!("theme.opacity", SettingKind::Float { min: 0.85, max: 1.0 },
        ("Opacité", "Opacity"),
        ("Transparence du fond, si la plateforme le permet", "Background transparency, if the platform allows it"),
        fr: ["transparence", "opacité"], en: ["opacity", "transparency"]),
    setting!("theme.icons", SettingKind::Choice(&["color", "mono"]),
        ("Icônes", "Icons"),
        ("Icônes d'applications en couleur ou monochromes", "Application icons in color or monochrome"),
        fr: ["icônes"], en: ["icons"]),
    setting!("theme.font", SettingKind::Text,
        ("Police", "Font"),
        ("inter, system, ou le nom d'une police installée", "inter, system, or the name of an installed font"),
        fr: ["police"], en: ["font"]),
    setting!("theme.tray_icon", SettingKind::Choice(&["auto", "accent"]),
        ("Icône de notification", "Tray icon"),
        ("Couleur de l'icône : suit la barre des tâches, ou couleur d'accent", "Icon color: follows the taskbar, or accent color"),
        fr: ["icône notification"], en: ["tray"]),
    setting!("theme.animations", SettingKind::Bool,
        ("Animations", "Animations"),
        ("Fondu à l'apparition de la fenêtre", "Fade-in when the window appears"),
        fr: ["animations"], en: ["animations"]),
    // [files]
    setting!("files.enabled", SettingKind::Bool,
        ("Recherche de fichiers", "File search"),
        ("Indexer et rechercher les fichiers", "Index and search files"),
        fr: ["fichiers"], en: ["files"]),
    setting!("files.roots", SettingKind::List { item: ListItem::Path, non_empty: false },
        ("Dossiers indexés", "Indexed folders"),
        ("Dossiers racines de l'index de fichiers", "Root folders of the file index"),
        fr: ["dossiers", "racines"], en: ["folders", "roots"]),
    setting!("files.exclude", SettingKind::List { item: ListItem::Text, non_empty: false },
        ("Exclusions", "Exclusions"),
        ("Noms de dossiers ou motifs ignorés par l'index", "Folder names or patterns ignored by the index"),
        fr: ["exclure", "exclusions"], en: ["exclude"]),
    setting!("files.include_hidden", SettingKind::Bool,
        ("Fichiers cachés", "Hidden files"),
        ("Indexer aussi les fichiers et dossiers cachés", "Also index hidden files and folders"),
        fr: ["cachés"], en: ["hidden"]),
    setting!("files.other_drives", SettingKind::Bool,
        ("Autres disques", "Other drives"),
        ("Windows : indexer aussi les autres disques (D:\\, E:\\…)", "Windows: also index other drives (D:\\, E:\\…)"),
        fr: ["disques"], en: ["drives"]),
    // [resources]
    setting!("resources.renderer", SettingKind::Choice(&["auto", "gpu", "cpu"]),
        ("Moteur de rendu", "Renderer"),
        ("Rendu GPU, ou logiciel (cpu) sans mémoire vidéo", "GPU rendering, or software (cpu) without video memory"),
        fr: ["rendu"], en: ["renderer", "rendering"], restart: true),
    setting!("resources.pause_on_battery", SettingKind::Bool,
        ("Pause sur batterie", "Pause on battery"),
        ("Suspendre la surveillance et l'indexation sur batterie", "Suspend watching and indexing on battery"),
        fr: ["batterie"], en: ["battery"]),
    setting!("resources.trim_memory_after", SettingKind::Integer { min: 0, max: 3600 },
        ("Libérer la mémoire après", "Trim memory after"),
        ("Secondes après le masquage avant de libérer la mémoire (0 = jamais)", "Seconds after hiding before freeing memory (0 = never)"),
        fr: ["mémoire"], en: ["memory"]),
    setting!("resources.efficiency_mode", SettingKind::Bool,
        ("Mode efficacité", "Efficiency mode"),
        ("Windows : mode efficacité (EcoQoS) quand Flair est caché", "Windows: efficiency mode (EcoQoS) when Flair is hidden"),
        fr: ["efficacité", "ecoqos"], en: ["efficiency", "ecoqos"]),
    // [providers]
    setting!("providers.apps", SettingKind::Bool,
        ("Applications", "Applications"),
        ("Rechercher les applications installées", "Search installed applications"),
        fr: ["applications", "apps"], en: ["applications", "apps"]),
    setting!("providers.calculator", SettingKind::Bool,
        ("Calculatrice", "Calculator"),
        ("Calculs et conversions", "Calculations and conversions"),
        fr: ["calculatrice", "calcul"], en: ["calculator", "calc"]),
    setting!("providers.web", SettingKind::Bool,
        ("Recherche web", "Web search"),
        ("Mots-clés de recherche web", "Web search keywords"),
        fr: ["web"], en: ["web"]),
    setting!("providers.system", SettingKind::Bool,
        ("Commandes système", "System commands"),
        ("Verrouiller, éteindre, redémarrer…", "Lock, shut down, restart…"),
        fr: ["système"], en: ["system"]),
    // [update]
    setting!("update.channel", SettingKind::Choice(&["main", "stable"]),
        ("Canal de mise à jour", "Update channel"),
        ("main : chaque push sur main ; stable : versions taguées", "main: every push to main; stable: tagged versions"),
        fr: ["canal"], en: ["channel"]),
    setting!("update.auto_check", SettingKind::Bool,
        ("Vérifier les mises à jour", "Check for updates"),
        ("Vérifier au plus une fois par jour, à l'ouverture de Flair", "Check at most once a day, when Flair opens"),
        fr: ["mise à jour auto"], en: ["auto update"]),
    // [[web]]
    setting!("web", SettingKind::WebEngines,
        ("Moteurs de recherche web", "Web search engines"),
        ("Mots-clés et adresses des moteurs de recherche ({query} = la recherche)", "Keywords and URLs of search engines ({query} = the search)"),
        fr: ["moteurs", "recherche web"], en: ["engines", "web search"]),
];

/// Reads a dotted key in a TOML table.
pub(crate) fn lookup_table<'a>(table: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let (first, rest) = key.split_once('.').unwrap_or((key, ""));
    let value = table.get(first)?;
    if rest.is_empty() {
        Some(value)
    } else {
        lookup(value, rest)
    }
}

/// Removes a dotted key from a TOML table (no-op if absent).
pub(crate) fn remove_key(table: &mut toml::Table, key: &str) {
    match key.split_once('.') {
        None => {
            table.remove(key);
        }
        Some((section, rest)) => {
            if let Some(toml::Value::Table(sub)) = table.get_mut(section) {
                remove_key(sub, rest);
            }
        }
    }
}

/// Keys present in `table` that are neither settings, sections, nor `config_version`.
pub(crate) fn unknown_keys(table: &toml::Table) -> Vec<String> {
    let mut unknown = Vec::new();
    for (k, v) in table {
        if k == "config_version" || find(k).is_some() {
            continue;
        }
        let is_section = REGISTRY.iter().any(|d| d.section() == Some(k.as_str()));
        match v {
            toml::Value::Table(sub) if is_section => {
                for sub_key in sub.keys() {
                    let full = format!("{k}.{sub_key}");
                    if find(&full).is_none() {
                        unknown.push(full);
                    }
                }
            }
            _ => unknown.push(k.clone()),
        }
    }
    unknown.sort();
    unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Collects the leaf keys of the default config (tables are walked, arrays are leaves).
    fn config_keys() -> BTreeSet<String> {
        fn walk(prefix: &str, value: &toml::Value, out: &mut BTreeSet<String>) {
            match value {
                toml::Value::Table(table) => {
                    for (k, v) in table {
                        let key = if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}.{k}")
                        };
                        walk(&key, v, out);
                    }
                }
                _ => {
                    out.insert(prefix.to_owned());
                }
            }
        }
        let mut keys = BTreeSet::new();
        let defaults = toml::Value::try_from(Config::default()).expect("serializable");
        walk("", &defaults, &mut keys);
        keys.remove("config_version");
        keys
    }

    #[test]
    fn every_config_field_has_a_registry_entry_and_vice_versa() {
        let config = config_keys();
        let registry: BTreeSet<String> = REGISTRY.iter().map(|d| d.key.to_owned()).collect();
        let missing: Vec<_> = config.difference(&registry).collect();
        let extra: Vec<_> = registry.difference(&config).collect();
        assert!(
            missing.is_empty(),
            "config fields missing from the registry: {missing:?}"
        );
        assert!(
            extra.is_empty(),
            "registry entries without a config field: {extra:?}"
        );
        assert_eq!(registry.len(), REGISTRY.len(), "duplicate registry keys");
    }

    #[test]
    fn every_setting_is_described_in_both_languages() {
        for d in REGISTRY {
            for text in [d.name.fr, d.name.en, d.description.fr, d.description.en] {
                assert!(
                    !text.trim().is_empty(),
                    "{}: empty name or description",
                    d.key
                );
            }
            assert!(
                !d.aliases_fr.is_empty() && !d.aliases_en.is_empty(),
                "{}: aliases",
                d.key
            );
        }
    }

    #[test]
    fn every_default_value_is_valid() {
        for d in REGISTRY {
            let value = d
                .default_value()
                .unwrap_or_else(|| panic!("{}: no default", d.key));
            assert_eq!(d.validate(&value), Ok(()), "{}: invalid default", d.key);
        }
    }

    #[test]
    fn every_choice_deserializes_into_the_config() {
        for d in REGISTRY.iter().filter(|d| !d.choices().is_empty()) {
            for choice in d.choices() {
                let mut table = toml::Value::try_from(Config::default()).expect("serializable");
                set(&mut table, d.key, toml::Value::String((*choice).into()));
                let parsed: Result<Config, _> = table.try_into();
                assert!(
                    parsed.is_ok(),
                    "{} = {choice:?} does not deserialize",
                    d.key
                );
            }
        }
    }

    fn set(root: &mut toml::Value, key: &str, value: toml::Value) {
        let mut parts: Vec<&str> = key.split('.').collect();
        let last = parts.pop().expect("non-empty key");
        let mut cur = root;
        for p in parts {
            cur = cur.get_mut(p).expect("section exists");
        }
        cur.as_table_mut()
            .expect("table")
            .insert(last.into(), value);
    }

    #[test]
    fn validation_rules() {
        let max_results = find("max_results").expect("exists");
        assert!(max_results.validate(&toml::Value::Integer(10)).is_ok());
        assert!(matches!(
            max_results.validate(&toml::Value::Integer(99)),
            Err(ValueError::OutOfRange { .. })
        ));
        let opacity = find("theme.opacity").expect("exists");
        assert!(opacity.validate(&toml::Value::Float(0.9)).is_ok());
        assert!(opacity.validate(&toml::Value::Integer(1)).is_ok());
        assert!(opacity.validate(&toml::Value::Float(0.5)).is_err());
        let langs = find("input_languages").expect("exists");
        assert_eq!(
            langs.validate(&toml::Value::Array(vec![])),
            Err(ValueError::EmptyList)
        );
        assert!(
            langs
                .validate(&toml::Value::Array(vec!["FR!".into()]))
                .is_err()
        );
        let hotkey = find("hotkey").expect("exists");
        assert!(hotkey.validate(&"Ctrl+Space".into()).is_ok());
        assert!(hotkey.validate(&"Space".into()).is_err());
        let mode = find("theme.mode").expect("exists");
        assert!(mode.validate(&"dark".into()).is_ok());
        assert!(mode.validate(&"purple".into()).is_err());
    }
}

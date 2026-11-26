//! Typed configuration, as stored in `config.toml`.
//!
//! Every field (except `config_version`) has exactly one entry in the
//! [registry](super::registry). Default values live here, in the `Default`
//! impls, and nowhere else: the registry reads them back from
//! [`Config::default`].

use serde::{Deserialize, Serialize};

/// Current version of the configuration format (see migrations, spec section 11).
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub config_version: u32,
    pub hotkey: String,
    pub ui_language: UiLanguageSetting,
    pub input_languages: Vec<String>,
    pub decimal_separator: DecimalSeparator,
    pub date_format: DateFormat,
    pub max_results: u32,
    pub keep_last_query: bool,
    pub autostart: bool,
    pub log_level: LogLevel,
    pub calc: CalcConfig,
    pub theme: ThemeConfig,
    pub files: FilesConfig,
    pub resources: ResourcesConfig,
    pub providers: ProvidersConfig,
    pub update: UpdateConfig,
    pub web: Vec<WebEngine>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            hotkey: "Alt+Space".into(),
            ui_language: UiLanguageSetting::System,
            input_languages: vec!["fr".into(), "en".into()],
            decimal_separator: DecimalSeparator::Auto,
            date_format: DateFormat::Auto,
            max_results: 8,
            keep_last_query: false,
            autostart: false,
            log_level: LogLevel::Info,
            calc: CalcConfig::default(),
            theme: ThemeConfig::default(),
            files: FilesConfig::default(),
            resources: ResourcesConfig::default(),
            providers: ProvidersConfig::default(),
            update: UpdateConfig::default(),
            web: WebEngine::defaults(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguageSetting {
    System,
    Fr,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecimalSeparator {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = ",")]
    Comma,
    #[serde(rename = ".")]
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    Auto,
    Dmy,
    Mdy,
    Ymd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AngleUnit {
    Deg,
    Rad,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalcConfig {
    pub angle_unit: AngleUnit,
    pub significant_digits: u32,
    pub binary_prefixes: bool,
    pub currency: bool,
    pub history_size: u32,
}

impl Default for CalcConfig {
    fn default() -> Self {
        Self {
            angle_unit: AngleUnit::Deg,
            significant_digits: 10,
            binary_prefixes: false,
            currency: false,
            history_size: 50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    Color,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayIconStyle {
    Auto,
    Accent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub mode: ThemeMode,
    pub light: String,
    pub dark: String,
    pub opacity: f64,
    pub icons: IconStyle,
    pub font: String,
    pub tray_icon: TrayIconStyle,
    pub animations: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            light: crate::theme::builtin::LIGHT_NAME.into(),
            dark: crate::theme::builtin::DARK_NAME.into(),
            opacity: 1.0,
            icons: IconStyle::Color,
            font: "inter".into(),
            tray_icon: TrayIconStyle::Auto,
            animations: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesConfig {
    pub enabled: bool,
    pub roots: Vec<String>,
    pub exclude: Vec<String>,
    pub include_hidden: bool,
    pub other_drives: bool,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            roots: vec!["~".into()],
            exclude: ["node_modules", ".git", "target", ".cache", "__pycache__"]
                .map(String::from)
                .to_vec(),
            include_hidden: false,
            other_drives: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RendererSetting {
    Auto,
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourcesConfig {
    pub renderer: RendererSetting,
    pub pause_on_battery: bool,
    pub trim_memory_after: u32,
    pub efficiency_mode: bool,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            renderer: RendererSetting::Auto,
            pause_on_battery: true,
            trim_memory_after: 5,
            efficiency_mode: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub apps: bool,
    pub calculator: bool,
    pub web: bool,
    pub system: bool,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            apps: true,
            calculator: true,
            web: true,
            system: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Main,
    Stable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    pub channel: UpdateChannel,
    pub auto_check: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Main,
            auto_check: true,
        }
    }
}

/// A web search engine bound to a keyword (`g rust iced`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebEngine {
    pub keyword: String,
    pub name: String,
    /// URL template; `{query}` is replaced by the URL-encoded query.
    pub url: String,
}

impl WebEngine {
    pub fn defaults() -> Vec<Self> {
        [
            ("g", "Google", "https://www.google.com/search?q={query}"),
            ("ddg", "DuckDuckGo", "https://duckduckgo.com/?q={query}"),
            (
                "wiki",
                "Wikipedia",
                "https://fr.wikipedia.org/w/index.php?search={query}",
            ),
            (
                "yt",
                "YouTube",
                "https://www.youtube.com/results?search_query={query}",
            ),
        ]
        .map(|(keyword, name, url)| Self {
            keyword: keyword.into(),
            name: name.into(),
            url: url.into(),
        })
        .to_vec()
    }
}

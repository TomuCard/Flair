//! Interface translations, with Fluent.
//!
//! The interface language is independent from the *input* languages (which
//! words are understood in queries, handled by lexicons). Any system language
//! other than French falls back to English.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::{LanguageIdentifier, langid};

use crate::settings::ValueError;
use crate::settings::config::UiLanguageSetting;

/// Languages the interface is translated into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Self; 2] = [Self::Fr, Self::En];

    pub fn code(self) -> &'static str {
        match self {
            Self::Fr => "fr",
            Self::En => "en",
        }
    }

    fn langid(self) -> LanguageIdentifier {
        match self {
            Self::Fr => langid!("fr"),
            Self::En => langid!("en"),
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::Fr => include_str!("../i18n/fr.ftl"),
            Self::En => include_str!("../i18n/en.ftl"),
        }
    }

    /// Picks the interface language from a BCP 47 locale (`fr-FR`, `en_US.UTF-8`).
    pub fn from_locale(locale: Option<&str>) -> Self {
        match locale.map(str::to_ascii_lowercase) {
            Some(l) if l == "fr" || l.starts_with("fr-") || l.starts_with("fr_") => Self::Fr,
            _ => Self::En,
        }
    }

    /// Resolves the `ui_language` setting (`system` reads the OS locale).
    pub fn resolve(setting: UiLanguageSetting) -> Self {
        match setting {
            UiLanguageSetting::Fr => Self::Fr,
            UiLanguageSetting::En => Self::En,
            UiLanguageSetting::System => Self::from_locale(sys_locale::get_locale().as_deref()),
        }
    }
}

/// Translations for one interface language. Cheap to share (`Send + Sync`).
pub struct I18n {
    lang: Lang,
    bundle: FluentBundle<FluentResource>,
}

impl std::fmt::Debug for I18n {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n")
            .field("lang", &self.lang)
            .finish_non_exhaustive()
    }
}

impl I18n {
    pub fn new(lang: Lang) -> Self {
        let mut bundle = FluentBundle::new_concurrent(vec![lang.langid()]);
        // No Unicode isolation marks around arguments: the UI is LTR only.
        bundle.set_use_isolating(false);
        match FluentResource::try_new(lang.source().to_owned()) {
            Ok(resource) => {
                if let Err(errors) = bundle.add_resource(resource) {
                    tracing::error!(?errors, lang = lang.code(), "duplicate translation ids");
                }
            }
            Err((resource, errors)) => {
                tracing::error!(?errors, lang = lang.code(), "translation file has errors");
                let _ = bundle.add_resource(resource);
            }
        }
        Self { lang, bundle }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// Translates a message without arguments. A missing id returns the id itself.
    pub fn t(&self, id: &str) -> String {
        self.format(id, None)
    }

    /// Translates a message with named string arguments.
    pub fn t_args(&self, id: &str, args: &[(&str, &str)]) -> String {
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(*name, FluentValue::from(*value));
        }
        self.format(id, Some(&fluent_args))
    }

    /// Localized explanation of a rejected setting value.
    pub fn value_error(&self, error: &ValueError) -> String {
        match error {
            ValueError::NotABool => self.t("value-not-bool"),
            ValueError::NotAnInteger => self.t("value-not-integer"),
            ValueError::NotANumber => self.t("value-not-number"),
            ValueError::OutOfRange { min, max } => {
                self.t_args("value-out-of-range", &[("min", min), ("max", max)])
            }
            ValueError::NotAText => self.t("value-not-text"),
            ValueError::NotAChoice { allowed } => {
                self.t_args("value-not-choice", &[("allowed", &allowed.join(" | "))])
            }
            ValueError::NotAList => self.t("value-not-list"),
            ValueError::EmptyList => self.t("value-empty-list"),
            ValueError::InvalidLanguage(v) => {
                self.t_args("value-invalid-language", &[("value", v)])
            }
            ValueError::InvalidHotkey(v) => self.t_args("value-invalid-hotkey", &[("value", v)]),
            ValueError::InvalidColor(v) => self.t_args("value-invalid-color", &[("value", v)]),
            ValueError::InvalidWebEngine(v) => {
                self.t_args("value-invalid-web-engine", &[("value", v)])
            }
        }
    }

    fn format(&self, id: &str, args: Option<&FluentArgs<'_>>) -> String {
        let Some(pattern) = self.bundle.get_message(id).and_then(|m| m.value()) else {
            tracing::warn!(id, lang = self.lang.code(), "missing translation");
            return id.to_owned();
        };
        let mut errors = Vec::new();
        let text = self.bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            tracing::warn!(id, ?errors, "translation formatting error");
        }
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ids(lang: Lang) -> BTreeSet<String> {
        let resource = FluentResource::try_new(lang.source().to_owned()).expect("valid ftl");
        resource
            .entries()
            .filter_map(|e| match e {
                fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn both_languages_have_the_same_messages() {
        let (fr, en) = (ids(Lang::Fr), ids(Lang::En));
        assert!(!fr.is_empty());
        assert_eq!(fr, en, "fr.ftl and en.ftl differ");
    }

    #[test]
    fn translates_with_arguments() {
        let fr = I18n::new(Lang::Fr);
        assert_eq!(fr.t("search-placeholder"), "Rechercher…");
        assert_eq!(
            fr.t_args("web-search", &[("query", "vsc")]),
            "Rechercher « vsc » sur le web"
        );
        let en = I18n::new(Lang::En);
        assert_eq!(
            en.t_args("read-as", &[("reading", "5 minutes")]),
            "read as: 5 minutes"
        );
        assert_eq!(en.t("does-not-exist"), "does-not-exist");
    }

    #[test]
    fn value_errors_are_translated() {
        let err = ValueError::OutOfRange {
            min: "0.85".into(),
            max: "1".into(),
        };
        assert_eq!(
            I18n::new(Lang::Fr).value_error(&err),
            "doit être entre 0.85 et 1"
        );
        assert_eq!(
            I18n::new(Lang::En).value_error(&err),
            "must be between 0.85 and 1"
        );
        let web = I18n::new(Lang::En).value_error(&ValueError::InvalidWebEngine("x".into()));
        assert!(web.contains("{query}"), "{web}");
    }

    #[test]
    fn locale_resolution() {
        assert_eq!(Lang::from_locale(Some("fr-FR")), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("fr_CA.UTF-8")), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("FR")), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("en-US")), Lang::En);
        assert_eq!(Lang::from_locale(Some("de-DE")), Lang::En);
        assert_eq!(Lang::from_locale(Some("frisian")), Lang::En);
        assert_eq!(Lang::from_locale(None), Lang::En);
    }
}

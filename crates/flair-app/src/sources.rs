//! The result sources of the application and their shared state (launch
//! history), built once at startup and kept across configuration reloads.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use flair_core::Provider;
use flair_core::frecency::Frecency;
use flair_core::i18n::{I18n, Lang};
use flair_core::lexicon;
use flair_core::settings::Config;
use flair_platform::paths::Paths;
use flair_providers::Providers;
use flair_providers::apps::{AppsProvider, Texts};
use flair_providers::calc::{self, CalcProvider, Calculator};

#[derive(Clone)]
pub struct Sources {
    pub apps: AppsProvider,
    pub calc: Arc<CalcProvider>,
    pub frecency: Arc<Mutex<Frecency>>,
    lexicon_dir: PathBuf,
}

fn texts(i18n: &I18n) -> Texts {
    Texts {
        lang: match i18n.lang() {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
        .into(),
        subtitle: i18n.t("kind-application"),
    }
}

/// The calculator for a configuration, with the user's language files
/// (invalid ones are ignored and logged).
fn calculator(config: &Config, i18n: &I18n, lexicon_dir: &std::path::Path) -> Calculator {
    let (user, invalid) = lexicon::load_dir(lexicon_dir);
    for (path, reason) in invalid {
        tracing::warn!(file = %path.display(), "language file ignored: {reason}");
    }
    let settings = calc::Settings {
        input_languages: config.input_languages.clone(),
        decimal_separator: config.decimal_separator,
        system_language: Lang::system_language(),
        french_interface: i18n.lang() == Lang::Fr,
        angle_unit: config.calc.angle_unit,
        significant_digits: config.calc.significant_digits as usize,
        binary_prefixes: config.calc.binary_prefixes,
    };
    Calculator::new(&settings, &user)
}

impl Sources {
    /// Loads the launch history and starts indexing the applications in the
    /// background. `watch`: follow changes of the app folders; without it
    /// (one-off runs like development screenshots), indexing is synchronous.
    pub fn start(paths: &Paths, config: &Config, i18n: &I18n, watch: bool) -> Self {
        let frecency = Arc::new(Mutex::new(Frecency::load(
            &paths.data_dir.join("frecency.json"),
        )));
        let icon_cache = cfg!(windows).then(|| paths.cache_dir.join("icons"));
        let apps = AppsProvider::new(texts(i18n), Arc::clone(&frecency), icon_cache);
        if watch {
            apps.refresh();
            apps.watch();
        } else {
            // One-off run (development screenshot): results must be complete
            // before the first query.
            apps.scan_blocking();
        }
        let lexicon_dir = paths.lexicon_dir();
        let calc = Arc::new(CalcProvider::new(
            calculator(config, i18n, &lexicon_dir),
            calc::Texts::new(i18n),
        ));
        Self {
            apps,
            calc,
            frecency,
            lexicon_dir,
        }
    }

    /// The merged provider for the current configuration (enabled sources,
    /// language, list size).
    pub fn providers(&self, i18n: &I18n, config: &Config) -> Arc<dyn Provider> {
        let mut sources: Vec<Arc<dyn Provider>> = Vec::new();
        if config.providers.calculator {
            sources.push(Arc::clone(&self.calc) as Arc<dyn Provider>);
        }
        if config.providers.apps {
            sources.push(Arc::new(self.apps.clone()));
        }
        Arc::new(Providers::new(
            sources,
            config.max_results as usize,
            config
                .providers
                .web
                .then(|| i18n.t_args("web-search", &[("query", "%s")])),
        ))
    }

    /// Configuration reloaded: new language and settings, language files and
    /// launch history maybe imported.
    pub fn reload(&self, config: &Config, i18n: &I18n) {
        self.apps.set_texts(texts(i18n));
        self.calc.replace(
            calculator(config, i18n, &self.lexicon_dir),
            calc::Texts::new(i18n),
        );
        if let Ok(mut f) = self.frecency.lock() {
            f.reload();
        }
    }

    /// Records the launch of a result (frecency).
    pub fn record_launch(&self, id: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Ok(mut f) = self.frecency.lock() {
            f.record(id, now);
        }
    }
}

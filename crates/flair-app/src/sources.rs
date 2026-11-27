//! The result sources of the application and their shared state (launch
//! history), built once at startup and kept across configuration reloads.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use flair_core::Provider;
use flair_core::frecency::Frecency;
use flair_core::history::History;
use flair_core::i18n::{I18n, Lang};
use flair_core::lexicon;
use flair_core::settings::Config;
use flair_platform::paths::Paths;
use flair_providers::Providers;
use flair_providers::apps::{AppsProvider, Texts};
use flair_providers::calc::currency::{Rates, SharedRates};
use flair_providers::calc::{self, CalcProvider, Calculator};
use flair_providers::files::{self, FilesProvider};

#[derive(Clone)]
pub struct Sources {
    pub apps: AppsProvider,
    pub calc: Arc<CalcProvider>,
    pub files: FilesProvider,
    pub frecency: Arc<Mutex<Frecency>>,
    pub history: Arc<Mutex<History>>,
    rates: SharedRates,
    rates_file: PathBuf,
    lexicon_dir: PathBuf,
}

/// Exchange rates older than this are fetched again (the ECB publishes
/// once a working day).
const RATES_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// Loads the cached rates, then fetches new ones in the background when the
/// cache is missing or old. Typing never waits for the network.
fn refresh_rates(rates: &SharedRates, file: &std::path::Path) {
    if let Some(cached) = Rates::load(file)
        && let Ok(mut r) = rates.write()
    {
        *r = Some(cached);
    }
    let fresh = std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < RATES_MAX_AGE);
    if fresh {
        return;
    }
    let rates = Arc::clone(rates);
    let file = file.to_owned();
    let spawned = std::thread::Builder::new()
        .name("rates".into())
        .spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .https_only(true)
                .timeout_global(Some(std::time::Duration::from_secs(15)))
                // Generic name: no user or machine information is sent.
                .user_agent("flair")
                .build()
                .into();
            let text = agent
                .get(calc::currency::URL)
                .call()
                .and_then(|mut r| r.body_mut().read_to_string());
            match text.ok().as_deref().and_then(Rates::parse) {
                Some(new) => {
                    if let Some(dir) = file.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if let Err(e) = std::fs::write(&file, new.to_json()) {
                        tracing::warn!(error = %e, "cannot cache the exchange rates");
                    }
                    tracing::info!(date = %new.date, "exchange rates updated");
                    if let Ok(mut r) = rates.write() {
                        *r = Some(new);
                    }
                }
                None => {
                    tracing::warn!("cannot fetch the exchange rates (offline?): using the cache")
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "cannot start the exchange rates fetch");
    }
}

fn home() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_owned())
}

/// The files settings of a configuration.
fn files_settings(config: &Config) -> files::Settings {
    let f = &config.files;
    files::Settings::new(
        f.enabled,
        &f.roots,
        &f.exclude,
        f.include_hidden,
        f.other_drives,
        config.resources.pause_on_battery,
        home().as_deref(),
    )
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
fn calculator(
    config: &Config,
    i18n: &I18n,
    lexicon_dir: &std::path::Path,
    rates: &SharedRates,
) -> Calculator {
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
        date_format: config.date_format,
        labels: calc::Labels::new(i18n),
    };
    let calculator = Calculator::new(&settings, &user);
    if config.calc.currency {
        calculator.with_rates(Arc::clone(rates))
    } else {
        calculator
    }
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
        let files = FilesProvider::new(
            paths.data_dir.join("index"),
            home(),
            Arc::clone(&frecency),
            files_settings(config),
        );
        files.start(watch);
        crate::stats::set_files(files.clone());
        let lexicon_dir = paths.lexicon_dir();
        let history = Arc::new(Mutex::new(History::load(
            &paths.data_dir.join("history.json"),
            config.calc.history_size as usize,
        )));
        let rates: SharedRates = Arc::new(std::sync::RwLock::new(None));
        let rates_file = paths.cache_dir.join("rates.json");
        if config.calc.currency {
            refresh_rates(&rates, &rates_file);
        }
        let calc = Arc::new(
            CalcProvider::new(
                calculator(config, i18n, &lexicon_dir, &rates),
                calc::Texts::new(i18n),
            )
            .with_history(Arc::clone(&history)),
        );
        Self {
            apps,
            calc,
            files,
            frecency,
            history,
            rates,
            rates_file,
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
        if config.files.enabled {
            sources.push(Arc::new(self.files.clone()));
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
        self.files.reconfigure(files_settings(config));
        if config.calc.currency {
            refresh_rates(&self.rates, &self.rates_file);
        }
        self.calc.replace(
            calculator(config, i18n, &self.lexicon_dir, &self.rates),
            calc::Texts::new(i18n),
        );
        if let Ok(mut f) = self.frecency.lock() {
            f.reload();
        }
        if let Ok(mut h) = self.history.lock() {
            h.set_max(config.calc.history_size as usize);
            h.reload();
        }
    }

    /// Records a copied calculation (`=` alone lists them).
    pub fn record_calculation(&self, input: &str) {
        if let Ok(mut h) = self.history.lock() {
            h.record(input);
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

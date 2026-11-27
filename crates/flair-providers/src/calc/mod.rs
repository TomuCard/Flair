//! Calculator: a 4-step pipeline, testable without the interface.
//!
//! 1. **Detection** ([`looks_like_calculation`]): cheap test before anything.
//! 2. **Normalization** ([`token`], [`normalize`]): tokens, lexicon senses
//!    with their conditions, every reading rendered in `fend-core` syntax.
//! 3. **Evaluation** ([`eval`]): fend with a deadline, plus the functions fend
//!    lacks. Readings that fail are dropped: evaluation settles ambiguities.
//! 4. **Formatting** ([`format`]): main result and variants in the interface
//!    language.

pub mod eval;
pub mod format;
pub mod normalize;
pub mod token;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use flair_core::lexicon::{self, Lexicon};
use flair_core::result::BuiltinIcon;
use flair_core::settings::config::{AngleUnit, DecimalSeparator};
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem,
};

use eval::{Deadline, EvalError, Outcome};
use format::{Shown, Style};
use normalize::{RenderOptions, Rendered, Vocabulary};
use token::DecimalMode;

/// Time allowed for all the readings of one input.
pub const BUDGET: Duration = Duration::from_millis(50);
/// Readings shown as variants besides the best one.
const MAX_OTHER_READINGS: usize = 2;

/// What the calculator needs from the configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Active input languages, in order of preference.
    pub input_languages: Vec<String>,
    pub decimal_separator: DecimalSeparator,
    /// Language of the system (`fr`, `en`…), for `auto` settings.
    pub system_language: String,
    /// Language of the interface: results are formatted in it.
    pub french_interface: bool,
    pub angle_unit: AngleUnit,
    pub significant_digits: usize,
    pub binary_prefixes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input_languages: vec!["fr".into(), "en".into()],
            decimal_separator: DecimalSeparator::Auto,
            system_language: "fr".into(),
            french_interface: true,
            angle_unit: AngleUnit::Deg,
            significant_digits: 10,
            binary_prefixes: false,
        }
    }
}

/// One value to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub display: String,
    /// `Alt+Enter`: no unit, `.` as decimal separator.
    pub raw: String,
    /// Another reading of the input (`5 min` for `5m`), shown as "read as".
    pub read_as: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Calculation {
    Values {
        main: Answer,
        variants: Vec<Answer>,
    },
    Prime {
        n: u64,
        prime: bool,
        factor: Option<u64>,
    },
    TooBig,
}

/// Quick test: worth normalizing? (`firefox` is not, `pi*2` is.) Inputs
/// without digits nor symbols (`une heure en minutes`) are checked later,
/// on their words.
pub fn looks_like_calculation(input: &str) -> bool {
    let input = input.trim();
    !input.is_empty()
        && input.chars().count() <= token::MAX_INPUT_CHARS
        && input
            .chars()
            .any(|c| c.is_ascii_digit() || "+-*/^()%!×÷√∛π".contains(c))
}

/// Words that make an input without digits a calculation.
fn calculation_word(segment: &normalize::Segment) -> bool {
    segment.candidates.iter().any(|c| match &c.sense {
        normalize::Sense::Lexicon(s) => {
            s == "convert_to" || s == "one" || s.starts_with("op:") || s.starts_with("func:")
        }
        _ => false,
    })
}

/// Bound of the cache of words known (or not) as units.
const MAX_CACHED_WORDS: usize = 10_000;

pub struct Calculator {
    vocabulary: Vocabulary,
    mode: DecimalMode,
    options: RenderOptions,
    style: Style,
    /// Words fend knows as units (asked once per word).
    units: Mutex<HashMap<String, bool>>,
}

/// Lexicons by code: built-in ones, replaced or completed by the user's.
pub fn lexicons(user: &[Lexicon]) -> HashMap<String, Lexicon> {
    let mut all: HashMap<String, Lexicon> = lexicon::BUILTIN
        .iter()
        .filter_map(|(code, _)| lexicon::builtin(code).map(|l| ((*code).to_owned(), l)))
        .collect();
    for l in user {
        all.insert(l.meta.code.clone(), l.clone());
    }
    all
}

impl Calculator {
    pub fn new(settings: &Settings, user_lexicons: &[Lexicon]) -> Self {
        let all = lexicons(user_lexicons);
        let common = all.get(lexicon::COMMON);
        let mut active: Vec<&Lexicon> = common.into_iter().collect();
        active.extend(
            settings
                .input_languages
                .iter()
                .filter_map(|code| all.get(code)),
        );
        let inactive: Vec<&Lexicon> = all
            .iter()
            .filter(|(code, _)| {
                *code != lexicon::COMMON && !settings.input_languages.contains(code)
            })
            .map(|(_, l)| l)
            .collect();
        let ui = if settings.french_interface {
            "fr"
        } else {
            "en"
        };
        let display: Vec<&Lexicon> = all.get(ui).into_iter().chain(common).collect();
        let vocabulary = Vocabulary::new(&active, &inactive, &display);

        // `auto`: the system language if it is active, else the first one.
        let convention = if settings.input_languages.contains(&settings.system_language) {
            all.get(&settings.system_language)
        } else {
            settings.input_languages.first().and_then(|c| all.get(c))
        };
        let comma = convention.and_then(|l| l.meta.decimal_separator) == Some(',');
        let mode = match settings.decimal_separator {
            DecimalSeparator::Comma => DecimalMode::Comma,
            DecimalSeparator::Dot => DecimalMode::Dot,
            DecimalSeparator::Auto => DecimalMode::Auto {
                preferred_comma: comma,
            },
        };
        Self {
            vocabulary,
            mode,
            options: RenderOptions {
                degrees: settings.angle_unit == AngleUnit::Deg,
                binary_prefixes: settings.binary_prefixes,
            },
            style: Style {
                french: settings.french_interface,
                significant: settings.significant_digits.clamp(1, 100),
            },
            units: Mutex::new(HashMap::new()),
        }
    }

    /// True when fend reads `1 <word>` as a quantity with a unit.
    fn is_unit(&self, word: &str) -> bool {
        if word.chars().count() > 30
            || !word
                .chars()
                .all(|c| c.is_alphabetic() || c == '_' || c == '°')
        {
            return false;
        }
        if let Ok(cache) = self.units.lock()
            && let Some(known) = cache.get(word)
        {
            return *known;
        }
        let known = eval::fend(
            &format!("1 {word}"),
            Deadline::after(Duration::from_millis(20)),
        )
        .is_ok_and(|(text, _)| text.chars().any(char::is_alphabetic) && !text.ends_with('i'));
        if let Ok(mut cache) = self.units.lock() {
            if cache.len() >= MAX_CACHED_WORDS {
                cache.clear();
            }
            cache.insert(word.to_owned(), known);
        }
        known
    }

    /// Normalizes and evaluates an input; `None` if it is not a calculation.
    pub fn calculate(&self, input: &str, cancel: &CancellationToken) -> Option<Calculation> {
        let input = input.trim();
        if input.is_empty() || input.chars().count() > token::MAX_INPUT_CHARS {
            return None;
        }
        let deadline = Deadline::after(BUDGET);
        let tokens = token::tokenize(input, self.mode);
        let symbols = looks_like_calculation(input);
        // Without digits nor symbols, a calculation needs at least two words
        // (`une heure en minutes`), never one (`firefox`).
        if !symbols && tokens.len() < 2 {
            return None;
        }
        let segments = normalize::segments(&tokens, &self.vocabulary, &|w| self.is_unit(w));
        if !symbols && !segments.iter().any(calculation_word) {
            return None;
        }
        let mut valid: Vec<(Rendered, Outcome)> = Vec::new();
        let mut too_big = false;
        for reading in normalize::readings(&segments) {
            if cancel.is_cancelled() {
                return None;
            }
            let Some(rendered) = normalize::render(&segments, &reading, self.options) else {
                continue;
            };
            if valid.iter().any(|(r, _)| r.fend == rendered.fend) {
                continue;
            }
            match eval::evaluate(&rendered.fend, deadline) {
                Ok(outcome) => valid.push((rendered, outcome)),
                Err(EvalError::TooBig) => too_big = true,
                Err(EvalError::Invalid) => {}
            }
        }
        let Some((best, outcome)) = valid.first() else {
            return too_big.then_some(Calculation::TooBig);
        };
        if best.trivial {
            return None;
        }
        let (text, approx) = match outcome {
            Outcome::Prime { n, prime, factor } => {
                return Some(Calculation::Prime {
                    n: *n,
                    prime: *prime,
                    factor: *factor,
                });
            }
            Outcome::Value { text, approx } => (text, *approx),
        };
        let mut shown = self.shown(text, approx);
        if let Some(prefix) = best.base_prefix
            && !shown.display.starts_with(prefix)
        {
            shown.display = format!("{prefix}{}", shown.display);
            shown.raw = shown.display.clone();
        }
        let mut main = Answer {
            display: shown.display.clone(),
            raw: shown.raw.clone(),
            read_as: None,
        };
        let mut variants: Vec<Answer> = Vec::new();
        let plain = |s: &Shown| Answer {
            display: s.display.clone(),
            raw: s.raw.clone(),
            read_as: None,
        };

        let (preferred, extra) = self.value_variants(best, text, deadline);
        if let Some(preferred) = preferred {
            variants.push(main);
            main = plain(&preferred);
        }
        variants.extend(extra.iter().map(plain));
        if let Some(full) = shown.full {
            variants.push(Answer {
                display: full.clone(),
                raw: full.replace(',', "."),
                read_as: None,
            });
        }
        // Other valid readings, with what they were read as.
        for (rendered, outcome) in valid.iter().skip(1).take(MAX_OTHER_READINGS) {
            if rendered.trivial {
                continue;
            }
            if let Outcome::Value { text, approx } = outcome {
                let s = self.shown(text, *approx);
                variants.push(Answer {
                    display: s.display,
                    raw: s.raw,
                    read_as: Some(rendered.read_as.clone()),
                });
            }
        }
        let mut seen = vec![main.display.clone()];
        variants.retain(|v| {
            let new = !seen.contains(&v.display);
            seen.push(v.display.clone());
            new
        });
        Some(Calculation::Values { main, variants })
    }

    fn shown(&self, text: &str, approx: bool) -> Shown {
        format::shown(text, approx, self.style, &self.vocabulary)
    }

    /// A better main form (fraction, kWh) and extra forms (decimal and
    /// binary bytes).
    fn value_variants(
        &self,
        rendered: &Rendered,
        text: &str,
        deadline: Deadline,
    ) -> (Option<Shown>, Vec<Shown>) {
        let convert = |target: &str| -> Option<Shown> {
            let (t, a) = eval::fend(&format!("({}) to {target}", rendered.fend), deadline).ok()?;
            Some(self.shown(&t, a))
        };
        let (number, unit) = match format::split_number(text) {
            Some((n, u)) => (Some(n), u.to_owned()),
            None => (None, String::new()),
        };
        // Fractions typed: `1/3 + 1/6` → `1/2` (the decimal becomes a
        // variant); a single one (`10/4`) keeps the decimal, with the
        // fraction as a variant.
        if rendered.fractions > 0
            && number.as_ref().is_some_and(|n| !n.is_integer())
            && let Ok((fraction, false)) =
                eval::fend(&format!("({}) to fraction", rendered.fend), deadline)
            && fraction.len() <= 24
        {
            let fraction = self.shown(&fraction, false);
            return if rendered.fractions >= 2 {
                (Some(fraction), Vec::new())
            } else {
                (None, vec![fraction])
            };
        }
        // Energy from power × time: `10 kW * 3h` → `30 kWh`.
        if !rendered.converts && matches!(unit.as_str(), "J" | "joule" | "joules") {
            let watts = rendered
                .units
                .iter()
                .find(|u| u.ends_with('W') && u.len() <= 2);
            if let Some(w) = watts
                && let Some(s) = convert(&format!("{w}h"))
            {
                return (Some(s), Vec::new());
            }
        }
        // Bytes: decimal and binary prefixes side by side (`Mo` and `Mio`),
        // unless a unit was asked for.
        if rendered.converts {
            return (None, Vec::new());
        }
        let pairs = [
            ("kB", "KiB"),
            ("MB", "MiB"),
            ("GB", "GiB"),
            ("TB", "TiB"),
            ("PB", "PiB"),
        ];
        let other = pairs.iter().find_map(|(d, b)| {
            if unit == *d {
                Some(*b)
            } else if unit == *b {
                Some(*d)
            } else {
                None
            }
        });
        let extra = other.and_then(convert).into_iter().collect();
        (None, extra)
    }
}

// ---------------------------------------------------------------- provider

/// Texts of the calculator results, in the interface language.
#[derive(Debug, Clone)]
pub struct Texts {
    /// `lu comme : {}` / `read as: {}` (`%s` is replaced).
    pub read_as: String,
    pub too_big: String,
    /// `%s est premier` / `%s is prime`.
    pub prime: String,
    /// `%s n'est pas premier` / `%s is not prime`.
    pub not_prime: String,
}

impl Texts {
    pub fn new(i18n: &flair_core::i18n::I18n) -> Self {
        Self {
            read_as: i18n.t_args("calc-read-as", &[("reading", "%s")]),
            too_big: i18n.t("calc-too-big"),
            prime: i18n.t_args("calc-prime", &[("n", "%s")]),
            not_prime: i18n.t_args("calc-not-prime", &[("n", "%s")]),
        }
    }
}

/// The calculator as a result source.
pub struct CalcProvider {
    calculator: RwLock<Arc<Calculator>>,
    texts: RwLock<Texts>,
}

/// Id of the main calculator result (the view shows it larger).
pub const MAIN_ID: &str = "calc:main";

impl CalcProvider {
    pub fn new(calculator: Calculator, texts: Texts) -> Self {
        Self {
            calculator: RwLock::new(Arc::new(calculator)),
            texts: RwLock::new(texts),
        }
    }

    /// New settings or lexicons.
    pub fn replace(&self, calculator: Calculator, texts: Texts) {
        if let Ok(mut c) = self.calculator.write() {
            *c = Arc::new(calculator);
        }
        if let Ok(mut t) = self.texts.write() {
            *t = texts;
        }
    }

    fn item(
        id: String,
        title: String,
        subtitle: Option<String>,
        copy: String,
        raw: String,
        input: &str,
        score: f32,
    ) -> ResultItem {
        let expression = format!("{input} = {title}");
        ResultItem {
            id,
            title,
            subtitle,
            icon: IconSource::Builtin(BuiltinIcon::Calculator),
            score,
            match_indices: Vec::new(),
            actions: vec![
                Action::CopyToClipboard { text: copy },
                Action::CopyToClipboard { text: expression },
                Action::CopyToClipboard { text: raw },
            ],
            provider: ProviderKind::Calculator,
        }
    }
}

/// Calculator results come first: above any app.
const SCORE: f32 = 100.0;

impl Provider for CalcProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Calculator
    }

    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem> {
        if q.is_empty() || !q.accepts(ProviderKind::Calculator) {
            return Vec::new();
        }
        let Ok(calculator) = self.calculator.read().map(|c| Arc::clone(&c)) else {
            return Vec::new();
        };
        let Ok(texts) = self.texts.read().map(|t| t.clone()) else {
            return Vec::new();
        };
        let input = q.text.as_str();
        match calculator.calculate(input, cancel) {
            None => Vec::new(),
            Some(Calculation::TooBig) => vec![ResultItem {
                id: MAIN_ID.into(),
                title: texts.too_big,
                subtitle: Some(input.to_owned()),
                icon: IconSource::Builtin(BuiltinIcon::Calculator),
                score: SCORE,
                match_indices: Vec::new(),
                actions: Vec::new(),
                provider: ProviderKind::Calculator,
            }],
            Some(Calculation::Prime { n, prime, factor }) => {
                let template = if prime {
                    &texts.prime
                } else {
                    &texts.not_prime
                };
                let mut title = template.replace("%s", &n.to_string());
                if let Some(f) = factor {
                    title.push_str(&format!(" ({f} × {})", n / f));
                }
                vec![Self::item(
                    MAIN_ID.into(),
                    title.clone(),
                    Some(input.to_owned()),
                    title,
                    n.to_string(),
                    input,
                    SCORE,
                )]
            }
            Some(Calculation::Values { main, variants }) => {
                let mut out = vec![Self::item(
                    MAIN_ID.into(),
                    main.display.clone(),
                    Some(input.to_owned()),
                    main.display,
                    main.raw,
                    input,
                    SCORE,
                )];
                for (i, v) in variants.into_iter().enumerate() {
                    let note = v.read_as.map(|r| texts.read_as.replace("%s", &r));
                    #[allow(clippy::cast_precision_loss)]
                    let score = SCORE - 1.0 - i as f32 * 0.01;
                    out.push(Self::item(
                        format!("calc:{i}"),
                        v.display.clone(),
                        note,
                        v.display,
                        v.raw,
                        input,
                        score,
                    ));
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(input: &str) -> Option<Calculation> {
        Calculator::new(&Settings::default(), &[]).calculate(input, &CancellationToken::new())
    }

    fn main(input: &str) -> String {
        match calc(input) {
            Some(Calculation::Values { main, .. }) => main.display,
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn detection() {
        assert!(looks_like_calculation("pi*2"));
        assert!(!looks_like_calculation("firefox"));
        assert_eq!(calc("firefox"), None);
        assert_eq!(calc("rapport 2024.pdf"), None);
        assert_eq!(calc("c++"), None);
        assert_eq!(calc("42"), None);
    }

    #[test]
    fn a_few_results() {
        assert_eq!(main("15% de 240"), "36");
        assert_eq!(main("1/3 + 1/6"), "1/2");
        assert_eq!(main("5 km en miles"), "3,106855961 miles");
        assert_eq!(main("1 Gio en Mo"), "1073,741824 Mo");
        assert_eq!(main("10 kW * 3h"), "30 kWh");
        assert_eq!(calc("10^10^10^10"), Some(Calculation::TooBig));
    }

    #[test]
    fn readings_become_variants() {
        let Some(Calculation::Values { main, variants }) = calc("5m * 2") else {
            panic!("no result");
        };
        assert_eq!(main.display, "10 m");
        assert_eq!(variants[0].display, "10 min");
        assert!(variants[0].read_as.is_some());
    }
}

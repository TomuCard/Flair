//! Calculator: a 4-step pipeline, testable without the interface.
//!
//! 1. **Detection** ([`looks_like_calculation`]): cheap test before anything.
//! 2. **Normalization** ([`token`], [`normalize`]): tokens, lexicon senses
//!    with their conditions, every reading rendered in `fend-core` syntax.
//! 3. **Evaluation** ([`eval`]): fend with a deadline, plus the functions fend
//!    lacks. Readings that fail are dropped: evaluation settles ambiguities.
//! 4. **Formatting** ([`format`]): main result and variants in the interface
//!    language.

pub mod currency;
pub mod eval;
pub mod format;
pub mod normalize;
pub mod temporal;
pub mod token;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use flair_core::lexicon::DateOrder;
use flair_core::lexicon::{self, Lexicon};
use flair_core::result::BuiltinIcon;
use flair_core::settings::config::{AngleUnit, DateFormat, DecimalSeparator};
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem,
};

use eval::{Deadline, Env, EvalError, Outcome};
use format::{Shown, Style};
use normalize::{RenderOptions, Rendered, Vocabulary};
use token::DecimalMode;

/// Time allowed for all the readings of one input.
pub const BUDGET: Duration = Duration::from_millis(50);
/// Readings shown as variants besides the best one.
const MAX_OTHER_READINGS: usize = 2;

/// Source of the current time, injected so that tests are deterministic.
pub trait Clock: Send + Sync {
    fn now(&self) -> jiff::ZonedDateTime;
}

/// The system clock, in the system time zone.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> jiff::ZonedDateTime {
        jiff::ZonedDateTime::now()
    }
}

/// A fixed time (tests).
pub struct FixedClock(pub jiff::ZonedDateTime);

impl Clock for FixedClock {
    fn now(&self) -> jiff::ZonedDateTime {
        self.0.clone()
    }
}

/// Words of results that do not come from the lexicons.
#[derive(Debug, Clone)]
pub struct Labels {
    pub next_day: String,
    pub previous_day: String,
    pub local_time: String,
    /// `%s` is replaced by the date of the exchange rates.
    pub rates_date: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            next_day: "next day".into(),
            previous_day: "previous day".into(),
            local_time: "local time".into(),
            rates_date: "exchange rates of %s".into(),
        }
    }
}

impl Labels {
    pub fn new(i18n: &flair_core::i18n::I18n) -> Self {
        Self {
            next_day: i18n.t("calc-next-day"),
            previous_day: i18n.t("calc-previous-day"),
            local_time: i18n.t("calc-local-time"),
            rates_date: i18n.t_args("calc-rates-date", &[("date", "%s")]),
        }
    }
}

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
    pub date_format: DateFormat,
    pub labels: Labels,
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
            date_format: DateFormat::Auto,
            labels: Labels::default(),
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
    /// `1/0`, `5 mod 0`.
    DivisionByZero,
    /// Outside the domain of a function: `log(0)`, `tan(90)`.
    Undefined,
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

/// Units written with their power glued: `m2`, `cm3`, `km²`.
const GLUED_POWER_UNITS: &[&str] = &["mm", "cm", "dm", "m", "km", "ft", "yd", "mi", "µm"];

/// `m²` → `m^2`, `20 m2` → `20 m^2` (but `5m2s` stays 5 min 2 s).
fn powers(input: &str) -> String {
    let input = input.replace('²', "^2").replace('³', "^3");
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let starts_word = chars[i].is_alphabetic() && (i == 0 || !chars[i - 1].is_alphabetic());
        if starts_word {
            let end = (i..chars.len())
                .find(|&k| !chars[k].is_alphabetic())
                .unwrap_or(chars.len());
            let word: String = chars[i..end].iter().collect();
            out.push_str(&word);
            let power = chars.get(end).is_some_and(|c| *c == '2' || *c == '3');
            let alone = !chars.get(end + 1).is_some_and(|c| c.is_alphanumeric());
            // Not inside a compound duration (`1h45m2`).
            let digits = chars[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .count();
            let compound = digits > 0 && i > digits && chars[i - digits - 1].is_alphabetic();
            if GLUED_POWER_UNITS.contains(&word.as_str()) && power && alone && !compound {
                out.push('^');
            }
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// A result in squared minutes or minutes times metres: the `m` of a
/// reading taken for minutes (`5 m² + 3 m²` read as `5 min²…`). Never
/// worth showing as another reading.
fn odd_minutes(evaluated: &Evaluated) -> bool {
    let Evaluated::Fend(Outcome::Value { text, .. }) = evaluated else {
        return false;
    };
    let Some((_, unit)) = format::split_number(text) else {
        return false;
    };
    let words: Vec<&str> = unit
        .split([' ', '/', '*'])
        .filter(|w| !w.is_empty())
        .collect();
    let minutes = |w: &str| w == "min" || w == "mins";
    words
        .iter()
        .any(|w| w.starts_with("min^") || w.starts_with("mins^"))
        || (words.len() > 1 && words.iter().any(|w| minutes(w)))
}

/// `10000 m^2` → `10000 m²` (powers of units only).
fn superscripts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut previous = ' ';
    while let Some(c) = chars.next() {
        if c == '^' && previous.is_alphabetic() {
            match chars.peek() {
                Some('2') => {
                    out.push('²');
                    chars.next();
                    previous = '²';
                    continue;
                }
                Some('3') => {
                    out.push('³');
                    chars.next();
                    previous = '³';
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
        previous = c;
    }
    out
}

/// Words that make an input without digits a calculation.
fn calculation_word(segment: &normalize::Segment) -> bool {
    segment.candidates.iter().any(|c| match &c.sense {
        normalize::Sense::Lexicon(s) => {
            s == "convert_to"
                || s == "one"
                || s.starts_with("op:")
                || s.starts_with("func:")
                || (flair_core::lexicon::SENSES.contains(&s.as_str())
                    && !matches!(s.as_str(), "of" | "percent" | "ignore"))
        }
        _ => false,
    })
}

/// A currency unit (`EUR`, `$`, `euros`).
fn is_currency(unit: &str, rates: &currency::Rates) -> bool {
    matches!(
        unit,
        "€" | "$" | "£" | "¥" | "euro" | "euros" | "dollar" | "dollars"
    ) || rates.per_euro.contains_key(&unit.to_uppercase())
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
    clock: Arc<dyn Clock>,
    names: temporal::Names,
    order: DateOrder,
    /// Exchange rates, when currency conversions are enabled.
    rates: Option<currency::SharedRates>,
    /// `taux du %s` / `rates of %s`.
    rates_label: String,
}

/// A reading evaluated.
enum Evaluated {
    Fend(Outcome),
    /// Already formatted (dates and times).
    Ready(Answer, Vec<Answer>),
}

const TIME_RESULT_UNITS: &[&str] = &[
    "s", "second", "seconds", "min", "mins", "minute", "minutes", "h", "hour", "hours", "hr",
    "hrs", "ms",
];

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
        let order = match settings.date_format {
            DateFormat::Dmy => DateOrder::Dmy,
            DateFormat::Mdy => DateOrder::Mdy,
            DateFormat::Ymd => DateOrder::Ymd,
            DateFormat::Auto => convention
                .and_then(|l| l.meta.date_order)
                .unwrap_or(DateOrder::Dmy),
        };
        let unit_names = |unit: &str| {
            (
                vocabulary.unit_name(unit, false).unwrap_or(unit).to_owned(),
                vocabulary.unit_name(unit, true).unwrap_or(unit).to_owned(),
            )
        };
        let names = temporal::Names {
            months: vocabulary.month_names(),
            weekdays: vocabulary.weekday_names(),
            day: unit_names("day"),
            week: unit_names("week"),
            month: unit_names("month"),
            year: unit_names("year"),
            hour: unit_names("hour"),
            minute: unit_names("minute"),
            second: unit_names("second"),
            next_day: settings.labels.next_day.clone(),
            previous_day: settings.labels.previous_day.clone(),
            local_time: settings.labels.local_time.clone(),
        };
        Self {
            rates: None,
            rates_label: settings.labels.rates_date.clone(),
            clock: Arc::new(SystemClock),
            names,
            order,
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

    /// Enables currency conversions with these rates.
    #[must_use]
    pub fn with_rates(mut self, rates: currency::SharedRates) -> Self {
        self.rates = Some(rates);
        self
    }

    /// Uses another clock (tests).
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
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
            &Env::new(Deadline::after(Duration::from_millis(20))),
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
        let env = Env {
            deadline: Deadline::after(BUDGET),
            rates: self.rates.clone(),
        };
        let deadline = &env;
        let input = &powers(input);
        let tokens = token::tokenize(input, self.mode);
        let symbols = looks_like_calculation(input);
        // Without digits nor symbols, a calculation needs at least two words
        // (`une heure en minutes`), never one (`firefox`), except a day or
        // the time (`demain`, `now`).
        if !symbols && tokens.len() > 1 {
            // Checked below, on the words.
        } else if !symbols && tokens.len() == 1 {
            let segments = normalize::segments(&tokens, &self.vocabulary, &|_| false);
            let alone = segments.iter().all(|s| {
                s.candidates.iter().any(|c| {
                    matches!(&c.sense, normalize::Sense::Lexicon(x)
                        if matches!(x.as_str(), "now" | "today" | "tomorrow" | "yesterday"))
                })
            });
            if !alone {
                return None;
            }
        } else if !symbols {
            return None;
        }
        let segments = normalize::segments(&tokens, &self.vocabulary, &|w| self.is_unit(w));
        if !symbols && !segments.iter().any(calculation_word) {
            return None;
        }
        let cx = temporal::Context {
            now: self.clock.now(),
            order: self.order,
            french: self.style.french,
            names: &self.names,
        };
        let mut valid: Vec<(Rendered, Evaluated)> = Vec::new();
        // The error of the best reading that failed for a reason worth
        // telling (`1/0`, `log(0)`, `10^10^10^10`).
        let mut error: Option<Calculation> = None;
        for reading in normalize::readings(&segments) {
            if cancel.is_cancelled() {
                return None;
            }
            let Some(rendered) = normalize::render(&segments, &reading, self.options) else {
                continue;
            };
            // Dates and times first, by their own evaluator.
            if let Some(result) = temporal::evaluate(&rendered.pieces, &cx) {
                if let Some((main, variants)) = result
                    && !valid.iter().any(|(r, _)| r.pieces == rendered.pieces)
                {
                    valid.push((rendered, Evaluated::Ready(main, variants)));
                }
                continue;
            }
            let Some(fend) = rendered.fend.clone() else {
                continue;
            };
            if valid.iter().any(|(r, _)| r.fend.as_ref() == Some(&fend)) {
                continue;
            }
            match eval::evaluate(&fend, deadline) {
                Ok(outcome) => valid.push((rendered, Evaluated::Fend(outcome))),
                Err(EvalError::TooBig) => {
                    error.get_or_insert(Calculation::TooBig);
                }
                Err(EvalError::DivisionByZero) => {
                    error.get_or_insert(Calculation::DivisionByZero);
                }
                Err(EvalError::Undefined) => {
                    error.get_or_insert(Calculation::Undefined);
                }
                Err(EvalError::Invalid) => {}
            }
        }
        let Some((best, evaluated)) = valid.first() else {
            return error;
        };
        if best.trivial {
            return None;
        }
        let (mut main, mut variants) = match evaluated {
            Evaluated::Ready(main, variants) => (main.clone(), variants.clone()),
            Evaluated::Fend(Outcome::Prime { n, prime, factor }) => {
                return Some(Calculation::Prime {
                    n: *n,
                    prime: *prime,
                    factor: *factor,
                });
            }
            Evaluated::Fend(Outcome::Value { text, approx }) => {
                self.fend_answers(best, text, *approx, deadline)
            }
        };
        // Other valid readings, with what they were read as.
        for (rendered, evaluated) in valid
            .iter()
            .skip(1)
            .filter(|(_, e)| !odd_minutes(e))
            .take(MAX_OTHER_READINGS)
        {
            if rendered.trivial {
                continue;
            }
            let display = match evaluated {
                Evaluated::Fend(Outcome::Value { text, approx }) => {
                    self.fend_answers(rendered, text, *approx, deadline).0
                }
                Evaluated::Ready(main, _) => main.clone(),
                Evaluated::Fend(Outcome::Prime { .. }) => continue,
            };
            variants.push(Answer {
                read_as: Some(rendered.read_as.clone()),
                ..display
            });
        }
        let mut seen = vec![main.display.clone()];
        variants.retain(|v| {
            let new = !v.display.is_empty() && !seen.contains(&v.display);
            seen.push(v.display.clone());
            new
        });
        main.read_as = None;
        Some(Calculation::Values { main, variants })
    }

    /// Main result and variants of a fend value.
    fn fend_answers(
        &self,
        rendered: &Rendered,
        text: &str,
        approx: bool,
        deadline: &Env,
    ) -> (Answer, Vec<Answer>) {
        let plain = |s: &Shown| Answer {
            display: s.display.clone(),
            raw: s.raw.clone(),
            read_as: None,
        };
        if let Some(durations) = self.duration_answers(rendered, text, deadline) {
            return durations;
        }
        // Integers computed from other bases (`0xff & 0x0f`) or with bitwise
        // operators: decimal first, then hexadecimal, binary and octal.
        if let Some(answers) = self.base_answers(rendered, text, deadline) {
            return answers;
        }
        let mut shown = self.shown(text, approx);
        if let Some(prefix) = rendered.base_prefix
            && !shown.display.starts_with(prefix)
        {
            shown.display = format!("{prefix}{}", shown.display);
            shown.raw = shown.display.clone();
        }
        let mut main = plain(&shown);
        let mut variants: Vec<Answer> = Vec::new();
        let (preferred, extra) = self.value_variants(rendered, text, deadline);
        if let Some(preferred) = preferred {
            variants.push(main);
            main = plain(&preferred);
        }
        variants.extend(extra.iter().map(plain));
        // Currencies: the date of the rates used.
        if let Some(rates) = self.rates.as_ref().and_then(|r| r.read().ok()?.clone())
            && rendered.units.iter().any(|u| is_currency(u, &rates))
        {
            variants.push(Answer {
                display: self.rates_label.replace("%s", &rates.date),
                raw: rates.date.clone(),
                read_as: None,
            });
        }
        if let Some(full) = shown.full {
            variants.push(Answer {
                display: full.clone(),
                raw: full.replace(',', "."),
                read_as: None,
            });
        }
        main.display = superscripts(&main.display);
        for v in &mut variants {
            v.display = superscripts(&v.display);
        }
        (main, variants)
    }

    fn base_answers(
        &self,
        rendered: &Rendered,
        text: &str,
        deadline: &Env,
    ) -> Option<(Answer, Vec<Answer>)> {
        let fend = rendered.fend.as_ref()?;
        if rendered.converts {
            return None;
        }
        let bases = ["0x", "0b", "0o"].iter().any(|b| fend.contains(b));
        let bitwise = [" and ", " or ", " xor ", "<<", ">>", "bitnot("]
            .iter()
            .any(|o| fend.contains(o));
        if !bases && !bitwise {
            return None;
        }
        let decimal =
            if format::split_number(text).is_some_and(|(n, u)| u.is_empty() && n.is_integer()) {
                text.to_owned()
            } else {
                eval::fend(&format!("({fend}) to decimal"), deadline)
                    .ok()?
                    .0
            };
        let (n, unit) = format::split_number(&decimal)?;
        if !unit.is_empty() || !n.is_integer() {
            return None;
        }
        let item = |display: String| Answer {
            raw: display.clone(),
            display,
            read_as: None,
        };
        let main = item(n.plain('.'));
        let mut variants = Vec::new();
        for (base, prefix) in [("hex", "0x"), ("binary", "0b"), ("octal", "0o")] {
            if let Ok((t, _)) = eval::fend(&format!("({fend}) to {base}"), deadline) {
                let shown = if t.starts_with(prefix) || t.starts_with('-') {
                    t
                } else {
                    format!("{prefix}{t}")
                };
                variants.push(item(shown));
            }
        }
        Some((main, variants))
    }

    /// Durations shown as `6h41` (and times of day as `17h15`, `2h
    /// (lendemain)`), from a fend result in a time unit.
    fn duration_answers(
        &self,
        rendered: &Rendered,
        text: &str,
        deadline: &Env,
    ) -> Option<(Answer, Vec<Answer>)> {
        use normalize::Piece;
        let fend = rendered.fend.as_ref()?;
        let (_, unit) = format::split_number(text)?;
        if !TIME_RESULT_UNITS.contains(&unit) {
            return None;
        }
        let p = &rendered.pieces;
        // `5m2s`: a compound duration adds its parts.
        let compound = p.windows(4).any(|w| {
            matches!(w, [Piece::Num(_), Piece::Unit(a), Piece::Num(_), Piece::Unit(b)]
                if TIME_RESULT_UNITS.contains(&a.as_str()) && TIME_RESULT_UNITS.contains(&b.as_str()))
        });
        let operations = compound
            || p.iter()
                .any(|x| matches!(x, Piece::Op(o) if "+-*/".contains(o.as_str())));
        if !(rendered.transfer || rendered.clock_target || (!rendered.converts && operations)) {
            return None;
        }
        let (seconds, _) = eval::fend(&format!("({fend}) to s"), deadline).ok()?;
        let seconds: f64 = format::split_number(&seconds)?.0.plain('.').parse().ok()?;
        let french = self.style.french;
        let item = |display: String, raw: String| Answer {
            display,
            raw,
            read_as: None,
        };

        // A time of day plus or minus a duration: `14:30 + 2h45`, `23h + 3h`.
        let hour = |n: &str| n.parse::<u32>().is_ok_and(|h| h < 24);
        let clock_start = match (p.first(), p.get(1)) {
            (Some(Piece::Num(n)), Some(Piece::Sym(c))) => c == ":" && hour(n),
            (Some(Piece::Num(n)), Some(Piece::Unit(u))) => {
                matches!(u.as_str(), "h" | "hour" | "hours") && hour(n)
            }
            _ => false,
        };
        let only_add = p
            .iter()
            .all(|x| !matches!(x, Piece::Op(o) if o == "*" || o == "/"));
        let written_clock = p.get(1) == Some(&Piece::Sym(":".into()));
        let overflow = !(0.0..86_400.0).contains(&seconds);
        if clock_start
            && only_add
            && !rendered.converts
            && !rendered.transfer
            && (written_clock || overflow)
        {
            let day = (seconds / 86_400.0).floor();
            let in_day = seconds - day * 86_400.0;
            #[allow(clippy::cast_possible_truncation)]
            let minutes = (in_day / 60.0).round() as i64;
            let (h, m) = (minutes / 60 % 24, minutes % 60);
            let label = if day == 0.0 {
                String::new()
            } else if day == 1.0 {
                format!(" ({})", self.names.next_day)
            } else if day == -1.0 {
                format!(" ({})", self.names.previous_day)
            } else {
                format!(" ({day:+})")
            };
            let shown = temporal::clock(h, m, french);
            return Some((
                item(format!("{shown}{label}"), format!("{h:02}:{m:02}")),
                Vec::new(),
            ));
        }
        let main = item(
            format::duration(seconds, french),
            format::decimal(seconds, 3, false),
        );
        Some((main, format::duration_variants(seconds, french)))
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
        deadline: &Env,
    ) -> (Option<Shown>, Vec<Shown>) {
        let convert = |target: &str| -> Option<Shown> {
            let fend = rendered.fend.as_ref()?;
            let (t, a) = eval::fend(&format!("({fend}) to {target}"), deadline).ok()?;
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
            && let Some(fend) = rendered.fend.as_ref()
            && let Ok((fraction, false)) = eval::fend(&format!("({fend}) to fraction"), deadline)
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
    pub division_by_zero: String,
    pub undefined: String,
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
            division_by_zero: i18n.t("calc-division-by-zero"),
            undefined: i18n.t("calc-undefined"),
            prime: i18n.t_args("calc-prime", &[("n", "%s")]),
            not_prime: i18n.t_args("calc-not-prime", &[("n", "%s")]),
        }
    }
}

/// The calculator as a result source.
pub struct CalcProvider {
    calculator: RwLock<Arc<Calculator>>,
    texts: RwLock<Texts>,
    history: Option<Arc<Mutex<flair_core::history::History>>>,
}

/// Id of the main calculator result (the view shows it larger).
pub const MAIN_ID: &str = "calc:main";
/// Id of the last valid result, kept greyed while typing goes on.
pub const STALE_ID: &str = "calc:stale";

impl CalcProvider {
    pub fn new(calculator: Calculator, texts: Texts) -> Self {
        Self {
            calculator: RwLock::new(Arc::new(calculator)),
            texts: RwLock::new(texts),
            history: None,
        }
    }

    /// Shows this history for `=` alone.
    #[must_use]
    pub fn with_history(mut self, history: Arc<Mutex<flair_core::history::History>>) -> Self {
        self.history = Some(history);
        self
    }

    /// `=` alone: the last calculations, evaluated again.
    fn history_rows(&self, calculator: &Calculator, cancel: &CancellationToken) -> Vec<ResultItem> {
        let Some(items) = self
            .history
            .as_ref()
            .and_then(|h| h.lock().ok().map(|h| h.items().to_vec()))
        else {
            return Vec::new();
        };
        items
            .iter()
            .enumerate()
            .filter_map(|(i, input)| {
                let Some(Calculation::Values { main, .. }) = calculator.calculate(input, cancel)
                else {
                    return None;
                };
                #[allow(clippy::cast_precision_loss)]
                let score = SCORE - i as f32;
                Some(Self::item(
                    format!("calc:history:{i}"),
                    main.display.clone(),
                    Some(input.clone()),
                    main.display,
                    main.raw,
                    input,
                    score,
                ))
            })
            .collect()
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
        if !q.accepts(ProviderKind::Calculator) {
            return Vec::new();
        }
        let Ok(calculator) = self.calculator.read().map(|c| Arc::clone(&c)) else {
            return Vec::new();
        };
        if q.is_empty() {
            return if q.filter == Some(ProviderKind::Calculator) {
                self.history_rows(&calculator, cancel)
            } else {
                Vec::new()
            };
        }
        let Ok(texts) = self.texts.read().map(|t| t.clone()) else {
            return Vec::new();
        };
        let input = q.text.as_str();
        match calculator.calculate(input, cancel) {
            None => Vec::new(),
            Some(
                error
                @ (Calculation::TooBig | Calculation::DivisionByZero | Calculation::Undefined),
            ) => vec![ResultItem {
                id: MAIN_ID.into(),
                title: match error {
                    Calculation::TooBig => texts.too_big,
                    Calculation::DivisionByZero => texts.division_by_zero,
                    _ => texts.undefined,
                },
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
    fn equal_sign_alone_lists_the_history() {
        let mut history = flair_core::history::History::in_memory(10);
        history.record("15% de 240");
        history.record("not a calculation");
        let provider = CalcProvider::new(
            Calculator::new(&Settings::default(), &[]),
            Texts::new(&flair_core::i18n::I18n::new(flair_core::i18n::Lang::Fr)),
        )
        .with_history(Arc::new(Mutex::new(history)));
        let rows = provider.query(&Query::parse("="), &CancellationToken::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "36");
        assert_eq!(rows[0].subtitle.as_deref(), Some("15% de 240"));
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

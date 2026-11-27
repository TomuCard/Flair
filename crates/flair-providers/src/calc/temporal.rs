//! The date and time evaluator: what fend does not read (dates in French or
//! English, `now`, `ago`, `until`, Unix timestamps, time zones).
//!
//! It works on the pieces of a reading. It claims the reading when it holds
//! a date, a time word or a time zone; plain durations (`5h36 + 65min`) are
//! left to fend and only formatted as durations.

use std::sync::OnceLock;

use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Span, Timestamp, Unit, ZonedDateTime};

use flair_core::lexicon::DateOrder;

use super::Answer;
use super::normalize::Piece;

/// What the date and time evaluator needs besides the pieces.
pub struct Context<'a> {
    pub now: ZonedDateTime,
    pub order: DateOrder,
    pub french: bool,
    pub names: &'a Names,
}

/// Words of the results, in the interface language.
#[derive(Debug, Clone)]
pub struct Names {
    /// January … December.
    pub months: Vec<String>,
    /// Monday … Sunday.
    pub weekdays: Vec<String>,
    /// `day` / `days` (`jour` / `jours`).
    pub day: (String, String),
    pub week: (String, String),
    pub next_day: String,
    pub previous_day: String,
    pub local_time: String,
}

impl Names {
    fn month(&self, m: i8) -> &str {
        usize::try_from(m - 1)
            .ok()
            .and_then(|i| self.months.get(i))
            .map_or("?", String::as_str)
    }

    fn weekday(&self, d: Date) -> &str {
        usize::try_from(d.weekday().to_monday_zero_offset())
            .ok()
            .and_then(|i| self.weekdays.get(i))
            .map_or("?", String::as_str)
    }
}

/// A calendar duration: months and days are counted on the calendar.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Dur {
    months: i64,
    days: i64,
    seconds: f64,
}

#[derive(Debug, Clone)]
enum Value {
    Instant(ZonedDateTime),
    Day(Date),
    Dur(Dur),
    Number(f64),
}

/// Seconds, days or months of a time unit (fend and lexicon names).
fn time_unit(name: &str) -> Option<Dur> {
    let d = |months, days, seconds| {
        Some(Dur {
            months,
            days,
            seconds,
        })
    };
    match name {
        "year" | "years" | "yr" => d(12, 0, 0.0),
        "month" | "months" => d(1, 0, 0.0),
        "week" | "weeks" => d(0, 7, 0.0),
        "day" | "days" => d(0, 1, 0.0),
        "h" | "hour" | "hours" | "hr" => d(0, 0, 3600.0),
        "min" | "minute" | "minutes" | "mins" => d(0, 0, 60.0),
        "s" | "second" | "seconds" | "sec" => d(0, 0, 1.0),
        "ms" => d(0, 0, 0.001),
        _ => None,
    }
}

/// True when the pieces need this evaluator.
pub fn claims(pieces: &[Piece]) -> bool {
    // A city with a time zone (`15h Tokyo en Paris`).
    let city = (0..pieces.len())
        .any(|i| matches!(pieces[i], Piece::Value(_)) && zone(pieces, i).is_some());
    city || pieces.iter().any(|p| match p {
        Piece::Sense(s) => s != "rate_at",
        Piece::Month(_) | Piece::Weekday(_) | Piece::Zone(_) => true,
        Piece::Sym(s) => s == "@",
        _ => false,
    }) || (0..pieces.len()).any(|i| numeric_date(pieces, i, DateOrder::Dmy).is_some())
}

fn number(p: Option<&Piece>) -> Option<&str> {
    match p {
        Some(Piece::Num(n)) => Some(n.as_str()),
        _ => None,
    }
}

fn int(p: Option<&Piece>) -> Option<i64> {
    number(p)?.parse().ok()
}

fn is_op(p: Option<&Piece>, op: &str) -> bool {
    matches!(p, Some(Piece::Op(o)) if o == op)
}

/// `26/11/2026` (order of the settings) or `2026-11-26`.
fn numeric_date(p: &[Piece], i: usize, order: DateOrder) -> Option<(Date, usize)> {
    let (a, b, c) = (
        number(p.get(i))?,
        number(p.get(i + 2)),
        number(p.get(i + 4)),
    );
    if a.len() == 4 && is_op(p.get(i + 1), "-") && is_op(p.get(i + 3), "-") {
        let date = Date::new(a.parse().ok()?, b?.parse().ok()?, c?.parse().ok()?).ok()?;
        return Some((date, i + 5));
    }
    if is_op(p.get(i + 1), "/") && is_op(p.get(i + 3), "/") && c.is_some_and(|c| c.len() == 4) {
        let (x, y, z): (i64, i64, i64) = (a.parse().ok()?, b?.parse().ok()?, c?.parse().ok()?);
        let (d, m, y) = match order {
            DateOrder::Dmy => (x, y, z),
            DateOrder::Mdy => (y, x, z),
            DateOrder::Ymd => return None,
        };
        let date = Date::new(
            i16::try_from(y).ok()?,
            i8::try_from(m).ok()?,
            i8::try_from(d).ok()?,
        )
        .ok()?;
        return Some((date, i + 5));
    }
    None
}

struct Parser<'a> {
    p: Vec<Piece>,
    i: usize,
    cx: &'a Context<'a>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Piece> {
        self.p.get(self.i)
    }

    fn sense(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Piece::Sense(x)) if x == s)
    }

    fn today(&self) -> Date {
        self.cx.now.date()
    }

    /// A date written in any supported way; `short` allows `25/12` and
    /// `25 décembre` without year (then the year is chosen by `pick`).
    fn date(&mut self, pick: &dyn Fn(Date, Date) -> Date) -> Option<Date> {
        // A weekday before a date is decoration (`mardi 14 juillet 1789`).
        if matches!(self.peek(), Some(Piece::Weekday(_))) {
            self.i += 1;
        }
        if let Some((date, next)) = numeric_date(&self.p, self.i, self.cx.order) {
            self.i = next;
            return Some(date);
        }
        let i = self.i;
        let p = self.p.clone();
        let year_at = |k: usize| int(p.get(k)).filter(|y| (1..=9999).contains(y));
        let year_now = i64::from(self.today().year());
        let build = |y: Option<i64>, m: u8, d: i64| -> Option<Date> {
            let in_year = |y: i64| {
                Date::new(
                    i16::try_from(y).ok()?,
                    i8::try_from(m).ok()?,
                    i8::try_from(d).ok()?,
                )
                .ok()
            };
            match y {
                Some(y) => in_year(y),
                None => {
                    let current = in_year(year_now)?;
                    // The same day of the next or previous year, chosen by `pick`.
                    let next = in_year(year_now + 1).unwrap_or(current);
                    let previous = in_year(year_now - 1).unwrap_or(current);
                    let other = if pick(current, next) == next && next != current {
                        next
                    } else {
                        previous
                    };
                    Some(pick(current, other))
                }
            }
        };
        // `14 juillet 1789`, `1er janvier`.
        if let (Some(d), Some(Piece::Month(m))) = (int(p.get(i)), p.get(i + 1)) {
            let m = *m;
            let y = year_at(i + 2).filter(|y| *y >= 100);
            self.i = i + 2 + usize::from(y.is_some());
            return build(y, m, d);
        }
        // `July 14 1789`, `December 25`.
        if let (Some(Piece::Month(m)), Some(d)) = (p.get(i), int(p.get(i + 1))) {
            let m = *m;
            let y = year_at(i + 2).filter(|y| *y >= 100);
            self.i = i + 2 + usize::from(y.is_some());
            return build(y, m, d);
        }
        // `25/12` (after `until`, `since`, `what day`).
        if let (Some(a), true, Some(b)) =
            (int(p.get(i)), is_op(p.get(i + 1), "/"), int(p.get(i + 2)))
        {
            let (d, m) = match self.cx.order {
                DateOrder::Mdy => (b, a),
                _ => (a, b),
            };
            self.i = i + 3;
            return build(None, u8::try_from(m).ok()?, d);
        }
        None
    }

    /// `14h30`, `14:30`, `14h` after a date or before a time zone.
    fn time(&mut self) -> Option<Time> {
        let i = self.i;
        let h = int(self.p.get(i))?;
        let (m, next) = match self.p.get(i + 1) {
            Some(Piece::Sym(s)) if s == ":" => (int(self.p.get(i + 2))?, i + 3),
            Some(Piece::Unit(u)) if matches!(u.as_str(), "h" | "hour" | "hours") => {
                match int(self.p.get(i + 2)) {
                    Some(m) if !matches!(self.p.get(i + 3), Some(Piece::Unit(_))) => (m, i + 3),
                    _ => (0, i + 2),
                }
            }
            _ => return None,
        };
        let time = Time::new(i8::try_from(h).ok()?, i8::try_from(m).ok()?, 0, 0).ok()?;
        self.i = next;
        Some(time)
    }

    /// `3 h 30 min`, `45 jours`, `5h36`.
    fn duration(&mut self) -> Option<Dur> {
        let start = self.i;
        let mut total = Dur::default();
        let mut found = false;
        while let (Some(n), Some(Piece::Unit(u))) =
            (number(self.p.get(self.i)), self.p.get(self.i + 1))
        {
            let Some(unit) = time_unit(u) else { break };
            let n: f64 = n.parse().ok()?;
            if unit.months > 0 || unit.days > 0 {
                if n.fract() != 0.0 {
                    return None;
                }
                #[allow(clippy::cast_possible_truncation)]
                let n = n as i64;
                total.months += unit.months * n;
                total.days += unit.days * n;
            } else {
                total.seconds += unit.seconds * n;
            }
            found = true;
            self.i += 2;
            // `5h36`: minutes without their unit.
            if unit.seconds == 3600.0
                && let Some(m) = number(self.p.get(self.i))
                && !matches!(self.p.get(self.i + 1), Some(Piece::Unit(_)))
            {
                total.seconds += m.parse::<f64>().ok()? * 60.0;
                self.i += 1;
            }
        }
        if !found {
            self.i = start;
        }
        found.then_some(total)
    }

    fn term(&mut self) -> Option<Value> {
        let now = self.cx.now.clone();
        let today = self.today();
        let piece = self.peek()?.clone();
        match piece {
            Piece::Sense(s) => {
                self.i += 1;
                match s.as_str() {
                    "now" => Some(Value::Instant(now)),
                    "today" => Some(Value::Day(today)),
                    "tomorrow" => Some(Value::Day(today.tomorrow().ok()?)),
                    "yesterday" => Some(Value::Day(today.yesterday().ok()?)),
                    _ => None,
                }
            }
            Piece::Sym(s) if s == "@" => {
                self.i += 1;
                let seconds = int(self.peek())?;
                self.i += 1;
                Some(Value::Instant(
                    Timestamp::from_second(seconds)
                        .ok()?
                        .to_zoned_datetime(TimeZone::UTC),
                ))
            }
            Piece::Open => {
                self.i += 1;
                let v = self.sum()?;
                if self.peek() != Some(&Piece::Close) {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            _ => {
                let pick_current = |current: Date, _other: Date| current;
                if let Some(date) = self.date(&pick_current) {
                    // A time after the date: `26/11/2026 14h30`.
                    if let Some(time) = self.time() {
                        let zoned = date
                            .to_datetime(time)
                            .to_zoned_datetime(now.time_zone().clone())
                            .ok()?;
                        return Some(Value::Instant(zoned));
                    }
                    return Some(Value::Day(date));
                }
                if let Some(d) = self.duration() {
                    return Some(Value::Dur(d));
                }
                let n: f64 = number(self.peek())?.parse().ok()?;
                self.i += 1;
                Some(Value::Number(n))
            }
        }
    }

    fn product(&mut self) -> Option<Value> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Piece::Op(o)) if o == "*" || o == "/" => o.clone(),
                _ => return Some(left),
            };
            self.i += 1;
            let right = self.term()?;
            left = match (left, right, op.as_str()) {
                (Value::Dur(d), Value::Number(n), "*") | (Value::Number(n), Value::Dur(d), "*") => {
                    Value::Dur(scale(d, n)?)
                }
                (Value::Dur(d), Value::Number(n), "/") if n != 0.0 => {
                    Value::Dur(scale(d, 1.0 / n)?)
                }
                (Value::Number(a), Value::Number(b), "*") => Value::Number(a * b),
                (Value::Number(a), Value::Number(b), "/") if b != 0.0 => Value::Number(a / b),
                _ => return None,
            };
        }
    }

    fn sum(&mut self) -> Option<Value> {
        let mut left = self.product()?;
        loop {
            let minus = match self.peek() {
                Some(Piece::Op(o)) if o == "+" => false,
                Some(Piece::Op(o)) if o == "-" => true,
                _ => return Some(left),
            };
            self.i += 1;
            let right = self.product()?;
            left = combine(left, right, minus)?;
        }
    }
}

fn scale(d: Dur, n: f64) -> Option<Dur> {
    // Calendar parts only scale by whole numbers.
    #[allow(clippy::cast_precision_loss)]
    let seconds = d.seconds * n
        + if n.fract() == 0.0 {
            0.0
        } else {
            (d.days as f64) * 86_400.0 * n
        };
    #[allow(clippy::cast_possible_truncation)]
    Some(if n.fract() == 0.0 {
        Dur {
            months: d.months * n as i64,
            days: d.days * n as i64,
            seconds,
        }
    } else if d.months == 0 {
        Dur {
            months: 0,
            days: 0,
            seconds,
        }
    } else {
        return None;
    })
}

fn negate(d: Dur) -> Dur {
    Dur {
        months: -d.months,
        days: -d.days,
        seconds: -d.seconds,
    }
}

fn add_to_date(date: Date, d: Dur) -> Option<Date> {
    let span = Span::new()
        .try_months(d.months)
        .ok()?
        .try_days(d.days)
        .ok()?;
    date.checked_add(span).ok()
}

fn add_to_instant(z: &ZonedDateTime, d: Dur) -> Option<ZonedDateTime> {
    let span = Span::new()
        .try_months(d.months)
        .ok()?
        .try_days(d.days)
        .ok()?;
    #[allow(clippy::cast_possible_truncation)]
    let exact = SignedDuration::from_millis((d.seconds * 1000.0).round() as i64);
    z.checked_add(span).ok()?.checked_add(exact).ok()
}

fn combine(left: Value, right: Value, minus: bool) -> Option<Value> {
    let signed = |d: Dur| if minus { negate(d) } else { d };
    Some(match (left, right) {
        (Value::Day(date), Value::Dur(d)) if d.seconds == 0.0 => {
            Value::Day(add_to_date(date, signed(d))?)
        }
        (Value::Day(date), Value::Dur(d)) => {
            let z = date.to_zoned_datetime(TimeZone::system()).ok()?;
            Value::Instant(add_to_instant(&z, signed(d))?)
        }
        (Value::Instant(z), Value::Dur(d)) => Value::Instant(add_to_instant(&z, signed(d))?),
        (Value::Dur(d), Value::Day(date)) if !minus && d.seconds == 0.0 => {
            Value::Day(add_to_date(date, d)?)
        }
        (Value::Dur(a), Value::Dur(b)) => {
            let b = signed(b);
            Value::Dur(Dur {
                months: a.months + b.months,
                days: a.days + b.days,
                seconds: a.seconds + b.seconds,
            })
        }
        (Value::Day(a), Value::Day(b)) if minus => {
            let days = b.until((Unit::Day, a)).ok()?.get_days();
            Value::Dur(Dur {
                days: i64::from(days),
                ..Dur::default()
            })
        }
        (Value::Instant(a), Value::Instant(b)) if minus => {
            let seconds = a.timestamp().duration_since(b.timestamp()).as_secs_f64();
            Value::Dur(Dur {
                seconds,
                ..Dur::default()
            })
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------- time zones

/// Names of the time zone database (`Europe/Paris`…), read once.
fn zone_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        jiff::tz::db()
            .available()
            .map(|n| n.as_str().to_owned())
            .collect()
    })
}

/// A time zone from a city (`Tokyo`, `New York`) or a lexicon zone.
fn zone(p: &[Piece], i: usize) -> Option<(TimeZone, String, usize)> {
    if let Some(Piece::Zone(name)) = p.get(i) {
        return Some((TimeZone::get(name).ok()?, city_of(name), i + 1));
    }
    let mut words = Vec::new();
    let mut k = i;
    while let Some(Piece::Value(w)) = p.get(k) {
        words.push(w.clone());
        k += 1;
    }
    // The longest run of words naming a city.
    while !words.is_empty() {
        let city = words.join("_").to_lowercase();
        if let Some(name) = zone_names().iter().find(|n| {
            n.rsplit('/')
                .next()
                .is_some_and(|c| c.to_lowercase() == city)
        }) {
            return Some((TimeZone::get(name).ok()?, words.join(" "), i + words.len()));
        }
        words.pop();
    }
    None
}

fn city_of(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).replace('_', " ")
}

// ---------------------------------------------------------------- formatting

/// `17h15` / `17:15`; `2h` / `2:00`.
pub fn clock(h: i64, m: i64, french: bool) -> String {
    match (french, m) {
        (true, 0) => format!("{h}h"),
        (true, m) => format!("{h}h{m:02}"),
        (false, m) => format!("{h}:{m:02}"),
    }
}

fn date_text(d: Date, cx: &Context<'_>) -> String {
    let n = cx.names;
    if cx.french {
        let day = if d.day() == 1 {
            "1er".to_owned()
        } else {
            d.day().to_string()
        };
        format!(
            "{day} {} {} ({})",
            n.month(d.month()),
            d.year(),
            n.weekday(d)
        )
    } else {
        format!(
            "{} {}, {} ({})",
            n.month(d.month()),
            d.day(),
            d.year(),
            n.weekday(d)
        )
    }
}

fn numeric_date_text(d: Date, order: DateOrder) -> String {
    match order {
        DateOrder::Mdy => format!("{:02}/{:02}/{:04}", d.month(), d.day(), d.year()),
        DateOrder::Ymd => iso(d),
        DateOrder::Dmy => format!("{:02}/{:02}/{:04}", d.day(), d.month(), d.year()),
    }
}

fn iso(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

fn instant_text(z: &ZonedDateTime, cx: &Context<'_>) -> String {
    let d = z.date();
    let t = clock(
        i64::from(z.time().hour()),
        i64::from(z.time().minute()),
        cx.french,
    );
    let n = cx.names;
    if cx.french {
        let day = if d.day() == 1 {
            "1er".to_owned()
        } else {
            d.day().to_string()
        };
        format!(
            "{day} {} {} à {t} ({})",
            n.month(d.month()),
            d.year(),
            n.weekday(d)
        )
    } else {
        format!(
            "{} {}, {}, {t} ({})",
            n.month(d.month()),
            d.day(),
            d.year(),
            n.weekday(d)
        )
    }
}

fn answer(display: String, raw: String) -> Answer {
    Answer {
        display,
        raw,
        read_as: None,
    }
}

fn count_text(n: i64, unit: &(String, String), french: bool) -> String {
    let plural = if french { n.abs() >= 2 } else { n.abs() != 1 };
    format!("{n} {}", if plural { &unit.1 } else { &unit.0 })
}

/// Results of a value: main and variants.
fn show(v: &Value, cx: &Context<'_>) -> Option<(Answer, Vec<Answer>)> {
    Some(match v {
        Value::Day(d) => (
            answer(date_text(*d, cx), iso(*d)),
            vec![
                answer(
                    numeric_date_text(*d, cx.order),
                    numeric_date_text(*d, cx.order),
                ),
                answer(iso(*d), iso(*d)),
            ],
        ),
        Value::Instant(z) => {
            let local = z.with_time_zone(cx.now.time_zone().clone());
            let iso_text = format!(
                "{} {:02}:{:02}",
                iso(local.date()),
                local.time().hour(),
                local.time().minute()
            );
            (
                answer(instant_text(&local, cx), iso_text.clone()),
                vec![answer(iso_text.clone(), iso_text)],
            )
        }
        Value::Dur(d) if d.months == 0 && d.seconds == 0.0 => {
            let main = count_text(d.days, &cx.names.day, cx.french);
            let mut variants = Vec::new();
            if d.days.abs() >= 14 {
                #[allow(clippy::cast_precision_loss)]
                let weeks = d.days as f64 / 7.0;
                let text = super::format::decimal(weeks, 2, cx.french);
                variants.push(answer(
                    format!("{text} {}", cx.names.week.1),
                    text.replace(',', "."),
                ));
            }
            (answer(main, d.days.to_string()), variants)
        }
        Value::Dur(d) if d.months == 0 => {
            #[allow(clippy::cast_precision_loss)]
            let seconds = d.seconds + d.days as f64 * 86_400.0;
            let main = super::format::duration(seconds, cx.french);
            (
                answer(main, seconds.to_string()),
                super::format::duration_variants(seconds, cx.french),
            )
        }
        _ => return None,
    })
}

/// Evaluates the pieces; `None` when they are not for this evaluator,
/// `Some(None)` when they are but make no sense.
pub fn evaluate(pieces: &[Piece], cx: &Context<'_>) -> Option<Option<(Answer, Vec<Answer>)>> {
    if !claims(pieces) {
        return None;
    }
    Some(evaluate_claimed(pieces, cx))
}

fn evaluate_claimed(pieces: &[Piece], cx: &Context<'_>) -> Option<(Answer, Vec<Answer>)> {
    let mut parser = Parser {
        p: pieces.to_vec(),
        i: 0,
        cx,
    };
    let now = cx.now.clone();

    // `heure à Tokyo` / `time in Tokyo`.
    if parser.sense("time_in") {
        let (tz, city, end) = zone(pieces, 1)?;
        if end != pieces.len() {
            return None;
        }
        let there = now.with_time_zone(tz);
        return Some(zone_answer(&there, &city, cx));
    }
    // `15h Tokyo en Paris`.
    if let Some(r) = zone_conversion(&mut parser) {
        return r;
    }
    // `jours jusqu'au 25 décembre`, `weeks since January 1`.
    let unit = match parser.peek().cloned() {
        Some(Piece::Unit(u)) if matches!(parser.p.get(1), Some(Piece::Sense(s)) if s == "until" || s == "since") =>
        {
            parser.i += 1;
            Some(u)
        }
        _ => None,
    };
    if parser.sense("until") || parser.sense("since") {
        let until = parser.sense("until");
        parser.i += 1;
        let today = parser.today();
        let pick = move |current: Date, other: Date| {
            // Without a year: the next occurrence (until) or the last one (since).
            let (future, past) = if current >= today {
                (current, other.min(current))
            } else {
                (other.max(current), current)
            };
            if until { future } else { past }
        };
        let date = parser.date(&pick)?;
        if parser.i != pieces.len() {
            return None;
        }
        let days = if until {
            today.until((Unit::Day, date))
        } else {
            date.until((Unit::Day, today))
        }
        .ok()?
        .get_days();
        let days = i64::from(days);
        return Some(match unit.as_deref().and_then(time_unit) {
            Some(Dur { days: 7, .. }) => {
                #[allow(clippy::cast_precision_loss)]
                let weeks = days as f64 / 7.0;
                let text = super::format::decimal(weeks, 2, cx.french);
                let plural = if cx.french {
                    weeks.abs() >= 2.0
                } else {
                    (weeks - 1.0).abs() > f64::EPSILON
                };
                let name = if plural {
                    &cx.names.week.1
                } else {
                    &cx.names.week.0
                };
                (
                    answer(format!("{text} {name}"), text.replace(',', ".")),
                    vec![answer(
                        count_text(days, &cx.names.day, cx.french),
                        days.to_string(),
                    )],
                )
            }
            _ => (
                answer(count_text(days, &cx.names.day, cx.french), days.to_string()),
                Vec::new(),
            ),
        });
    }
    // `quel jour 14 juillet 1789` / `what day July 14 1789`.
    if parser.sense("what_day") {
        parser.i += 1;
        let pick_current = |current: Date, _other: Date| current;
        let date = parser.date(&pick_current)?;
        if parser.i != pieces.len() {
            return None;
        }
        let (_, variants) = show(&Value::Day(date), cx)?;
        let n = cx.names;
        let main = if cx.french {
            let day = if date.day() == 1 {
                "1er".to_owned()
            } else {
                date.day().to_string()
            };
            format!(
                "{} {day} {} {}",
                n.weekday(date),
                n.month(date.month()),
                date.year()
            )
        } else {
            format!(
                "{}, {} {}, {}",
                n.weekday(date),
                n.month(date.month()),
                date.day(),
                date.year()
            )
        };
        return Some((answer(main, iso(date)), variants));
    }
    // `dans 90 min` / `in 90 min`.
    let relative = parser.sense("relative_in");
    let ago_first = parser.sense("ago");
    if relative || ago_first {
        parser.i += 1;
    }
    let mut value = parser.sum()?;
    let ago_last = parser.sense("ago");
    if ago_last {
        parser.i += 1;
    }
    if relative || ago_first || ago_last {
        let Value::Dur(d) = value else { return None };
        let d = if relative { d } else { negate(d) };
        value = Value::Instant(add_to_instant(&now, d)?);
    }
    // Conversion: `en unix`, `en jours`.
    if parser.peek() == Some(&Piece::Convert) {
        parser.i += 1;
        let target = parser.peek()?.clone();
        parser.i += 1;
        if parser.i != pieces.len() {
            return None;
        }
        return convert(&value, &target, cx);
    }
    if parser.i != pieces.len() {
        return None;
    }
    if let Value::Instant(z) = &value
        && pieces.first() == Some(&Piece::Sym("@".into()))
    {
        return Some(unix_answer(z, cx));
    }
    show(&value, cx)
}

fn convert(value: &Value, target: &Piece, cx: &Context<'_>) -> Option<(Answer, Vec<Answer>)> {
    match (value, target) {
        (Value::Instant(z), Piece::Sense(s)) if s == "unix" => {
            let s = z.timestamp().as_second().to_string();
            Some((answer(s.clone(), s), Vec::new()))
        }
        (Value::Day(d), Piece::Sense(s)) if s == "unix" => {
            let z = d.to_zoned_datetime(cx.now.time_zone().clone()).ok()?;
            let s = z.timestamp().as_second().to_string();
            Some((answer(s.clone(), s), Vec::new()))
        }
        (Value::Dur(d), Piece::Unit(u)) if d.months == 0 => {
            let unit = time_unit(u)?;
            #[allow(clippy::cast_precision_loss)]
            let seconds = d.seconds + d.days as f64 * 86_400.0;
            #[allow(clippy::cast_precision_loss)]
            let per = unit.seconds + unit.days as f64 * 86_400.0;
            if unit.months != 0 || per == 0.0 {
                return None;
            }
            let n = seconds / per;
            let text = super::format::decimal(n, 4, cx.french);
            let name = match (unit.days, u.as_str()) {
                (1, _) => {
                    if n.abs() < 2.0 && cx.french || (n - 1.0).abs() < f64::EPSILON {
                        &cx.names.day.0
                    } else {
                        &cx.names.day.1
                    }
                }
                (7, _) => {
                    if n.abs() < 2.0 && cx.french || (n - 1.0).abs() < f64::EPSILON {
                        &cx.names.week.0
                    } else {
                        &cx.names.week.1
                    }
                }
                _ => u,
            };
            Some((
                answer(format!("{text} {name}"), text.replace(',', ".")),
                Vec::new(),
            ))
        }
        (Value::Instant(z), Piece::Zone(_) | Piece::Value(_)) => {
            let (tz, city, _) = zone(std::slice::from_ref(target), 0)?;
            Some(zone_answer(&z.with_time_zone(tz), &city, cx))
        }
        _ => None,
    }
}

/// `15h Tokyo en Paris`: a time in one zone, converted to another.
fn zone_conversion(parser: &mut Parser<'_>) -> Option<Option<(Answer, Vec<Answer>)>> {
    let p = parser.p.clone();
    let start = parser.i;
    let time = parser.time();
    let Some(time) = time else {
        parser.i = start;
        return None;
    };
    let Some((from, _, next)) = zone(&p, parser.i) else {
        parser.i = start;
        return None;
    };
    if p.get(next) != Some(&Piece::Convert) {
        parser.i = start;
        return None;
    }
    let cx = parser.cx;
    Some((|| {
        let (to, city, end) = zone(&p, next + 1)?;
        if end != p.len() {
            return None;
        }
        let day = cx.now.with_time_zone(from.clone()).date();
        let there = day.to_datetime(time).to_zoned_datetime(from).ok()?;
        let converted = there.with_time_zone(to);
        let t = clock(
            i64::from(converted.time().hour()),
            i64::from(converted.time().minute()),
            cx.french,
        );
        let shift = converted
            .date()
            .until((Unit::Day, there.date()))
            .ok()?
            .get_days();
        let label = match shift {
            0 => String::new(),
            n if n < 0 => format!(" ({})", cx.names.next_day),
            _ => format!(" ({})", cx.names.previous_day),
        };
        let raw = format!(
            "{:02}:{:02}",
            converted.time().hour(),
            converted.time().minute()
        );
        Some((
            answer(format!("{t}{label}"), raw),
            vec![answer(
                format!("{city}: {}", instant_text(&converted, cx)),
                String::new(),
            )],
        ))
    })())
}

fn zone_answer(there: &ZonedDateTime, city: &str, cx: &Context<'_>) -> (Answer, Vec<Answer>) {
    let t = clock(
        i64::from(there.time().hour()),
        i64::from(there.time().minute()),
        cx.french,
    );
    let d = there.date();
    let n = cx.names;
    let day = if cx.french {
        format!("{} {} {}", n.weekday(d), d.day(), n.month(d.month()))
    } else {
        format!("{}, {} {}", n.weekday(d), n.month(d.month()), d.day())
    };
    let raw = format!("{:02}:{:02}", there.time().hour(), there.time().minute());
    (
        answer(format!("{t} ({day})"), raw),
        vec![answer(
            format!("{city}: {}", instant_text(there, cx)),
            String::new(),
        )],
    )
}

/// `@1700000000` → `14/11/2023 22:13:20 UTC`, then the local time.
fn unix_answer(z: &ZonedDateTime, cx: &Context<'_>) -> (Answer, Vec<Answer>) {
    let utc = z.with_time_zone(TimeZone::UTC);
    let t = utc.time();
    let main = format!(
        "{} {:02}:{:02}:{:02} UTC",
        numeric_date_text(utc.date(), cx.order),
        t.hour(),
        t.minute(),
        t.second()
    );
    let local = z.with_time_zone(cx.now.time_zone().clone());
    let raw = format!(
        "{}T{:02}:{:02}:{:02}Z",
        iso(utc.date()),
        t.hour(),
        t.minute(),
        t.second()
    );
    (
        answer(main, raw),
        vec![answer(
            format!("{} ({})", instant_text(&local, cx), cx.names.local_time),
            String::new(),
        )],
    )
}

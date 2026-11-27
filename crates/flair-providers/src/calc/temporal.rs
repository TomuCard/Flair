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
    pub month: (String, String),
    pub year: (String, String),
    pub hour: (String, String),
    pub minute: (String, String),
    pub second: (String, String),
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

/// What `until`, `since` and `age` count to.
#[derive(Debug, Clone)]
enum Target {
    Day(Date),
    Instant(ZonedDateTime),
    /// A year alone (`depuis 1999`).
    Year(i16),
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
        Piece::Month(_) | Piece::Weekday(_) | Piece::Zone(..) => true,
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

fn is_sense(p: Option<&Piece>, sense: &str) -> bool {
    matches!(p, Some(Piece::Sense(s)) if s == sense)
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
        return Some((ymd(y, m, d)?, i + 5));
    }
    None
}

fn ymd(y: i64, m: i64, d: i64) -> Option<Date> {
    Date::new(
        i16::try_from(y).ok()?,
        i8::try_from(m).ok()?,
        i8::try_from(d).ok()?,
    )
    .ok()
}

/// Easter Sunday of a year (anonymous Gregorian algorithm).
fn easter(year: i64) -> Option<Date> {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    ymd(year, month, day)
}

/// Chooses between a day of this year and the same day of another year
/// (`25 décembre` after `until`: the next one).
type Pick<'a> = &'a dyn Fn(Date, Date) -> Date;

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
        is_sense(self.peek(), s)
    }

    /// Skips the sense if it comes next.
    fn skip(&mut self, s: &str) -> bool {
        let found = self.sense(s);
        if found {
            self.i += 1;
        }
        found
    }

    fn at_end(&self) -> bool {
        self.i == self.p.len()
    }

    fn today(&self) -> Date {
        self.cx.now.date()
    }

    /// A day of the year without a year, in the year chosen by `pick`.
    fn in_some_year(&self, make: &dyn Fn(i64) -> Option<Date>, pick: Pick<'_>) -> Option<Date> {
        let year = i64::from(self.today().year());
        let current = make(year)?;
        let next = make(year + 1).unwrap_or(current);
        let previous = make(year - 1).unwrap_or(current);
        let other = if pick(current, next) == next && next != current {
            next
        } else {
            previous
        };
        Some(pick(current, other))
    }

    /// A date written in any supported way, holidays and weekdays included;
    /// without a year, the year is chosen by `pick`.
    fn date(&mut self, pick: Pick<'_>) -> Option<Date> {
        let start = self.i;
        if let Some(date) = self.written_date(pick) {
            return Some(date);
        }
        self.i = start;
        if let Some(date) = self.weekday(pick) {
            return Some(date);
        }
        self.i = start;
        None
    }

    fn written_date(&mut self, pick: Pick<'_>) -> Option<Date> {
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
        // `noël`, `Easter`.
        if let Some(Piece::Sense(s)) = p.get(i)
            && let Some(holiday) = s.strip_prefix("holiday:")
        {
            self.i = i + 1;
            if holiday == "easter" {
                return self.in_some_year(&easter, pick);
            }
            let (m, d) = holiday.split_once('-')?;
            let (m, d): (i64, i64) = (m.parse().ok()?, d.parse().ok()?);
            return self.in_some_year(&|y| ymd(y, m, d), pick);
        }
        let year_at = |k: usize| {
            int(p.get(k)).filter(|y| {
                (100..=9999).contains(y) && number(p.get(k)).is_some_and(|n| n.len() >= 3)
            })
        };
        let build = |parser: &Self, y: Option<i64>, m: u8, d: i64| match y {
            Some(y) => ymd(y, i64::from(m), d),
            None => parser.in_some_year(&|y| ymd(y, i64::from(m), d), pick),
        };
        // `14 juillet 1789`, `1er janvier`.
        if let (Some(d), Some(Piece::Month(m))) = (int(p.get(i)), p.get(i + 1)) {
            let y = year_at(i + 2);
            self.i = i + 2 + usize::from(y.is_some());
            return build(self, y, *m, d);
        }
        if let Some(Piece::Month(m)) = p.get(i) {
            let m = *m;
            // `janvier 2020`: its first day.
            if let Some(y) = year_at(i + 1) {
                self.i = i + 2;
                return ymd(y, i64::from(m), 1);
            }
            // `July 14 1789`, `December 25`.
            if let Some(d) = int(p.get(i + 1)) {
                let y = year_at(i + 2);
                self.i = i + 2 + usize::from(y.is_some());
                return build(self, y, m, d);
            }
            // `juin` alone: its first day.
            self.i = i + 1;
            return build(self, None, m, 1);
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
            return build(self, None, u8::try_from(m).ok()?, d);
        }
        None
    }

    /// `vendredi`, `lundi prochain`, `next Monday`, `last Friday`.
    fn weekday(&mut self, pick: Pick<'_>) -> Option<Date> {
        let mut next = self.skip("next");
        let mut last = !next && self.skip("last");
        let Some(Piece::Weekday(d)) = self.peek() else {
            return None;
        };
        let d = i64::from(*d);
        self.i += 1;
        if !next && !last {
            next = self.skip("next");
            last = !next && self.skip("last");
        }
        let today = self.today();
        let wd = i64::from(today.weekday().to_monday_one_offset());
        let ahead = match (d - wd).rem_euclid(7) {
            0 => 7,
            n => n,
        };
        let behind = match (wd - d).rem_euclid(7) {
            0 => 7,
            n => n,
        };
        let after = today.checked_add(Span::new().try_days(ahead).ok()?).ok()?;
        let before = today.checked_sub(Span::new().try_days(behind).ok()?).ok()?;
        Some(if next {
            after
        } else if last {
            before
        } else {
            pick(after, before)
        })
    }

    /// `14h30`, `14:30`, `14h` (`3pm` and `midi` arrive as `15:00`, `12:00`).
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

    /// A time of day after `à` / `at`, or right away (`demain 14h`).
    fn at_time(&mut self) -> Option<Time> {
        let start = self.i;
        self.skip("at");
        let time = self.time();
        if time.is_none() {
            self.i = start;
        }
        time
    }

    /// A day, or an instant when a time follows (`demain à 14h`).
    fn day_value(&mut self, date: Date) -> Option<Value> {
        match self.at_time() {
            Some(time) => Some(Value::Instant(self.local(date, time)?)),
            None => Some(Value::Day(date)),
        }
    }

    fn local(&self, date: Date, time: Time) -> Option<ZonedDateTime> {
        date.to_datetime(time)
            .to_zoned_datetime(self.cx.now.time_zone().clone())
            .ok()
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
            Piece::Sense(s) if s == "now" => {
                self.i += 1;
                Some(Value::Instant(now))
            }
            Piece::Sense(s) if matches!(s.as_str(), "today" | "tomorrow" | "yesterday") => {
                self.i += 1;
                let day = match s.as_str() {
                    "today" => today,
                    "tomorrow" => today.tomorrow().ok()?,
                    _ => today.yesterday().ok()?,
                };
                self.day_value(day)
            }
            Piece::Sym(s) if s == "@" => {
                self.i += 1;
                self.timestamp()
            }
            Piece::Sense(s) if s == "unix_at" => {
                self.i += 1;
                self.timestamp()
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
                // Without a year: the next occurrence (`Noël + 2 jours`).
                let upcoming = |current: Date, other: Date| {
                    if current >= today {
                        current
                    } else {
                        other.max(current)
                    }
                };
                if let Some(date) = self.date(&upcoming) {
                    // A time after the date: `26/11/2026 14h30`.
                    return self.day_value(date);
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

    /// The seconds of a Unix timestamp.
    fn timestamp(&mut self) -> Option<Value> {
        let seconds = int(self.peek())?;
        self.i += 1;
        Some(Value::Instant(
            Timestamp::from_second(seconds)
                .ok()?
                .to_zoned_datetime(TimeZone::UTC),
        ))
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

    /// What `until` / `since` count to: `25 décembre`, `1999`, `18h`,
    /// `vendredi`, `demain à 9h`.
    fn target(&mut self, until: bool) -> Option<Target> {
        let now = self.cx.now.clone();
        let today = self.today();
        if self.skip("now") {
            return Some(Target::Instant(now));
        }
        for (sense, day) in [
            ("today", Some(today)),
            ("tomorrow", today.tomorrow().ok()),
            ("yesterday", today.yesterday().ok()),
        ] {
            if self.skip(sense) {
                let day = day?;
                return Some(match self.at_time() {
                    Some(time) => Target::Instant(self.local(day, time)?),
                    None => Target::Day(day),
                });
            }
        }
        // A year alone.
        let year = number(self.peek())
            .filter(|n| n.len() == 4 && self.i + 1 == self.p.len())
            .and_then(|n| n.parse().ok());
        if let Some(year) = year {
            self.i += 1;
            return Some(Target::Year(year));
        }
        let pick = move |current: Date, other: Date| {
            // Without a year: the next occurrence (until) or the last one (since).
            let (future, past) = if current > today {
                (current, other.min(current))
            } else if current == today {
                (current, current)
            } else {
                (other.max(current), current)
            };
            if until { future } else { past }
        };
        if let Some(date) = self.date(&pick) {
            return Some(match self.at_time() {
                Some(time) => Target::Instant(self.local(date, time)?),
                None => Target::Day(date),
            });
        }
        // A time alone: today, else the next (until) or last (since) one.
        let time = self.time()?;
        let mut at = self.local(today, time)?;
        if until && at <= now {
            at = at.checked_add(Span::new().days(1)).ok()?;
        } else if !until && at > now {
            at = at.checked_sub(Span::new().days(1)).ok()?;
        }
        Some(Target::Instant(at))
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
    let day_plus = |date: Date, d: Dur| -> Option<Value> {
        if d.seconds == 0.0 {
            Some(Value::Day(add_to_date(date, d)?))
        } else {
            let z = date.to_zoned_datetime(TimeZone::system()).ok()?;
            Some(Value::Instant(add_to_instant(&z, d)?))
        }
    };
    Some(match (left, right) {
        (Value::Day(date), Value::Dur(d)) => day_plus(date, signed(d))?,
        (Value::Instant(z), Value::Dur(d)) => Value::Instant(add_to_instant(&z, signed(d))?),
        // `5 jours + maintenant`: addition commutes.
        (Value::Dur(d), Value::Day(date)) if !minus => day_plus(date, d)?,
        (Value::Dur(d), Value::Instant(z)) if !minus => Value::Instant(add_to_instant(&z, d)?),
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
        (Value::Instant(a), Value::Day(b)) if minus => {
            let b = b.to_zoned_datetime(a.time_zone().clone()).ok()?;
            let seconds = a.timestamp().duration_since(b.timestamp()).as_secs_f64();
            Value::Dur(Dur {
                seconds,
                ..Dur::default()
            })
        }
        (Value::Day(a), Value::Instant(b)) if minus => {
            let a = a.to_zoned_datetime(b.time_zone().clone()).ok()?;
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

/// A time zone from a city (`Tokyo`, `New York`) or a lexicon zone, with
/// the name to show and the index after it.
fn zone(p: &[Piece], i: usize) -> Option<(TimeZone, String, usize)> {
    if let Some(Piece::Zone(name, typed)) = p.get(i) {
        return Some((TimeZone::get(name).ok()?, zone_label(name, typed), i + 1));
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
            return Some((TimeZone::get(name).ok()?, city_of(name), i + words.len()));
        }
        words.pop();
    }
    None
}

fn city_of(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).replace('_', " ")
}

/// `Londres` as typed, `UTC` and `PST` in capitals, `japon` → `Japon`.
fn zone_label(name: &str, typed: &str) -> String {
    let city = city_of(name);
    if flair_core::lexicon::fold(typed) == flair_core::lexicon::fold(&city) {
        return city;
    }
    if typed.chars().count() <= 4 && typed.chars().all(|c| c.is_ascii_alphabetic()) {
        return typed.to_uppercase();
    }
    let mut out = String::with_capacity(typed.len());
    let mut start = true;
    for c in typed.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c == ' ' || c == '-';
    }
    out
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

fn clock_of(z: &ZonedDateTime, french: bool) -> String {
    clock(
        i64::from(z.time().hour()),
        i64::from(z.time().minute()),
        french,
    )
}

fn day_number(d: Date, french: bool) -> String {
    if french && d.day() == 1 {
        "1er".to_owned()
    } else {
        d.day().to_string()
    }
}

fn date_text(d: Date, cx: &Context<'_>) -> String {
    let n = cx.names;
    if cx.french {
        format!(
            "{} {} {} ({})",
            day_number(d, true),
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
    let t = clock_of(z, cx.french);
    let n = cx.names;
    if cx.french {
        format!(
            "{} {} {} à {t} ({})",
            day_number(d, true),
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

fn plural(n: f64, french: bool) -> bool {
    if french {
        n.abs() >= 2.0
    } else {
        (n.abs() - 1.0).abs() > f64::EPSILON
    }
}

fn count_text(n: i64, unit: &(String, String), french: bool) -> String {
    #[allow(clippy::cast_precision_loss)]
    let many = plural(n as f64, french);
    format!("{n} {}", if many { &unit.1 } else { &unit.0 })
}

/// `5,14 semaines`, with its raw value.
fn amount(n: f64, decimals: usize, unit: &(String, String), french: bool) -> Answer {
    let text = super::format::decimal(n, decimals, french);
    let name = if plural(n, french) { &unit.1 } else { &unit.0 };
    answer(format!("{text} {name}"), text.replace(',', "."))
}

/// `27 ans, 10 mois, 12 jours`: the calendar between two days.
fn breakdown(a: Date, b: Date, cx: &Context<'_>) -> Option<Answer> {
    let span = a.until((Unit::Year, b)).ok()?;
    let parts: Vec<String> = [
        (i64::from(span.get_years()), &cx.names.year),
        (i64::from(span.get_months()), &cx.names.month),
        (i64::from(span.get_days()), &cx.names.day),
    ]
    .iter()
    .filter(|(n, _)| *n != 0)
    .map(|(n, unit)| count_text(*n, unit, cx.french))
    .collect();
    (parts.len() > 1).then(|| answer(parts.join(", "), String::new()))
}

fn days_answer(days: i64, cx: &Context<'_>) -> Answer {
    answer(count_text(days, &cx.names.day, cx.french), days.to_string())
}

fn weeks_answer(days: i64, cx: &Context<'_>) -> Answer {
    #[allow(clippy::cast_precision_loss)]
    let weeks = days as f64 / 7.0;
    amount(weeks, 2, &cx.names.week, cx.french)
}

fn duration_answers(seconds: f64, french: bool) -> (Answer, Vec<Answer>) {
    (
        answer(
            super::format::duration(seconds, french),
            super::format::decimal(seconds, 3, false),
        ),
        super::format::duration_variants(seconds, french),
    )
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
            let mut variants = Vec::new();
            if d.days.abs() >= 14 {
                variants.push(weeks_answer(d.days, cx));
            }
            (days_answer(d.days, cx), variants)
        }
        Value::Dur(d) if d.months == 0 => {
            #[allow(clippy::cast_precision_loss)]
            let seconds = d.seconds + d.days as f64 * 86_400.0;
            duration_answers(seconds, cx.french)
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

    // `heure à Tokyo`, `15h Tokyo en Paris`, `now in UTC`.
    if let Some(r) = zones(&mut parser) {
        return r;
    }
    // `de 9h à 17h30`, `from 9am to 5pm`.
    if let Some(r) = time_range(&mut parser) {
        return r;
    }
    // `jours jusqu'au 25 décembre`, `années depuis 1999`, `âge 14/07/1990`.
    let unit = match parser.peek().cloned() {
        Some(Piece::Unit(u))
            if is_sense(parser.p.get(1), "until") || is_sense(parser.p.get(1), "since") =>
        {
            parser.i += 1;
            Some(u)
        }
        _ => None,
    };
    let age = unit.is_none() && parser.skip("age");
    if age || parser.sense("until") || parser.sense("since") {
        let until = !age && parser.sense("until");
        if !age {
            parser.i += 1;
        }
        let target = parser.target(until)?;
        if !parser.at_end() {
            return None;
        }
        let unit = unit.or_else(|| age.then(|| "year".to_owned()));
        return count(&target, unit.as_deref(), until, cx);
    }
    // `quel jour 14 juillet 1789` / `what day is Christmas`.
    if parser.sense("what_day") {
        parser.i += 1;
        let today = parser.today();
        let upcoming = |current: Date, other: Date| {
            if current >= today {
                current
            } else {
                other.max(current)
            }
        };
        let date = parser.date(&upcoming)?;
        if !parser.at_end() {
            return None;
        }
        let (_, variants) = show(&Value::Day(date), cx)?;
        let n = cx.names;
        let main = if cx.french {
            format!(
                "{} {} {} {}",
                n.weekday(date),
                day_number(date, true),
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
    // `dans 90 min` / `in 90 min`, `il y a 2h`, `2 weeks from now`.
    let relative = parser.skip("relative_in");
    let ago_first = !relative && parser.skip("ago");
    let mut value = parser.sum()?;
    let ago_last = parser.skip("ago");
    let later = parser.skip("from_now");
    if relative || later || ago_first || ago_last {
        let Value::Dur(d) = value else { return None };
        let d = if relative || later { d } else { negate(d) };
        value = Value::Instant(add_to_instant(&now, d)?);
    }
    // `dans 3 jours à 14h`: the time of that day.
    if let Some(time) = parser.at_time() {
        let date = match &value {
            Value::Instant(z) => z.with_time_zone(now.time_zone().clone()).date(),
            Value::Day(d) => *d,
            _ => return None,
        };
        value = Value::Instant(parser.local(date, time)?);
    }
    // Conversion: `en unix`, `en jours`, `en Tokyo`.
    if parser.peek() == Some(&Piece::Convert) {
        parser.i += 1;
        let target = parser.peek()?.clone();
        if let Value::Instant(z) = &value
            && let Some((tz, city, end)) = zone(&parser.p, parser.i)
        {
            if end != pieces.len() {
                return None;
            }
            return Some(zone_answer(&z.with_time_zone(tz), &city, cx));
        }
        parser.i += 1;
        if !parser.at_end() {
            return None;
        }
        return convert(&value, &target, cx);
    }
    if !parser.at_end() {
        return None;
    }
    // `Mars 2020`, `juin` alone: names more often than dates.
    if matches!(pieces.first(), Some(Piece::Month(_))) && pieces.len() <= 2 {
        return None;
    }
    if let Value::Instant(z) = &value
        && (pieces.first() == Some(&Piece::Sym("@".into())) || is_sense(pieces.first(), "unix_at"))
    {
        return Some(unix_answer(z, cx));
    }
    show(&value, cx)
}

/// `jours jusqu'au 25 décembre`, `années depuis 1999`, `heures jusqu'à 18h`.
fn count(
    target: &Target,
    unit: Option<&str>,
    until: bool,
    cx: &Context<'_>,
) -> Option<(Answer, Vec<Answer>)> {
    let now = &cx.now;
    let today = now.date();
    let tz = now.time_zone().clone();
    let (day, instant) = match target {
        Target::Day(d) => (*d, d.to_zoned_datetime(tz).ok()?),
        Target::Instant(z) => (z.date(), z.clone()),
        Target::Year(y) => {
            let d = Date::new(*y, 1, 1).ok()?;
            (d, d.to_zoned_datetime(tz).ok()?)
        }
    };
    // From the earlier to the later one: positive when the order is right.
    let (from, to) = if until { (today, day) } else { (day, today) };
    let days = i64::from(from.until((Unit::Day, to)).ok()?.get_days());
    let seconds = if until {
        instant.timestamp().duration_since(now.timestamp())
    } else {
        now.timestamp().duration_since(instant.timestamp())
    }
    .as_secs_f64();
    let years = || -> Option<i64> {
        Some(match target {
            Target::Year(y) => {
                let diff = i64::from(today.year()) - i64::from(*y);
                if until { -diff } else { diff }
            }
            _ => i64::from(from.until((Unit::Year, to)).ok()?.get_years()),
        })
    };
    let unit = unit.map(|u| (u, time_unit(u)));
    let n = cx.names;
    Some(match unit {
        Some((_, Some(Dur { months: 12, .. }))) => {
            let mut variants = Vec::new();
            if !matches!(target, Target::Year(_)) {
                variants.extend(breakdown(from, to, cx));
            }
            variants.push(days_answer(days, cx));
            let y = years()?;
            (
                answer(count_text(y, &n.year, cx.french), y.to_string()),
                variants,
            )
        }
        Some((_, Some(Dur { months: 1, .. }))) => {
            let months = i64::from(from.until((Unit::Month, to)).ok()?.get_months());
            (
                answer(count_text(months, &n.month, cx.french), months.to_string()),
                vec![days_answer(days, cx)],
            )
        }
        Some((_, Some(Dur { days: 7, .. }))) => {
            let weeks = if matches!(target, Target::Instant(_)) {
                amount(seconds / 604_800.0, 2, &n.week, cx.french)
            } else {
                weeks_answer(days, cx)
            };
            (weeks, vec![days_answer(days, cx)])
        }
        Some((_, Some(Dur { days: 1, .. }))) => {
            if matches!(target, Target::Instant(_)) {
                (amount(seconds / 86_400.0, 2, &n.day, cx.french), Vec::new())
            } else {
                (days_answer(days, cx), Vec::new())
            }
        }
        Some((_, Some(Dur { seconds: per, .. }))) if per > 0.0 => {
            let name = if per >= 3600.0 {
                &n.hour
            } else if per >= 60.0 {
                &n.minute
            } else {
                &n.second
            };
            let decimals = if per >= 60.0 { 2 } else { 0 };
            let (duration, mut variants) = duration_answers(seconds, cx.french);
            variants.insert(0, duration);
            (amount(seconds / per, decimals, name, cx.french), variants)
        }
        Some(_) => return None,
        None => match target {
            Target::Instant(_) => duration_answers(seconds, cx.french),
            Target::Year(_) => {
                let y = years()?;
                (
                    answer(count_text(y, &n.year, cx.french), y.to_string()),
                    vec![days_answer(days, cx)],
                )
            }
            Target::Day(_) => {
                let mut variants = Vec::new();
                if days.abs() >= 14 {
                    variants.push(weeks_answer(days, cx));
                }
                if days.abs() >= 60 {
                    variants.extend(breakdown(from, to, cx));
                }
                (days_answer(days, cx), variants)
            }
        },
    })
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
            let names = cx.names;
            let name = match unit.days {
                1 => Some(&names.day),
                7 => Some(&names.week),
                _ if per == 3600.0 => Some(&names.hour),
                _ if per == 60.0 => Some(&names.minute),
                _ if per == 1.0 => Some(&names.second),
                _ => None,
            };
            Some((
                match name {
                    Some(name) => amount(n, 4, name, cx.french),
                    None => {
                        let text = super::format::decimal(n, 4, cx.french);
                        answer(format!("{text} {u}"), text.replace(',', "."))
                    }
                },
                Vec::new(),
            ))
        }
        _ => None,
    }
}

/// The start of a time zone form.
enum ZoneStart {
    Now,
    Clock(Time),
}

/// `heure à Tokyo [en Paris]`, `now Tokyo to Paris`, `15h Tokyo en Paris`,
/// `3pm in Tokyo`, `15h Tokyo` (to the local time).
fn zones(parser: &mut Parser<'_>) -> Option<Option<(Answer, Vec<Answer>)>> {
    let p = parser.p.clone();
    let n = p.len();
    let link = |k: usize| matches!(p.get(k), Some(Piece::Convert)) || is_sense(p.get(k), "at");
    let (start, mut i) = if is_sense(p.first(), "time_in") {
        (ZoneStart::Now, 1)
    } else if matches!(p.first(), Some(Piece::Unit(u)) if u == "hour" || u == "hours") {
        // `heure Tokyo`, `heure en Chine`.
        let k = if link(1) { 2 } else { 1 };
        zone(&p, k)?;
        (ZoneStart::Now, k)
    } else if is_sense(p.first(), "now") {
        (ZoneStart::Now, 1)
    } else {
        parser.i = 0;
        let time = parser.time();
        let after = parser.i;
        parser.i = 0;
        (ZoneStart::Clock(time?), after)
    };
    // `now in Tokyo to Paris`: the first `in` only links.
    let linked_zone =
        zone(&p, i + 1).is_some_and(|(_, _, end)| end < n && p.get(end) == Some(&Piece::Convert));
    if is_sense(p.get(i), "at") || (p.get(i) == Some(&Piece::Convert) && linked_zone) {
        i += 1;
    }
    let from = zone(&p, i);
    if let Some((_, _, end)) = &from {
        i = *end;
    }
    let to = if p.get(i) == Some(&Piece::Convert) {
        match zone(&p, i + 1) {
            Some((tz, city, end)) if end == n => {
                i = n;
                Some((tz, city))
            }
            // `now to unix`, `15h en minutes`: not a time zone form.
            _ => return None,
        }
    } else {
        None
    };
    let from = from.map(|(tz, city, _)| (tz, city));
    if i != n || (from.is_none() && to.is_none()) {
        return None;
    }
    let cx = parser.cx;
    let now = &cx.now;
    Some(match start {
        ZoneStart::Now => {
            let (tz, city) = to.as_ref().or(from.as_ref())?;
            let there = now.with_time_zone(tz.clone());
            let (main, mut variants) = zone_answer(&there, city, cx);
            if let (Some((from_tz, from_city)), Some(_)) = (&from, &to) {
                let source = now.with_time_zone(from_tz.clone());
                variants.insert(
                    0,
                    answer(
                        format!(
                            "{from_city} {} → {city} {}",
                            clock_of(&source, cx.french),
                            clock_of(&there, cx.french)
                        ),
                        String::new(),
                    ),
                );
            }
            Some((main, variants))
        }
        ZoneStart::Clock(time) => {
            let local = (now.time_zone().clone(), cx.names.local_time.clone());
            let (from_tz, _) = from.unwrap_or_else(|| local.clone());
            let (to_tz, city) = to.unwrap_or(local);
            clock_conversion(time, &from_tz, to_tz, &city, cx)
        }
    })
}

/// A time of day in one zone, shown in another.
fn clock_conversion(
    time: Time,
    from: &TimeZone,
    to: TimeZone,
    city: &str,
    cx: &Context<'_>,
) -> Option<(Answer, Vec<Answer>)> {
    let day = cx.now.with_time_zone(from.clone()).date();
    let there = day.to_datetime(time).to_zoned_datetime(from.clone()).ok()?;
    let converted = there.with_time_zone(to);
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
        answer(format!("{}{label}", clock_of(&converted, cx.french)), raw),
        vec![answer(
            format!("{city}: {}", instant_text(&converted, cx)),
            String::new(),
        )],
    ))
}

/// `de 9h à 17h30`, `from 9am to 5pm`, `9h à 17h`: the time between.
fn time_range(parser: &mut Parser<'_>) -> Option<Option<(Answer, Vec<Answer>)>> {
    parser.i = 0;
    let from = parser.skip("from");
    let Some(start) = parser.time() else {
        parser.i = 0;
        return None;
    };
    let linked = parser.skip("at")
        || (from && (parser.peek() == Some(&Piece::Convert) || is_op(parser.peek(), "-")) && {
            parser.i += 1;
            true
        });
    let end = if linked { parser.time() } else { None };
    let (Some(end), true) = (end, parser.at_end()) else {
        parser.i = 0;
        return None;
    };
    let mut seconds = start.until(end).ok()?.total(Unit::Second).ok()?;
    if seconds < 0.0 {
        // `de 22h à 6h`: over midnight.
        seconds += 86_400.0;
    }
    Some(Some(duration_answers(seconds, parser.cx.french)))
}

fn zone_answer(there: &ZonedDateTime, city: &str, cx: &Context<'_>) -> (Answer, Vec<Answer>) {
    let d = there.date();
    let n = cx.names;
    let day = if cx.french {
        format!(
            "{} {} {}",
            n.weekday(d),
            day_number(d, true),
            n.month(d.month())
        )
    } else {
        format!("{}, {} {}", n.weekday(d), n.month(d.month()), d.day())
    };
    let raw = format!("{:02}:{:02}", there.time().hour(), there.time().minute());
    (
        answer(format!("{} ({day})", clock_of(there, cx.french)), raw),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easter_dates() {
        assert_eq!(easter(2026), ymd(2026, 4, 5));
        assert_eq!(easter(2027), ymd(2027, 3, 28));
        assert_eq!(easter(2000), ymd(2000, 4, 23));
    }

    #[test]
    fn zone_labels() {
        assert_eq!(zone_label("Europe/London", "Londres"), "Londres");
        assert_eq!(zone_label("UTC", "utc"), "UTC");
        assert_eq!(zone_label("America/Los_Angeles", "pst"), "PST");
        assert_eq!(zone_label("Asia/Tokyo", "japon"), "Japon");
        assert_eq!(zone_label("America/New_York", "new york"), "New York");
    }
}

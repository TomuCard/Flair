//! Formatting of results in the interface language: significant digits,
//! scientific notation for very large and very small numbers, decimal
//! separator, unit names.

use super::normalize::Vocabulary;

#[derive(Debug, Clone, Copy)]
pub struct Style {
    /// French conventions: `1,5`, unit names of the French lexicon.
    pub french: bool,
    /// Significant digits of approximate results.
    pub significant: usize,
}

/// Plain notation up to this many integer digits, scientific beyond.
const MAX_PLAIN_DIGITS: i64 = 15;
/// Plain notation down to 1e-6, scientific below.
const MIN_PLAIN_EXPONENT: i64 = -5;

/// A decimal number: `digits` without leading or trailing zeros, `point`
/// digits before the decimal point (may be ≤ 0 or > `digits.len()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    point: i64,
}

impl Decimal {
    /// Parses `-123.45`; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, body) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (int, frac) = body.split_once('.').unwrap_or((body, ""));
        if int.is_empty()
            || !int.chars().all(|c| c.is_ascii_digit())
            || !frac.chars().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let mut digits: Vec<u8> = int.bytes().chain(frac.bytes()).map(|b| b - b'0').collect();
        let mut point = i64::try_from(int.len()).ok()?;
        let leading = digits.iter().take_while(|d| **d == 0).count();
        digits.drain(..leading);
        point -= i64::try_from(leading).ok()?;
        while digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            point = 1;
        }
        Some(Self {
            negative: negative && !digits.is_empty(),
            digits,
            point,
        })
    }

    pub fn digit_count(&self) -> usize {
        self.digits.len()
    }

    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    pub fn is_integer(&self) -> bool {
        i64::try_from(self.digits.len()).is_ok_and(|len| len <= self.point)
    }

    /// Absolute value is exactly one.
    pub fn is_one(&self) -> bool {
        self.digits == [1] && self.point == 1
    }

    /// Absolute value below two (French singular).
    pub fn below_two(&self) -> bool {
        self.point <= 0 || (self.point == 1 && self.digits.first().is_some_and(|d| *d < 2))
    }

    /// Rounds to `n` significant digits (half up).
    pub fn round(&mut self, n: usize) {
        if self.digits.len() <= n {
            return;
        }
        let up = self.digits[n] >= 5;
        self.digits.truncate(n);
        if up {
            let mut i = n;
            loop {
                if i == 0 {
                    self.digits.insert(0, 1);
                    self.point += 1;
                    break;
                }
                i -= 1;
                if self.digits[i] == 9 {
                    self.digits[i] = 0;
                } else {
                    self.digits[i] += 1;
                    break;
                }
            }
        }
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
    }

    fn digit_string(digits: &[u8]) -> String {
        digits.iter().map(|d| char::from(b'0' + d)).collect()
    }

    /// `1234.5`, with the given decimal separator.
    pub fn plain(&self, separator: char) -> String {
        let mut out = String::new();
        if self.negative {
            out.push('-');
        }
        if self.is_zero() {
            out.push('0');
            return out;
        }
        let len = i64::try_from(self.digits.len()).unwrap_or(i64::MAX);
        if self.point <= 0 {
            out.push('0');
            out.push(separator);
            out.push_str(&"0".repeat(usize::try_from(-self.point).unwrap_or(0)));
            out.push_str(&Self::digit_string(&self.digits));
        } else if self.point >= len {
            out.push_str(&Self::digit_string(&self.digits));
            out.push_str(&"0".repeat(usize::try_from(self.point - len).unwrap_or(0)));
        } else {
            let split = usize::try_from(self.point).unwrap_or(0);
            out.push_str(&Self::digit_string(&self.digits[..split]));
            out.push(separator);
            out.push_str(&Self::digit_string(&self.digits[split..]));
        }
        out
    }

    /// `1.5e524`.
    pub fn scientific(&self, separator: char) -> String {
        let mut out = String::new();
        if self.negative {
            out.push('-');
        }
        out.push_str(&Self::digit_string(&self.digits[..1]));
        if self.digits.len() > 1 {
            out.push(separator);
            out.push_str(&Self::digit_string(&self.digits[1..]));
        }
        out.push('e');
        out.push_str(&(self.point - 1).to_string());
        out
    }

    /// Scientific notation reads better.
    pub fn wants_scientific(&self) -> bool {
        !self.is_zero() && (self.point > MAX_PLAIN_DIGITS || self.point < MIN_PLAIN_EXPONENT)
    }
}

/// A formatted value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// As displayed and copied by `Enter`.
    pub display: String,
    /// Raw value for `Alt+Enter`: no unit, `.` as decimal separator.
    pub raw: String,
    /// The complete number when `display` is in scientific notation.
    pub full: Option<String>,
}

/// Splits a fend result into its leading number and the rest (unit…).
pub fn split_number(text: &str) -> Option<(Decimal, &str)> {
    let end = text
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*c == '-' && *i == 0)))
        .map_or(text.len(), |(i, _)| i);
    let rest = &text[end..];
    // `5 + 5i`, `0b1100`, `1/2`: not a simple number.
    let complex = rest.trim_start().starts_with(['+', '-']) && rest.ends_with('i');
    if complex || (rest.starts_with(['/', 'i', 'b', 'x', 'o', '(']) && !rest.starts_with(' ')) {
        return None;
    }
    Some((Decimal::parse(&text[..end])?, rest.trim()))
}

/// Formats a fend result.
pub fn shown(text: &str, approx: bool, style: Style, vocabulary: &Vocabulary) -> Shown {
    let separator = if style.french { ',' } else { '.' };
    // `0 + i` is `i`.
    let text = text
        .strip_prefix("0 + ")
        .filter(|t| t.ends_with('i'))
        .unwrap_or(text);
    let Some((mut number, unit)) = split_number(text) else {
        // Complex numbers, fractions, other bases: shown as fend prints them,
        // with the decimal separator of the language.
        let display = if style.french {
            swap_decimal_points(text)
        } else {
            text.to_owned()
        };
        return Shown {
            raw: text.to_owned(),
            display,
            full: None,
        };
    };
    // Approximations are rounded to the significant digits asked, and so
    // are long exact decimals (`465.6612873077392578125`); integers and
    // short exact decimals (`9460730472580.8`) are shown whole.
    if approx || (!number.is_integer() && number.digit_count() > 15) {
        let keep = usize::try_from(number.point)
            .unwrap_or(0)
            .clamp(style.significant.max(1), 15);
        number.round(keep);
    }
    let unit = unit_text(unit, &number, style, vocabulary);
    let scientific = number.wants_scientific();
    let mut value = if scientific {
        let mut short = number.clone();
        short.round(style.significant.max(1));
        short.scientific(separator)
    } else {
        number.plain(separator)
    };
    let full =
        (scientific && number.point < 2000).then(|| with_unit(&number.plain(separator), &unit));
    let raw = if scientific {
        number.scientific('.')
    } else {
        number.plain('.')
    };
    value = with_unit(&value, &unit);
    Shown {
        display: value,
        raw,
        full,
    }
}

fn with_unit(number: &str, unit: &str) -> String {
    if unit.is_empty() {
        number.to_owned()
    } else if unit.starts_with('°') && unit.len() <= 3 || unit == "%" {
        // `30°`, `86 °F` keep fend's spacing for temperatures.
        if unit == "°" || unit == "%" {
            format!("{number}{unit}")
        } else {
            format!("{number} {unit}")
        }
    } else {
        format!("{number} {unit}")
    }
}

/// `1.5` → `1,5` in numbers only (not in `0b1.1`-like words).
fn swap_decimal_points(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let digit_around = i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if *c == '.' && digit_around { ',' } else { *c }
        })
        .collect()
}

/// Unit names in the interface language; ` / ` written `/`.
fn unit_text(unit: &str, number: &Decimal, style: Style, vocabulary: &Vocabulary) -> String {
    let plural = if style.french {
        !number.below_two()
    } else {
        !number.is_one()
    };
    let unit = unit.replace(" / ", "/");
    let mut out = String::with_capacity(unit.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() {
            out.push_str(&translate_unit(word, plural, vocabulary));
            word.clear();
        }
    };
    for c in unit.chars() {
        if c.is_alphabetic() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn translate_unit(word: &str, plural: bool, vocabulary: &Vocabulary) -> String {
    let singular = match word {
        "feet" => "foot".to_owned(),
        "inches" => "inch".to_owned(),
        w if w.len() > 3 && w.ends_with('s') => w[..w.len() - 1].to_owned(),
        w => w.to_owned(),
    };
    vocabulary
        .unit_name(word, plural)
        .or_else(|| vocabulary.unit_name(&singular, plural))
        .map_or_else(|| word.to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: Style = Style {
        french: false,
        significant: 10,
    };
    const FR: Style = Style {
        french: true,
        significant: 10,
    };

    fn show(text: &str, approx: bool, style: Style) -> Shown {
        shown(text, approx, style, &Vocabulary::default())
    }

    #[test]
    fn decimals() {
        let d = Decimal::parse("0012.3400").expect("decimal");
        assert_eq!(d.plain('.'), "12.34");
        assert_eq!(Decimal::parse("0.000").expect("zero").plain('.'), "0");
        assert_eq!(Decimal::parse("-0.05").expect("d").plain(','), "-0,05");
        assert!(Decimal::parse("12a").is_none());
        let mut r = Decimal::parse("9.99996").expect("d");
        r.round(4);
        assert_eq!(r.plain('.'), "10");
    }

    #[test]
    fn scientific_for_huge_and_tiny() {
        let big = format!("15{}", "0".repeat(523));
        let s = show(&big, false, EN);
        assert_eq!(s.display, "1.5e524");
        assert_eq!(s.full.as_deref().map(str::len), Some(525));
        assert_eq!(show("0.000000001", false, EN).display, "1e-9");
        assert_eq!(show("0.00001", false, EN).display, "0.00001");
        assert_eq!(
            show("123456789012345", false, EN).display,
            "123456789012345"
        );
        assert_eq!(show("1234567890123456", false, EN).display, "1.23456789e15");
    }

    #[test]
    fn approximations_and_language() {
        assert_eq!(
            show("3.1068559612 miles", true, EN).display,
            "3.106855961 miles"
        );
        assert_eq!(
            show("3.1068559612 miles", true, FR).display,
            "3,106855961 miles"
        );
        assert_eq!(show("1.5", false, FR).raw, "1.5");
        assert_eq!(show("12.5 MB / s", false, EN).display, "12.5 MB/s");
        assert_eq!(show("86 °F", false, EN).display, "86 °F");
        assert_eq!(show("30°", true, EN).display, "30°");
        assert_eq!(show("15%", false, EN).display, "15%");
        assert_eq!(show("5 + 5i", false, EN).display, "5 + 5i");
        assert_eq!(show("0.5 + 1.5i", false, FR).display, "0,5 + 1,5i");
        assert_eq!(show("1/2", false, EN).display, "1/2");
        assert_eq!(show("0 + i", true, EN).display, "i");
        assert_eq!(
            show("465.6612873077392578125 GiB", false, EN).display,
            "465.6612873 GiB"
        );
        assert_eq!(show("0b1100", false, EN).display, "0b1100");
    }
}

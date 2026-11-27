//! Splitting an input into tokens: numbers (with thousands separators and
//! the decimal separator of the settings), words, symbols, parentheses.

/// How a comma inside a number is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalMode {
    /// `1,5` = 1.5; `,` + space separates function arguments.
    Comma,
    /// `1,500` = 1500 (English grouping); `1,5` is not a number.
    Dot,
    /// Follows `preferred`, but `1,500` is ambiguous: both readings are kept,
    /// the preferred one first.
    Auto { preferred_comma: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A number; `readings` are fend literals (`1500.75`, `0xff`), the
    /// preferred one first (several only for an ambiguous comma).
    Number {
        readings: Vec<String>,
    },
    Word,
    Symbol,
    Open,
    Close,
    /// Argument separator (`;`, or `,` when not part of a number).
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// Text as typed.
    pub text: String,
    /// A space precedes the token.
    pub space_before: bool,
}

impl Token {
    pub fn is_number(&self) -> bool {
        matches!(self.kind, Kind::Number { .. })
    }
}

/// Spaces, including the no-break and thin spaces copied from documents.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{00A0}' | '\u{202F}' | '\u{2009}')
}

/// Separators of thousands groups (followed by exactly 3 digits).
fn is_group_separator(c: char) -> bool {
    is_space(c) || matches!(c, '_' | '\'' | '’')
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

const TWO_CHAR_SYMBOLS: &[&str] = &["**", "->", "=>", "<<", ">>"];

/// Longest accepted input, in chars (beyond: not a calculation).
pub const MAX_INPUT_CHARS: usize = 500;

/// Splits the input. Never fails: unknown characters become symbols.
pub fn tokenize(input: &str, mode: DecimalMode) -> Vec<Token> {
    let chars: Vec<char> = input.chars().take(MAX_INPUT_CHARS).collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut space_before = false;
    while i < chars.len() {
        let c = chars[i];
        if is_space(c) {
            space_before = true;
            i += 1;
            continue;
        }
        let start = i;
        let kind = if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let (readings, end) = number(&chars, i, mode);
            i = end;
            Kind::Number { readings }
        } else if c == '°' {
            // `°C`, `°F`: one token; `°` alone otherwise.
            i += 1;
            if chars.get(i).is_some_and(|n| matches!(n, 'C' | 'F' | 'K'))
                && !chars.get(i + 1).is_some_and(|n| is_word_char(*n))
            {
                i += 1;
            }
            Kind::Word
        } else if is_word_char(c) {
            // Letters only: `h30m` is `h`, `30`, `m`. Digits stay in a word
            // only before a parenthesis (`log2(8)`).
            while i < chars.len() && is_word_char(chars[i]) && !chars[i].is_ascii_digit() {
                i += 1;
            }
            let digits = digits_at(&chars, i);
            if digits > 0 && chars.get(i + digits) == Some(&'(') {
                i += digits;
            }
            // An apostrophe sticks to the word before it: `jusqu'` `au`.
            if i < chars.len() && matches!(chars[i], '\'' | '’') {
                i += 1;
            }
            Kind::Word
        } else if c == '(' || c == '[' {
            i += 1;
            Kind::Open
        } else if c == ')' || c == ']' {
            i += 1;
            Kind::Close
        } else if c == ';' || c == ',' {
            i += 1;
            Kind::Separator
        } else {
            let pair: String = chars[i..chars.len().min(i + 2)].iter().collect();
            i += if TWO_CHAR_SYMBOLS.contains(&pair.as_str()) {
                2
            } else {
                1
            };
            Kind::Symbol
        };
        tokens.push(Token {
            kind,
            text: chars[start..i].iter().collect(),
            space_before,
        });
        space_before = false;
    }
    tokens
}

fn digits_at(chars: &[char], i: usize) -> usize {
    chars[i.min(chars.len())..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count()
}

/// Exactly 3 digits at `i`, not followed by another digit.
fn group_at(chars: &[char], i: usize) -> bool {
    digits_at(chars, i) == 3
}

/// Reads a number at `i`; returns its fend readings and the end index.
fn number(chars: &[char], start: usize, mode: DecimalMode) -> (Vec<String>, usize) {
    let mut i = start;
    // 0x, 0b, 0o literals: kept as typed.
    if chars[i] == '0'
        && chars
            .get(i + 1)
            .is_some_and(|c| matches!(c, 'x' | 'X' | 'b' | 'B' | 'o' | 'O'))
    {
        let base = chars[i + 1].to_ascii_lowercase();
        let valid = |c: &char| match base {
            'x' => c.is_ascii_hexdigit(),
            'b' => matches!(c, '0' | '1'),
            _ => matches!(c, '0'..='7'),
        };
        let len = chars[i + 2..].iter().take_while(|c| valid(c)).count();
        if len > 0 {
            let end = i + 2 + len;
            let text: String = chars[start..end].iter().collect();
            return (vec![text.to_lowercase()], end);
        }
    }

    let mut int = String::new();
    let first = digits_at(chars, i);
    int.extend(&chars[i..i + first]);
    i += first;
    // Thousands groups: 1-3 digits first, then groups of exactly 3.
    let mut comma_grouped = false;
    let mut ambiguous_comma = false;
    if (1..=3).contains(&first) {
        while let Some(&sep) = chars.get(i) {
            if is_group_separator(sep) && group_at(chars, i + 1) {
                int.extend(&chars[i + 1..i + 4]);
                i += 4;
                continue;
            }
            if sep == ',' && group_at(chars, i + 1) {
                // A comma followed by 3 digits: grouping (English) or decimal.
                let more_groups = chars.get(i + 4) == Some(&',') && group_at(chars, i + 5);
                let then_dot = chars.get(i + 4) == Some(&'.');
                let grouping = match mode {
                    DecimalMode::Dot => true,
                    DecimalMode::Comma => false,
                    DecimalMode::Auto { .. } => {
                        if !(comma_grouped || more_groups || then_dot) {
                            ambiguous_comma = true;
                        }
                        true
                    }
                };
                if grouping {
                    comma_grouped = true;
                    int.extend(&chars[i + 1..i + 4]);
                    i += 4;
                    continue;
                }
            }
            break;
        }
    }

    let mut frac = String::new();
    let decimal_comma =
        matches!(mode, DecimalMode::Comma | DecimalMode::Auto { .. }) && !comma_grouped;
    if let Some(&sep) = chars.get(i) {
        let is_decimal = sep == '.' || (sep == ',' && decimal_comma);
        let n = digits_at(chars, i + 1);
        if is_decimal && n > 0 {
            frac.extend(&chars[i + 1..i + 1 + n]);
            i += 1 + n;
        }
    }

    // Exponent: e5, E-9, e+3.
    let mut exp = String::new();
    if let Some(&e) = chars.get(i)
        && matches!(e, 'e' | 'E')
    {
        let sign = chars.get(i + 1).filter(|c| matches!(c, '+' | '-'));
        let at = i + 1 + usize::from(sign.is_some());
        let n = digits_at(chars, at);
        // `2e` alone or `2em`: not an exponent.
        if n > 0 && !chars.get(at + n).is_some_and(|c| c.is_alphabetic()) {
            exp.push('e');
            exp.extend(sign);
            exp.extend(&chars[at..at + n]);
            i = at + n;
        }
    }

    let literal = |int: &str, frac: &str| {
        let mut s = int.to_owned();
        if !frac.is_empty() {
            s.push('.');
            s.push_str(frac);
        }
        s.push_str(&exp);
        s
    };
    let mut readings = vec![literal(&int, &frac)];
    if ambiguous_comma && frac.is_empty() {
        // `1,500` read as 1.5 (comma decimal), the 3 digits of the group being
        // the decimals.
        let digits: String = chars[start..i]
            .iter()
            .filter(|c| c.is_ascii_digit())
            .collect();
        let split = digits.len() - 3;
        let decimal = literal(&digits[..split], &digits[split..]);
        if let DecimalMode::Auto {
            preferred_comma: true,
        } = mode
        {
            readings.insert(0, decimal);
        } else {
            readings.push(decimal);
        }
    }
    (readings, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(input: &str, mode: DecimalMode) -> Vec<Vec<String>> {
        tokenize(input, mode)
            .into_iter()
            .filter_map(|t| match t.kind {
                Kind::Number { readings } => Some(readings),
                _ => None,
            })
            .collect()
    }

    fn texts(input: &str) -> Vec<String> {
        tokenize(input, DecimalMode::Comma)
            .into_iter()
            .map(|t| t.text)
            .collect()
    }

    const FR: DecimalMode = DecimalMode::Comma;
    const EN: DecimalMode = DecimalMode::Dot;
    const AUTO_FR: DecimalMode = DecimalMode::Auto {
        preferred_comma: true,
    };
    const AUTO_EN: DecimalMode = DecimalMode::Auto {
        preferred_comma: false,
    };

    #[test]
    fn thousands_groups() {
        assert_eq!(numbers("1 500", FR), [["1500"]]);
        assert_eq!(numbers("2 500 000", EN), [["2500000"]]);
        assert_eq!(numbers("1\u{202F}500,75", FR), [["1500.75"]]);
        assert_eq!(numbers("1\u{00A0}000", FR), [["1000"]]);
        assert_eq!(numbers("1_000_000", EN), [["1000000"]]);
        assert_eq!(numbers("1'000'000", EN), [["1000000"]]);
        assert_eq!(numbers("1,000,000", EN), [["1000000"]]);
        assert_eq!(numbers("1,500.25", AUTO_FR), [["1500.25"]]);
    }

    #[test]
    fn invalid_groups_stay_separate_numbers() {
        assert_eq!(numbers("15 00", FR), [["15"], ["00"]]);
        assert_eq!(numbers("1 5000", FR), [["1"], ["5000"]]);
        assert_eq!(numbers("1500 000", FR), [["1500"], ["000"]]);
        // A unit after a number is not a group.
        assert_eq!(texts("5 km"), ["5", "km"]);
    }

    #[test]
    fn decimal_comma_and_dot() {
        assert_eq!(numbers("1,5", FR), [["1.5"]]);
        assert_eq!(numbers("1.5", FR), [["1.5"]]);
        assert_eq!(numbers("1,5", AUTO_EN), [["1.5"]]);
        // Dot mode: `1,5` is two numbers around a separator.
        assert_eq!(numbers("1,5", EN), [["1"], ["5"]]);
        assert_eq!(numbers("1,500", FR), [["1.500"]]);
        assert_eq!(numbers("1,500", EN), [["1500"]]);
    }

    #[test]
    fn ambiguous_comma_keeps_both_readings() {
        assert_eq!(numbers("1,500", AUTO_FR), [["1.500", "1500"]]);
        assert_eq!(numbers("1,500", AUTO_EN), [["1500", "1.500"]]);
    }

    #[test]
    fn exponents_and_bases() {
        assert_eq!(numbers("6.022e23", EN), [["6.022e23"]]);
        assert_eq!(numbers("1E-9", EN), [["1e-9"]]);
        assert_eq!(numbers("0xFF + 0b101", EN), [["0xff"], ["0b101"]]);
        // `2e` is 2 × e, `5em` a unit.
        assert_eq!(texts("2e"), ["2", "e"]);
    }

    #[test]
    fn words_symbols_and_units() {
        assert_eq!(texts("5km/h"), ["5", "km", "/", "h"]);
        assert_eq!(texts("30 °C en °F"), ["30", "°C", "en", "°F"]);
        assert_eq!(texts("sin(30°)"), ["sin", "(", "30", "°", ")"]);
        assert_eq!(texts("jusqu'au 25"), ["jusqu'", "au", "25"]);
        assert_eq!(texts("2**10"), ["2", "**", "10"]);
        assert_eq!(texts("pgcd(12; 18)"), ["pgcd", "(", "12", ";", "18", ")"]);
        assert_eq!(texts("année-lumière"), ["année", "-", "lumière"]);
        assert_eq!(texts("1h30m"), ["1", "h", "30", "m"]);
        assert_eq!(
            texts("log2(8) + log10 x"),
            ["log2", "(", "8", ")", "+", "log", "10", "x"]
        );
        let t = tokenize("a  b", FR);
        assert!(!t[0].space_before && t[1].space_before);
    }
}

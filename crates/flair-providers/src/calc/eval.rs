//! Evaluation of a normalized expression: `fend-core` with a deadline, plus
//! the functions fend lacks (`gcd`, `lcm`, `isprime`, `nCr(n, k)`…).

use std::time::{Duration, Instant};

/// Results longer than this (in chars) are "too big to compute".
const MAX_RESULT_CHARS: usize = 20_000;

/// Stops fend when the time is up.
#[derive(Debug, Clone, Copy)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(budget: Duration) -> Self {
        Self(Instant::now() + budget)
    }

    pub fn passed(self) -> bool {
        Instant::now() >= self.0
    }
}

impl fend_core::Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        self.passed()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A value as printed by fend (`approx.` removed).
    Value { text: String, approx: bool },
    /// Primality of an integer, with its smallest factor when found.
    Prime {
        n: u64,
        prime: bool,
        factor: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// Not a valid calculation.
    Invalid,
    /// Too long or too large to compute (`10^10^10^10`, `1000000!`).
    TooBig,
}

/// Evaluates with fend only.
pub fn fend(expr: &str, deadline: Deadline) -> Result<(String, bool), EvalError> {
    if deadline.passed() {
        return Err(EvalError::TooBig);
    }
    let mut context = fend_core::Context::new();
    context.disable_rng();
    let result =
        fend_core::evaluate_with_interrupt(expr, &mut context, &deadline).map_err(|e| {
            if e == "interrupted" || deadline.passed() {
                EvalError::TooBig
            } else {
                EvalError::Invalid
            }
        })?;
    if result.output_is_empty() {
        return Err(EvalError::Invalid);
    }
    let text = result.get_main_result();
    if text.len() > MAX_RESULT_CHARS {
        return Err(EvalError::TooBig);
    }
    // Functions, booleans, dates or objects are not calculation results.
    if text.starts_with('\\') || text.contains('{') || matches!(text, "true" | "false") {
        return Err(EvalError::Invalid);
    }
    match text.strip_prefix("approx. ") {
        Some(rest) => Ok((rest.to_owned(), true)),
        None => Ok((text.to_owned(), false)),
    }
}

/// Evaluates an expression, Flair's functions included.
pub fn evaluate(expr: &str, deadline: Deadline) -> Result<Outcome, EvalError> {
    if let Some(arg) = call_argument(expr, "isprime") {
        let n = integer(&expand(arg, deadline)?, deadline)?;
        let n = u64::try_from(n).map_err(|_| EvalError::TooBig)?;
        let prime = is_prime(n);
        let factor = if prime { None } else { smallest_factor(n) };
        return Ok(Outcome::Prime { n, prime, factor });
    }
    let expanded = expand(expr, deadline)?;
    let (mut text, approx) = fend(&expanded, deadline)?;
    // fend prints 10 decimals; ask for more so that rounding to the
    // significant digits of the settings is done once, from exact digits.
    // (Not for complex numbers: their real part would show its rounding noise.)
    if approx
        && !text.ends_with('i')
        && let Ok((precise, _)) = fend(&format!("({expanded}) to 20 sf"), deadline)
    {
        text = precise;
    }
    Ok(Outcome::Value { text, approx })
}

/// `name(…)` when it is the whole expression: the text inside.
fn call_argument<'a>(expr: &'a str, name: &str) -> Option<&'a str> {
    let rest = expr.trim().strip_prefix(name)?.strip_prefix('(')?;
    let close = matching_paren(rest)?;
    (close + 1 == rest.len()).then(|| &rest[..close])
}

/// Index of the `)` closing an already opened parenthesis.
fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits arguments on the commas of the top level.
fn arguments(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out
}

/// Replaces Flair's functions by their value or by fend syntax.
fn expand(expr: &str, deadline: Deadline) -> Result<String, EvalError> {
    let mut out = String::with_capacity(expr.len());
    let mut rest = expr;
    loop {
        let found = super::normalize::FLAIR_FUNCTIONS
            .iter()
            .filter_map(|name| find_call(rest, name).map(|at| (at, *name)))
            .min_by_key(|(at, _)| *at);
        let Some((at, name)) = found else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(&rest[..at]);
        let inner_start = at + name.len() + 1;
        let close = matching_paren(&rest[inner_start..]).ok_or(EvalError::Invalid)? + inner_start;
        let inner = expand(&rest[inner_start..close], deadline)?;
        let args = arguments(&inner);
        let replacement = match (name, args.as_slice()) {
            ("ncr", [n, k]) => format!("(({n}) nCr ({k}))"),
            ("npr", [n, k]) => format!("(({n}) nPr ({k}))"),
            ("factorial", [x]) => format!("(({x})!)"),
            ("gcd" | "lcm", [first, others @ ..]) if !others.is_empty() => {
                let mut acc = integer(first, deadline)?.unsigned_abs();
                for arg in others {
                    let v = integer(arg, deadline)?.unsigned_abs();
                    acc = if name == "gcd" {
                        gcd(acc, v)
                    } else {
                        lcm(acc, v).ok_or(EvalError::TooBig)?
                    };
                }
                acc.to_string()
            }
            // `isprime` inside a larger expression, wrong argument counts…
            _ => return Err(EvalError::Invalid),
        };
        out.push_str(&replacement);
        rest = &rest[close + 1..];
    }
}

/// Position of `name(` as a whole word.
fn find_call(text: &str, name: &str) -> Option<usize> {
    let pattern = format!("{name}(");
    let mut from = 0;
    while let Some(pos) = text[from..].find(&pattern) {
        let at = from + pos;
        let word_start = !text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if word_start {
            return Some(at);
        }
        from = at + pattern.len();
    }
    None
}

/// Evaluates to an exact integer.
fn integer(expr: &str, deadline: Deadline) -> Result<i128, EvalError> {
    let (text, approx) = fend(expr, deadline)?;
    if approx {
        return Err(EvalError::Invalid);
    }
    let digits = text.strip_prefix('-').unwrap_or(&text);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(EvalError::Invalid);
    }
    text.parse::<i128>().map_err(|_| EvalError::TooBig)
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn lcm(a: u128, b: u128) -> Option<u128> {
    if a == 0 || b == 0 {
        return Some(0);
    }
    (a / gcd(a, b)).checked_mul(b)
}

fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    u64::try_from(u128::from(a) * u128::from(b) % u128::from(m)).unwrap_or(0)
}

fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut result = 1;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mul_mod(result, base, m);
        }
        base = mul_mod(base, base, m);
        exp >>= 1;
    }
    result
}

/// Deterministic Miller–Rabin for 64-bit integers.
pub fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for p in BASES {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    let mut d = n - 1;
    let mut r = 0;
    while d.is_multiple_of(2) {
        d /= 2;
        r += 1;
    }
    'bases: for a in BASES {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..r {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// Smallest factor by trial division (up to one million).
fn smallest_factor(n: u64) -> Option<u64> {
    if n < 4 {
        return None;
    }
    (2..=1_000_000u64)
        .take_while(|d| d.saturating_mul(*d) <= n)
        .find(|d| n.is_multiple_of(*d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(expr: &str) -> Result<Outcome, EvalError> {
        evaluate(expr, Deadline::after(Duration::from_millis(200)))
    }

    fn value(expr: &str) -> String {
        match eval(expr) {
            Ok(Outcome::Value { text, .. }) => text,
            other => panic!("{expr}: {other:?}"),
        }
    }

    #[test]
    fn fend_values() {
        assert_eq!(value("1/3 + 1/6"), "0.5");
        assert_eq!(value("(5 km) to miles"), "3.106855961186669849 miles");
        assert_eq!(eval("5 km + 3 kg"), Err(EvalError::Invalid));
        assert_eq!(eval("1/0"), Err(EvalError::Invalid));
        assert_eq!(eval("x: x^2"), Err(EvalError::Invalid));
    }

    #[test]
    fn flair_functions() {
        assert_eq!(value("gcd(12, 18)"), "6");
        assert_eq!(value("lcm(4, 6)"), "12");
        assert_eq!(value("gcd(12, 18, 8) + 1"), "3");
        assert_eq!(value("ncr(5, 2)"), "10");
        assert_eq!(value("npr(5, 2)"), "20");
        assert_eq!(value("factorial(5)"), "120");
        assert_eq!(eval("gcd(1.5, 3)"), Err(EvalError::Invalid));
        assert_eq!(eval("gcd(12)"), Err(EvalError::Invalid));
        // A word containing a function name is not a call.
        assert_eq!(find_call("xgcd(1)", "gcd"), None);
    }

    #[test]
    fn primality() {
        assert_eq!(
            eval("isprime(7)"),
            Ok(Outcome::Prime {
                n: 7,
                prime: true,
                factor: None
            })
        );
        assert_eq!(
            eval("isprime(21)"),
            Ok(Outcome::Prime {
                n: 21,
                prime: false,
                factor: Some(3)
            })
        );
        assert!(is_prime(18_446_744_073_709_551_557));
        assert!(!is_prime(1));
        assert!(!is_prime(561));
    }

    #[test]
    fn too_big_is_reported() {
        let t = Instant::now();
        assert_eq!(
            evaluate("10^10^10^10", Deadline::after(Duration::from_millis(50))),
            Err(EvalError::TooBig)
        );
        assert_eq!(
            evaluate("1000000!", Deadline::after(Duration::from_millis(50))),
            Err(EvalError::TooBig)
        );
        assert!(t.elapsed() < Duration::from_millis(500));
    }
}

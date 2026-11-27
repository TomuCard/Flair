//! Multilingual normalization: tokens are matched against the active
//! lexicons (whole tokens, longest form first), each word gets the senses
//! whose conditions hold, and the possible readings are rendered in the
//! syntax of `fend-core`.

use std::collections::{HashMap, HashSet};

use flair_core::lexicon::{self, Lexicon, MAX_FORM_TOKENS, Position, When};

use super::token::{DecimalMode, Kind, Token, tokenize};

/// Most readings evaluated for one input.
pub const MAX_READINGS: usize = 16;

/// Words that `fend-core` treats as keywords: they must come from a lexicon
/// (so that disabling a language really disables them), never be passed as is.
const FEND_KEYWORDS: &[&str] = &[
    "to", "as", "in", "into", "of", "per", "and", "or", "xor", "not", "then", "mod", "is",
];

/// Functions of `fend-core` usable by their English name in any language.
const FEND_FUNCTIONS: &[&str] = &[
    "sqrt", "cbrt", "abs", "floor", "ceil", "round", "ln", "log", "log2", "log10", "exp", "sin",
    "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
];

/// Functions computed by Flair itself (see `eval`).
pub const FLAIR_FUNCTIONS: &[&str] = &["gcd", "lcm", "ncr", "npr", "factorial", "isprime"];

const TRIG: &[&str] = &["sin", "cos", "tan"];
const INVERSE_TRIG: &[&str] = &["asin", "acos", "atan"];
const ANGLE_UNITS: &[&str] = &[
    "°", "deg", "degree", "degrees", "rad", "radian", "radians", "grad",
];
const TIME_UNITS: &[&str] = &[
    "s", "sec", "second", "seconds", "min", "minute", "minutes", "h", "hour", "hours", "day",
    "days", "week", "weeks", "month", "months", "year", "years", "ms",
];
/// Units of a compound duration (`1h30m`, `5m30s`) that make `m` a minute.
const DURATION_MARKERS: &[&str] = &["h", "hour", "s", "sec", "second", "min"];

/// One lexicon entry, ready for matching.
#[derive(Debug, Clone)]
struct Entry {
    /// Tokens of the form: folded, or exact when case sensitive.
    tokens: Vec<String>,
    case_sensitive: bool,
    sense: String,
    priority: i32,
    when: When,
    /// 0 for `common`, then 1, 2… in the order of `input_languages`.
    lang_rank: usize,
}

/// The active vocabulary: entries of `common` and of the input languages.
#[derive(Debug, Clone, Default)]
pub struct Vocabulary {
    by_first: HashMap<String, Vec<Entry>>,
    /// Forms of inactive languages, never passed to fend: folded, and exact
    /// for case-sensitive entries (`j` blocks `j`, not `J`, the joule).
    blocked: HashSet<String>,
    blocked_exact: HashSet<String>,
    /// Unit name → (singular, plural) in the interface language.
    display: HashMap<String, (String, String)>,
}

fn form_tokens(form: &str) -> Vec<String> {
    tokenize(form, DecimalMode::Dot)
        .into_iter()
        .map(|t| t.text)
        .collect()
}

impl Vocabulary {
    /// `active`: `common` first, then the input languages in order of
    /// preference. `inactive`: built-in languages that are disabled.
    /// `display`: lexicons of the interface language (then `common`) used to
    /// name units in results.
    pub fn new(active: &[&Lexicon], inactive: &[&Lexicon], display: &[&Lexicon]) -> Self {
        let mut by_first: HashMap<String, Vec<Entry>> = HashMap::new();
        for (rank, lexicon) in active.iter().enumerate() {
            for word in &lexicon.words {
                for form in &word.forms {
                    let case_sensitive = word.is_case_sensitive();
                    let tokens: Vec<String> = form_tokens(form)
                        .into_iter()
                        .map(|t| if case_sensitive { t } else { lexicon::fold(&t) })
                        .collect();
                    if tokens.is_empty() || tokens.len() > MAX_FORM_TOKENS {
                        continue;
                    }
                    by_first
                        .entry(lexicon::fold(&tokens[0]))
                        .or_default()
                        .push(Entry {
                            tokens,
                            case_sensitive,
                            sense: word.sense.clone(),
                            priority: word.priority,
                            when: word.when.clone(),
                            lang_rank: rank,
                        });
                }
            }
        }
        let mut blocked: HashSet<String> = FEND_KEYWORDS.iter().map(|k| (*k).to_owned()).collect();
        let mut blocked_exact = HashSet::new();
        for lexicon in inactive {
            for word in &lexicon.words {
                for form in &word.forms {
                    if word.is_case_sensitive() {
                        blocked_exact.insert(form.clone());
                    } else {
                        blocked.insert(lexicon::fold(form));
                    }
                }
            }
        }
        let mut names: HashMap<String, (String, String)> = HashMap::new();
        for lexicon in display {
            // Unconditioned entries name units best (`jour`, not `j`); then
            // conditioned ones (`an`, `ans`), never case-sensitive ones (the
            // `m` of compound durations).
            let unconditioned = lexicon.words.iter().filter(|w| w.when.count() == 0);
            let conditioned = lexicon
                .words
                .iter()
                .filter(|w| w.when.count() > 0 && !w.is_case_sensitive());
            for word in unconditioned.chain(conditioned) {
                if let Some(unit) = word.sense.strip_prefix("unit:")
                    && !names.contains_key(unit)
                {
                    let singular = word.forms[0].clone();
                    let plural = word
                        .forms
                        .get(1)
                        .cloned()
                        .unwrap_or_else(|| singular.clone());
                    names.insert(unit.to_owned(), (singular, plural));
                }
            }
        }
        Self {
            by_first,
            blocked,
            blocked_exact,
            display: names,
        }
    }

    fn is_blocked(&self, word: &str) -> bool {
        self.blocked.contains(&lexicon::fold(word)) || self.blocked_exact.contains(word)
    }

    /// Name of a unit in the interface language, if the lexicons have one.
    pub fn unit_name(&self, unit: &str, plural: bool) -> Option<&str> {
        self.display
            .get(unit)
            .map(|(s, p)| if plural { p.as_str() } else { s.as_str() })
    }
}

/// A sense chosen for a segment.
#[derive(Debug, Clone, PartialEq)]
pub enum Sense {
    /// A number literal for fend.
    Number(String),
    /// A lexicon sense (`unit:day`, `convert_to`…).
    Lexicon(String),
    /// The word as typed (fend may know it: `miles`, `sqrt`, `e`).
    Raw(String),
    /// A word of a disabled language or a fend keyword: no valid reading.
    Blocked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub sense: Sense,
    pub priority: i32,
    pub conditions: u32,
    pub lang_rank: usize,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub kind: Kind,
    /// Text as typed (several tokens for multi-word forms).
    pub text: String,
    /// Possible senses, best first; never empty.
    pub candidates: Vec<Candidate>,
    /// A word fend knows as a unit (`miles`), when no lexicon names it.
    pub raw_unit: bool,
}

/// Splits tokens into segments with their candidate senses.
pub fn segments(
    tokens: &[Token],
    vocabulary: &Vocabulary,
    is_unit: &dyn Fn(&str) -> bool,
) -> Vec<Segment> {
    // 1. Longest lexicon matches.
    let mut raw: Vec<(Kind, String, Vec<&Entry>)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        let mut matched: Vec<&Entry> = Vec::new();
        let mut len = 0;
        if !token.is_number()
            && let Some(entries) = vocabulary.by_first.get(&lexicon::fold(&token.text))
        {
            for n in (1..=MAX_FORM_TOKENS.min(tokens.len() - i)).rev() {
                let window = &tokens[i..i + n];
                matched = entries
                    .iter()
                    .filter(|e| e.tokens.len() == n)
                    .filter(|e| {
                        e.tokens.iter().zip(window).all(|(f, t)| {
                            if e.case_sensitive {
                                *f == t.text
                            } else {
                                *f == lexicon::fold(&t.text)
                            }
                        })
                    })
                    .collect();
                if !matched.is_empty() {
                    len = n;
                    break;
                }
            }
        }
        if len == 0 {
            raw.push((token.kind.clone(), token.text.clone(), Vec::new()));
            i += 1;
        } else {
            let text = tokens[i..i + len]
                .iter()
                .enumerate()
                .map(|(k, t)| {
                    if k > 0 && t.space_before {
                        format!(" {}", t.text)
                    } else {
                        t.text.clone()
                    }
                })
                .collect();
            raw.push((Kind::Word, text, matched));
            i += len;
        }
    }

    // 2. Classes of each segment, from what it could be.
    let n = raw.len();
    let is_number: Vec<bool> = raw
        .iter()
        .map(|(k, _, e)| matches!(k, Kind::Number { .. }) || e.iter().any(|e| e.sense == "one"))
        .collect();
    let unit_names: Vec<Vec<String>> = raw
        .iter()
        .map(|(kind, text, entries)| {
            let mut names: Vec<String> = entries
                .iter()
                .filter_map(|e| e.sense.strip_prefix("unit:").map(str::to_owned))
                .collect();
            if entries.is_empty()
                && matches!(kind, Kind::Word)
                && !vocabulary.is_blocked(text)
                && is_unit(text)
            {
                names.push(text.clone());
            }
            names
        })
        .collect();
    let is_unit_seg: Vec<bool> = unit_names.iter().map(|u| !u.is_empty()).collect();
    let is_slash = |k: usize| matches!(raw[k].0, Kind::Symbol) && raw[k].1 == "/";
    let mut quantity = vec![false; n];
    for k in 0..n {
        quantity[k] = is_unit_seg[k]
            && k > 0
            && (is_number[k - 1]
                || (is_unit_seg[k - 1] && quantity[k - 1])
                || (k > 1 && is_slash(k - 1) && quantity[k - 2]));
    }
    let duration: Vec<bool> = (0..n)
        .map(|k| {
            quantity[k]
                && unit_names[k]
                    .iter()
                    .any(|u| TIME_UNITS.contains(&u.as_str()))
        })
        .collect();
    let percent: Vec<bool> = raw
        .iter()
        .map(|(k, t, e)| {
            (matches!(k, Kind::Symbol) && t == "%") || e.iter().any(|e| e.sense == "percent")
        })
        .collect();
    // Compound durations: number-unit pairs side by side, at least two,
    // one of them hours or seconds (`1h30m`, `5m30s`).
    let mut in_duration = vec![false; n];
    let mut k = 0;
    while k < n {
        let mut end = k;
        let mut units = Vec::new();
        while end + 1 < n && is_number[end] && is_unit_seg[end + 1] {
            units.push(end + 1);
            end += 2;
        }
        if units.len() >= 2
            && units.iter().any(|&u| {
                unit_names[u]
                    .iter()
                    .any(|n| DURATION_MARKERS.contains(&n.as_str()))
            })
        {
            for u in units {
                in_duration[u] = true;
            }
        }
        k = end.max(k + 1);
    }

    let has_class = |k: Option<usize>, class: &str, ahead: bool| -> bool {
        let Some(k) = k else {
            return false;
        };
        if let Some(sense) = class.strip_prefix("sense:") {
            return raw[k].2.iter().any(|e| e.sense == sense);
        }
        // Looking ahead, a number starting a quantity counts as that quantity.
        let starts = |pred: &[bool]| ahead && is_number[k] && k + 1 < n && pred[k + 1];
        match class {
            "number" => is_number[k],
            "unit" => is_unit_seg[k],
            "quantity" => quantity[k] || starts(&quantity),
            "duration" => duration[k] || starts(&duration),
            "percent" => percent[k],
            _ => false,
        }
    };
    let holds = |k: usize, when: &When| -> bool {
        let prev = k.checked_sub(1);
        let next = (k + 1 < n).then_some(k + 1);
        let before_ok = |classes: &[&String]| {
            classes.iter().any(|c| {
                if *c == "start" {
                    prev.is_none()
                } else {
                    has_class(prev, c, false)
                }
            })
        };
        let after_ok = |classes: &[&String]| {
            classes.iter().any(|c| {
                if *c == "end" {
                    next.is_none()
                } else {
                    has_class(next, c, true)
                }
            })
        };
        let required_before = when.required_before();
        let required_after = when.required_after();
        (required_before.is_empty() || before_ok(&required_before))
            && (required_after.is_empty() || after_ok(&required_after))
            && match when.position {
                Some(Position::Start) => k == 0,
                Some(Position::End) => k + 1 == n,
                None => true,
            }
            && when.in_duration.is_none_or(|d| d == in_duration[k])
    };

    // 3. Candidates whose conditions hold.
    raw.iter()
        .enumerate()
        .map(|(k, (kind, text, entries))| {
            let mut candidates: Vec<Candidate> = match kind {
                Kind::Number { readings } => readings
                    .iter()
                    .enumerate()
                    .map(|(r, literal)| Candidate {
                        sense: Sense::Number(literal.clone()),
                        // The other reading of an ambiguous comma comes second.
                        priority: -i32::try_from(r).unwrap_or(1),
                        conditions: 0,
                        lang_rank: 0,
                    })
                    .collect(),
                _ => entries
                    .iter()
                    .filter(|e| holds(k, &e.when))
                    .map(|e| Candidate {
                        sense: Sense::Lexicon(e.sense.clone()),
                        priority: e.priority,
                        conditions: e.when.count(),
                        lang_rank: e.lang_rank,
                    })
                    .collect(),
            };
            // A word fend knows as a unit stays possible as typed, after the
            // lexicon senses (`J` is a joule, not the `j` of `jours`).
            if !candidates.is_empty()
                && matches!(kind, Kind::Word)
                && !vocabulary.is_blocked(text)
                && is_unit(text)
            {
                candidates.push(Candidate {
                    sense: Sense::Raw(text.clone()),
                    priority: -1000,
                    conditions: 0,
                    lang_rank: 100,
                });
            }
            // Same sense from several languages: keep the preferred one.
            let mut seen = HashSet::new();
            candidates.sort_by(|a, b| {
                b.priority
                    .cmp(&a.priority)
                    .then(b.conditions.cmp(&a.conditions))
                    .then(a.lang_rank.cmp(&b.lang_rank))
            });
            candidates.retain(|c| seen.insert(format!("{:?}", c.sense)));
            if candidates.is_empty() {
                let sense = if matches!(kind, Kind::Word) && vocabulary.is_blocked(text) {
                    Sense::Blocked
                } else {
                    Sense::Raw(text.clone())
                };
                candidates.push(Candidate {
                    sense,
                    priority: 0,
                    conditions: 0,
                    lang_rank: 0,
                });
            }
            Segment {
                kind: kind.clone(),
                text: text.clone(),
                raw_unit: is_unit_seg[k] || is_unit(text),
                candidates,
            }
        })
        .collect()
}

/// One way to read the input: a candidate index per segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading(pub Vec<usize>);

/// Ranking key of a reading: priorities, conditions met, language order
/// (lower is better), reinterpretations (lower is better).
fn key(
    segments: &[Segment],
    reading: &Reading,
) -> (i32, u32, std::cmp::Reverse<usize>, std::cmp::Reverse<usize>) {
    let mut priority = 0;
    let mut conditions = 0;
    let mut lang = 0;
    let mut alternatives = 0;
    for (segment, &choice) in segments.iter().zip(&reading.0) {
        let c = &segment.candidates[choice];
        priority += c.priority;
        conditions += c.conditions;
        lang += c.lang_rank;
        alternatives += usize::from(choice > 0);
    }
    (
        priority,
        conditions,
        std::cmp::Reverse(lang),
        std::cmp::Reverse(alternatives),
    )
}

/// The possible readings, best first, at most [`MAX_READINGS`].
pub fn readings(segments: &[Segment]) -> Vec<Reading> {
    let mut limits: Vec<usize> = segments.iter().map(|s| s.candidates.len()).collect();
    // Keep the combinations countable: trim the longest choice lists.
    while limits.iter().product::<usize>() > 256 {
        if let Some(max) = limits.iter_mut().max() {
            *max -= 1;
        }
    }
    let mut all = vec![Reading(vec![0; segments.len()])];
    for (index, &limit) in limits.iter().enumerate() {
        if limit <= 1 {
            continue;
        }
        let mut next = Vec::with_capacity(all.len() * limit);
        for reading in &all {
            for choice in 0..limit {
                let mut r = reading.clone();
                r.0[index] = choice;
                next.push(r);
            }
        }
        all = next;
    }
    all.sort_by_key(|r| std::cmp::Reverse(key(segments, r)));
    all.truncate(MAX_READINGS);
    all
}

/// Options of the rendering.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Trigonometry without an angle unit is in degrees.
    pub degrees: bool,
    /// `kB`, `MB`… are read as `KiB`, `MiB`… (like Windows Explorer).
    pub binary_prefixes: bool,
}

/// A reading in fend syntax, with what is needed to judge and show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub fend: String,
    /// The reading, written back for "read as: …".
    pub read_as: String,
    /// Not worth a result alone (`42`, `pi`).
    pub trivial: bool,
    /// Explicit conversion (`to`).
    pub converts: bool,
    /// Divisions of plain numbers (`1/3 + 1/6` has 2): fractions to show.
    pub fractions: usize,
    /// Conversion to another numeral base (`0x`, `0b`, `0o`).
    pub base_prefix: Option<&'static str>,
    /// Units used, in fend names.
    pub units: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Num(String),
    Unit(String),
    Op(String),
    Convert,
    Of,
    Percent,
    Func(String),
    Value(String),
    Open,
    Close,
    Sep,
    Text(String),
}

fn decimal_to_binary(unit: &str) -> Option<&'static str> {
    Some(match unit {
        "kB" => "KiB",
        "MB" => "MiB",
        "GB" => "GiB",
        "TB" => "TiB",
        "PB" => "PiB",
        _ => return None,
    })
}

/// Renders a reading; `None` when it cannot be valid (blocked word, two
/// numbers side by side…).
pub fn render(segments: &[Segment], reading: &Reading, options: RenderOptions) -> Option<Rendered> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut read_as: Vec<String> = Vec::new();
    for (segment, &choice) in segments.iter().zip(&reading.0) {
        let candidate = &segment.candidates[choice];
        let (piece, shown) = match (&candidate.sense, &segment.kind) {
            (Sense::Blocked, _) => return None,
            (Sense::Number(n), _) => (Piece::Num(n.clone()), n.clone()),
            (Sense::Raw(_), Kind::Open) => (Piece::Open, "(".into()),
            (Sense::Raw(_), Kind::Close) => (Piece::Close, ")".into()),
            (Sense::Raw(_), Kind::Separator) => (Piece::Sep, ";".into()),
            (Sense::Raw(t), Kind::Symbol) => match t.as_str() {
                "%" => (Piece::Percent, "%".into()),
                "+" | "-" | "*" | "/" | "^" | "!" | "<<" | ">>" => {
                    (Piece::Op(t.clone()), t.clone())
                }
                // `=` at the end (`2+2=`) is harmless.
                "=" => continue,
                // Anything else (`@`, `$`, `&`…): not supported here.
                _ => return None,
            },
            (Sense::Raw(t), _) => {
                let lower = t.to_lowercase();
                if FEND_FUNCTIONS.contains(&lower.as_str()) {
                    (Piece::Func(lower), t.clone())
                } else if segment.raw_unit {
                    (Piece::Unit(t.clone()), t.clone())
                } else {
                    (Piece::Value(t.clone()), t.clone())
                }
            }
            (Sense::Lexicon(sense), _) => {
                let piece = match sense.as_str() {
                    "convert_to" => Piece::Convert,
                    "of" => Piece::Of,
                    "percent" => Piece::Percent,
                    "one" => Piece::Num("1".into()),
                    "ignore" => continue,
                    s => {
                        if let Some(unit) = s.strip_prefix("unit:") {
                            let unit = if options.binary_prefixes {
                                decimal_to_binary(unit).unwrap_or(unit)
                            } else {
                                unit
                            };
                            Piece::Unit(unit.to_owned())
                        } else if let Some(op) = s.strip_prefix("op:") {
                            Piece::Op(op.to_owned())
                        } else if let Some(f) = s.strip_prefix("func:") {
                            Piece::Func(f.to_owned())
                        } else if let Some(c) = s.strip_prefix("const:") {
                            Piece::Value(c.to_owned())
                        } else {
                            // Other senses (dates, times, rates) are evaluated
                            // from milestone 5: no reading yet.
                            Piece::Text(s.strip_prefix("fend:")?.to_owned())
                        }
                    }
                };
                let shown = match &piece {
                    Piece::Unit(u) => u.clone(),
                    Piece::Convert => "→".into(),
                    Piece::Op(o) => o.clone(),
                    Piece::Num(n) => n.clone(),
                    _ => segment.text.clone(),
                };
                (piece, shown)
            }
        };
        pieces.push(piece);
        read_as.push(shown);
    }
    let units: Vec<String> = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Unit(u) => Some(u.clone()),
            Piece::Value(v) => Some(v.clone()),
            _ => None,
        })
        .collect();
    let pieces = rewrite(pieces, options)?;
    let converts = pieces.contains(&Piece::Convert);
    let operations = pieces.iter().any(|p| {
        matches!(
            p,
            Piece::Op(_)
                | Piece::Convert
                | Piece::Of
                | Piece::Percent
                | Piece::Func(_)
                | Piece::Text(_)
        )
    });
    let numbers = pieces.iter().filter(|p| matches!(p, Piece::Num(_))).count();
    let values = pieces
        .iter()
        .filter(|p| matches!(p, Piece::Num(_) | Piece::Value(_)))
        .count();
    let has_unit = pieces.iter().any(|p| matches!(p, Piece::Unit(_)));
    // `2pi` multiplies: not trivial, unlike `pi` or `42` alone.
    let trivial = !operations && values < 2 && !(numbers == 1 && has_unit);
    let plain = pieces
        .iter()
        .all(|p| matches!(p, Piece::Num(_) | Piece::Op(_) | Piece::Open | Piece::Close));
    let fractions = if plain {
        pieces
            .windows(3)
            .filter(|w| matches!(w, [Piece::Num(_), Piece::Op(o), Piece::Num(_)] if o == "/"))
            .count()
    } else {
        0
    };
    let base_prefix = match pieces.iter().skip_while(|p| **p != Piece::Convert).nth(1) {
        Some(Piece::Text(t)) if t == "hex" => Some("0x"),
        Some(Piece::Text(t)) if t == "binary" => Some("0b"),
        Some(Piece::Text(t)) if t == "octal" => Some("0o"),
        _ => None,
    };
    Some(Rendered {
        fend: to_text(&pieces),
        read_as: read_as.join(" "),
        trivial,
        converts,
        fractions,
        base_prefix,
        units,
    })
}

/// Structural rewrites on the pieces (see each step).
fn rewrite(mut p: Vec<Piece>, options: RenderOptions) -> Option<Vec<Piece>> {
    // Two numbers side by side are not a calculation (`15 00`).
    if p.windows(2)
        .any(|w| matches!(w, [Piece::Num(_), Piece::Num(_)]))
    {
        return None;
    }

    // Quantities side by side add up: `5 ft 11 in` → `5 ft + 11 in`; a bare
    // number after feet is inches (`5 pieds 11`).
    let mut i = 0;
    while i + 1 < p.len() {
        if matches!(p[i], Piece::Unit(_)) && matches!(p[i + 1], Piece::Num(_)) {
            let next_is_unit = matches!(p.get(i + 2), Some(Piece::Unit(_)));
            let feet = matches!(&p[i], Piece::Unit(u) if u == "foot" || u == "ft" || u == "feet");
            let ends = matches!(p.get(i + 2), None | Some(Piece::Convert));
            if next_is_unit || (feet && ends) {
                p.insert(i + 1, Piece::Op("+".into()));
                if !next_is_unit {
                    p.insert(i + 3, Piece::Unit("inch".into()));
                }
            }
        }
        i += 1;
    }

    // Percentages added or removed: `240 + 15%` → `(240) * (1 + 15/100)`.
    let end = p
        .iter()
        .position(|x| *x == Piece::Convert)
        .unwrap_or(p.len());
    if end >= 4
        && p[end - 1] == Piece::Percent
        && matches!(&p[end - 2], Piece::Num(_))
        && matches!(&p[end - 3], Piece::Op(o) if o == "+" || o == "-")
    {
        let Piece::Op(op) = p[end - 3].clone() else {
            return None;
        };
        let Piece::Num(n) = p[end - 2].clone() else {
            return None;
        };
        let mut out = vec![Piece::Open];
        out.extend(p[..end - 3].iter().cloned());
        out.push(Piece::Close);
        out.push(Piece::Op("*".into()));
        out.push(Piece::Text(format!("(1 {op} {n}/100)")));
        out.extend(p[end..].iter().cloned());
        p = out;
    }

    // Other percentages are hundredths (`240 * 15%`), except before `of`
    // (`15% of 240`) and as a conversion target (`… to %`).
    let mut i = 0;
    while i + 1 < p.len() {
        let target = i > 0 && p[i - 1] == Piece::Convert;
        if let (Piece::Num(n), Piece::Percent) = (&p[i], &p[i + 1])
            && !target
            && p.get(i + 2) != Some(&Piece::Of)
        {
            let n = n.clone();
            p.splice(i..i + 2, [Piece::Text(format!("({n}/100)"))]);
        }
        i += 1;
    }

    // `5 nCr 2`: infix form of fend (the call form `ncr(5, 2)` is Flair's).
    for i in 0..p.len() {
        if let Piece::Func(f) = &p[i]
            && (f == "ncr" || f == "npr")
            && p.get(i + 1) != Some(&Piece::Open)
        {
            p[i] = Piece::Text(if f == "ncr" {
                "nCr".into()
            } else {
                "nPr".into()
            });
        }
    }

    // Quantities are grouped, so `100 km / 1.5 h` divides by 1.5 hours.
    let mut out: Vec<Piece> = Vec::with_capacity(p.len() + 4);
    let mut i = 0;
    while i < p.len() {
        if matches!(p[i], Piece::Num(_)) && matches!(p.get(i + 1), Some(Piece::Unit(_))) {
            let mut j = i + 1;
            while matches!(p.get(j), Some(Piece::Unit(_))) {
                j += 1;
            }
            out.push(Piece::Open);
            out.extend(p[i..j].iter().cloned());
            out.push(Piece::Close);
            i = j;
        } else {
            out.push(p[i].clone());
            i += 1;
        }
    }
    let mut p = out;

    // Trigonometry: degrees by default when no angle unit is given.
    if options.degrees {
        let mut i = 0;
        while i < p.len() {
            if matches!(&p[i], Piece::Func(f) if TRIG.contains(&f.as_str())) {
                if p.get(i + 1) == Some(&Piece::Open) {
                    let close = matching_close(&p, i + 1)?;
                    let has_angle = p[i + 2..close]
                        .iter()
                        .any(|x| matches!(x, Piece::Unit(u) | Piece::Value(u) if ANGLE_UNITS.contains(&u.as_str())));
                    if !has_angle {
                        p.insert(close, Piece::Close);
                        p.insert(close + 1, Piece::Unit("°".into()));
                        p.insert(i + 2, Piece::Open);
                    }
                } else if matches!(p.get(i + 1), Some(Piece::Num(_))) {
                    // `sin 30` → `sin((30) °)`.
                    let n = p.remove(i + 1);
                    let wrapped = [
                        Piece::Open,
                        Piece::Open,
                        n,
                        Piece::Close,
                        Piece::Unit("°".into()),
                        Piece::Close,
                    ];
                    for (k, x) in wrapped.into_iter().enumerate() {
                        p.insert(i + 1 + k, x);
                    }
                }
            }
            i += 1;
        }
        // Inverse functions answer in degrees.
        if matches!(p.first(), Some(Piece::Func(f)) if INVERSE_TRIG.contains(&f.as_str()))
            && !p.contains(&Piece::Convert)
        {
            p.push(Piece::Convert);
            p.push(Piece::Unit("°".into()));
        }
    }

    // `7 est premier` / `7 prime`: the function applies to what precedes.
    if matches!(p.last(), Some(Piece::Func(f)) if f == "isprime") && p.len() > 1 {
        p.pop();
        let mut out = vec![Piece::Func("isprime".into()), Piece::Open];
        out.extend(p);
        out.push(Piece::Close);
        p = out;
    }
    Some(p)
}

fn matching_close(p: &[Piece], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (k, x) in p.iter().enumerate().skip(open) {
        match x {
            Piece::Open => depth += 1,
            Piece::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn to_text(pieces: &[Piece]) -> String {
    let mut out = String::new();
    let mut after_func = false;
    let mut after_convert = false;
    for piece in pieces {
        let (text, glue) = match piece {
            Piece::Num(n) => (n.as_str(), false),
            Piece::Unit(u) | Piece::Value(u) | Piece::Func(u) => (u.as_str(), false),
            Piece::Op(o) if o == "!" => ("!", true),
            Piece::Op(o) => (o.as_str(), false),
            Piece::Convert => ("to", false),
            Piece::Of => ("of", false),
            Piece::Percent => ("%", !after_convert),
            Piece::Open => ("(", after_func),
            Piece::Close => (")", true),
            Piece::Sep => (",", true),
            Piece::Text(t) => (t.as_str(), t.starts_with('^')),
        };
        if !out.is_empty() && !glue && !out.ends_with('(') {
            out.push(' ');
        }
        out.push_str(text);
        after_func = matches!(piece, Piece::Func(_));
        after_convert = *piece == Piece::Convert;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use flair_core::lexicon::builtin;

    fn vocabulary(langs: &[&str]) -> Vocabulary {
        let common = builtin("common").expect("common");
        let active: Vec<Lexicon> = std::iter::once(common.clone())
            .chain(langs.iter().filter_map(|l| builtin(l)))
            .collect();
        let inactive: Vec<Lexicon> = flair_core::lexicon::builtin_languages()
            .into_iter()
            .filter(|l| !langs.contains(l))
            .filter_map(builtin)
            .collect();
        Vocabulary::new(
            &active.iter().collect::<Vec<_>>(),
            &inactive.iter().collect::<Vec<_>>(),
            &[&common],
        )
    }

    fn fake_unit(word: &str) -> bool {
        matches!(
            word,
            "km" | "miles"
                | "cm"
                | "ft"
                | "h"
                | "min"
                | "s"
                | "kg"
                | "g"
                | "l"
                | "ml"
                | "m"
                | "MB"
                | "GB"
        )
    }

    const OPTIONS: RenderOptions = RenderOptions {
        degrees: true,
        binary_prefixes: false,
    };

    /// All valid renderings, best first.
    fn all(input: &str, langs: &[&str]) -> Vec<String> {
        let tokens = tokenize(
            input,
            DecimalMode::Auto {
                preferred_comma: true,
            },
        );
        let segs = segments(&tokens, &vocabulary(langs), &fake_unit);
        let mut out: Vec<String> = Vec::new();
        for r in readings(&segs)
            .iter()
            .filter_map(|r| render(&segs, r, OPTIONS))
        {
            if !out.contains(&r.fend) {
                out.push(r.fend);
            }
        }
        out
    }

    fn best(input: &str) -> String {
        all(input, &["fr", "en"])
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    #[test]
    fn words_of_both_languages() {
        assert_eq!(best("5 km en miles"), "(5 km) to miles");
        assert_eq!(best("5 km to miles"), "(5 km) to miles");
        assert_eq!(best("3 fois 4"), "3 * 4");
        assert_eq!(best("10 divided by 4"), "10 / 4");
        assert_eq!(best("10 divisé par 4"), "10 / 4");
        assert_eq!(best("15% de 240"), "15% of 240");
        assert_eq!(best("4 jours en heures"), "(4 day) to hour");
        assert_eq!(best("2 semaines"), "(2 week)");
    }

    #[test]
    fn whole_tokens_only() {
        // `de` is not replaced inside `dernier`, nor `in` inside `minutes`.
        assert_eq!(best("15% dernier"), "(15/100) dernier");
        assert_eq!(best("5 minutes"), "(5 minute)");
    }

    #[test]
    fn the_three_senses_of_in() {
        assert_eq!(best("12 in en cm"), "(12 inch) to cm");
        assert_eq!(best("5 km in miles"), "(5 km) to miles");
        // `in 90 min` is relative time (milestone 5): no reading yet, but
        // never inches nor a conversion.
        assert!(all("in 90 min", &["en"]).is_empty());
    }

    #[test]
    fn m_is_meter_then_minute() {
        assert_eq!(all("5m", &["fr", "en"]), ["(5 m)", "(5 min)"]);
        assert_eq!(best("1h30m"), "(1 h) + (30 min)");
        assert_eq!(best("5m30s"), "(5 min) + (30 s)");
    }

    #[test]
    fn an_is_a_year_after_a_number_or_one_before_a_unit() {
        assert_eq!(best("3 ans"), "(3 year)");
        assert_eq!(best("an h"), "(1 h)");
    }

    #[test]
    fn ambiguous_comma_gives_two_readings() {
        assert_eq!(all("1,500 + 1", &["fr"]), ["1.500 + 1", "1500 + 1"]);
    }

    #[test]
    fn disabled_language_words_are_blocked() {
        assert!(all("5 km to miles", &["fr"]).is_empty());
        assert!(all("5 km in miles", &["fr"]).is_empty());
        assert!(all("3 times 4", &["fr"]).is_empty());
        assert!(all("3 fois 4", &["en"]).is_empty());
        // `in` is never a conversion with French alone.
        assert!(all("12 in en cm", &["fr"]).is_empty());
    }

    #[test]
    fn rewrites() {
        assert_eq!(best("240 + 15%"), "(240) * (1 + 15/100)");
        assert_eq!(best("240 * 15%"), "240 * (15/100)");
        assert_eq!(best("36 / 240 → %"), "36 / 240 to %");
        assert_eq!(best("5 nCr 2"), "5 nCr 2");
        assert_eq!(best("100 km / 1.5 h"), "(100 km) / (1.5 h)");
        assert_eq!(best("sin(30)"), "sin((30) °)");
        assert_eq!(best("sin(30°)"), "sin((30 °))");
        assert_eq!(best("sin 30"), "sin((30) °)");
        assert_eq!(best("asin(0.5)"), "asin(0.5) to °");
        assert_eq!(best("5 ft 11 in to cm"), "(5 ft) + (11 inch) to cm");
        assert_eq!(best("5 pieds 11 en cm"), "(5 foot) + (11 inch) to cm");
        assert_eq!(best("pgcd(12; 18)"), "gcd(12, 18)");
        assert_eq!(best("7 est premier"), "isprime(7)");
        assert_eq!(best("5!"), "5!");
        assert!(all("15 00", &["fr"]).is_empty());
    }

    #[test]
    fn triviality() {
        let check = |input: &str| {
            let tokens = tokenize(input, DecimalMode::Dot);
            let segs = segments(&tokens, &vocabulary(&["fr", "en"]), &fake_unit);
            render(&segs, &readings(&segs)[0], OPTIONS).map(|r| r.trivial)
        };
        assert_eq!(check("42"), Some(true));
        assert_eq!(check("pi"), Some(true));
        assert_eq!(check("pi*2"), Some(false));
        assert_eq!(check("5 km"), Some(false));
        assert_eq!(check("sqrt(2)"), Some(false));
    }
}

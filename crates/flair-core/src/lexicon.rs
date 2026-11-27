//! Language files (lexicons) of the calculator: words of each language mapped
//! to canonical senses, with optional conditions and priorities.
//!
//! The format and the closed vocabulary of conditions are documented in
//! `lexicon/README.md`. Built-in files are embedded in the binary; the user
//! can add or override files in the `lexicon/` folder of the configuration.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Built-in files: `(code, TOML text)`. `common` is always active.
pub const BUILTIN: &[(&str, &str)] = &[
    ("common", include_str!("../../../lexicon/common.toml")),
    ("fr", include_str!("../../../lexicon/fr.toml")),
    ("en", include_str!("../../../lexicon/en.toml")),
];

/// Code of the neutral file, always active.
pub const COMMON: &str = "common";

/// Highest number of words in one form (`jours jusqu'au` = 3 tokens…).
pub const MAX_FORM_TOKENS: usize = 6;

/// Classes a condition may refer to.
pub const CLASSES: &[&str] = &[
    "number", "quantity", "unit", "duration", "time", "date", "percent", "start", "end",
];

/// Senses that are not `prefix:value` (see `lexicon/README.md`).
pub const SENSES: &[&str] = &[
    "convert_to",
    "of",
    "percent",
    "one",
    "ignore",
    "now",
    "ago",
    "relative_in",
    "until",
    "since",
    "what_day",
    "rate_at",
    "time_in",
];

/// Prefixes of parameterized senses (`unit:day`, `op:*`, `func:sqrt`…).
pub const SENSE_PREFIXES: &[&str] = &[
    "unit:", "op:", "func:", "const:", "fend:", "month:", "weekday:",
];

/// Date order of a language, for `01/02/2026`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateOrder {
    Dmy,
    Mdy,
    Ymd,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub code: String,
    #[serde(default)]
    pub name: String,
    /// `,` or `.` (not for `common`).
    #[serde(default)]
    pub decimal_separator: Option<char>,
    #[serde(default)]
    pub date_order: Option<DateOrder>,
}

/// One class or a list of classes (`before = ["end", "sense:convert_to"]`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Classes {
    One(String),
    Many(Vec<String>),
}

impl Classes {
    pub fn list(&self) -> &[String] {
        match self {
            Self::One(c) => std::slice::from_ref(c),
            Self::Many(c) => c,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Start,
    End,
}

/// Conditions of an entry; all given ones must hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    pub after: Option<Classes>,
    pub before: Option<Classes>,
    pub between: Option<(Classes, Classes)>,
    pub position: Option<Position>,
    pub in_duration: Option<bool>,
    pub case_sensitive: Option<bool>,
}

impl When {
    /// Number of conditions (more conditions = more precise entry).
    pub fn count(&self) -> u32 {
        u32::from(self.after.is_some())
            + u32::from(self.before.is_some())
            + 2 * u32::from(self.between.is_some())
            + u32::from(self.position.is_some())
            + u32::from(self.in_duration.is_some())
    }

    /// Class required just before the token, merging `after` and `between`.
    pub fn required_before(&self) -> Vec<&String> {
        let mut out: Vec<&String> = Vec::new();
        out.extend(self.after.iter().flat_map(Classes::list));
        out.extend(self.between.iter().flat_map(|(a, _)| a.list()));
        out
    }

    pub fn required_after(&self) -> Vec<&String> {
        let mut out: Vec<&String> = Vec::new();
        out.extend(self.before.iter().flat_map(Classes::list));
        out.extend(self.between.iter().flat_map(|(_, b)| b.list()));
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Word {
    pub forms: Vec<String>,
    pub sense: String,
    #[serde(default)]
    pub when: When,
    #[serde(default)]
    pub priority: i32,
    /// Same as `when.case_sensitive` (accepted at both places).
    #[serde(default)]
    pub case_sensitive: bool,
}

impl Word {
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive || self.when.case_sensitive == Some(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lexicon {
    pub meta: Meta,
    #[serde(default, rename = "word")]
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LexiconError {
    #[error("invalid TOML: {0}")]
    Toml(String),
    #[error("word {index} ({form}): {reason}")]
    Word {
        index: usize,
        form: String,
        reason: String,
    },
    #[error("[meta] code {0:?} must be a short lowercase language code")]
    Code(String),
}

fn valid_class(class: &str) -> bool {
    CLASSES.contains(&class) || class.strip_prefix("sense:").is_some_and(valid_sense)
}

pub fn valid_sense(sense: &str) -> bool {
    SENSES.contains(&sense)
        || SENSE_PREFIXES
            .iter()
            .any(|p| sense.strip_prefix(p).is_some_and(|v| !v.is_empty()))
}

impl Lexicon {
    /// Parses and validates a language file.
    pub fn parse(text: &str) -> Result<Self, LexiconError> {
        let lexicon: Self =
            toml::from_str(text).map_err(|e| LexiconError::Toml(e.message().to_owned()))?;
        let code = &lexicon.meta.code;
        let code_ok = !code.is_empty()
            && code.len() <= 12
            && code
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_');
        if !code_ok {
            return Err(LexiconError::Code(code.clone()));
        }
        for (index, word) in lexicon.words.iter().enumerate() {
            let fail = |reason: String| LexiconError::Word {
                index: index + 1,
                form: word.forms.first().cloned().unwrap_or_default(),
                reason,
            };
            if word.forms.is_empty() || word.forms.iter().any(|f| f.trim().is_empty()) {
                return Err(fail("empty form".into()));
            }
            if !valid_sense(&word.sense) {
                return Err(fail(format!("unknown sense {:?}", word.sense)));
            }
            let w = &word.when;
            let classes = w
                .after
                .iter()
                .chain(&w.before)
                .chain(w.between.iter().flat_map(|(a, b)| [a, b]))
                .flat_map(Classes::list);
            for class in classes {
                if !valid_class(class) {
                    return Err(fail(format!("unknown class {class:?}")));
                }
            }
        }
        Ok(lexicon)
    }
}

/// Lowercase and accent-free form, for case- and accent-insensitive matching
/// (`Année` → `annee`).
pub fn fold(text: &str) -> String {
    text.chars().flat_map(fold_char).collect()
}

fn fold_char(c: char) -> impl Iterator<Item = char> {
    let base = match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ÿ' | 'Ÿ' => 'y',
        '’' => '\'',
        other => other,
    };
    base.to_lowercase()
}

// ------------------------------------------------------------- conflicts

/// A form with different senses in several active files.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub form: String,
    /// `(file code, sense, priority, conditions)` of each side.
    pub a: (String, String, i32, When),
    pub b: (String, String, i32, When),
    pub resolved: bool,
    /// Why it is resolved (or not), in English, for the logs and the CLI.
    pub reason: String,
    /// An input where both entries could apply.
    pub example: String,
}

/// Classes known to describe the same token (a quantity ends with a unit, a
/// duration is a quantity).
fn classes_overlap(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let pair = |x: &str, y: &str| (a == x && b == y) || (a == y && b == x);
    pair("unit", "quantity")
        || pair("duration", "quantity")
        || pair("duration", "unit")
        || a.starts_with("sense:")
        || b.starts_with("sense:")
}

fn sets_exclusive(a: &[&String], b: &[&String]) -> bool {
    !a.is_empty() && !b.is_empty() && !a.iter().any(|x| b.iter().any(|y| classes_overlap(x, y)))
}

/// True when two entries can never apply to the same token.
pub fn mutually_exclusive(a: &When, b: &When) -> bool {
    let before_a = a.required_before();
    let before_b = b.required_before();
    let after_a = a.required_after();
    let after_b = b.required_after();
    if sets_exclusive(&before_a, &before_b) || sets_exclusive(&after_a, &after_b) {
        return true;
    }
    // At the start, the previous token is `start`.
    let starts = |w: &When, before: &[&String]| {
        w.position == Some(Position::Start) && before.iter().all(|c| *c == "start")
    };
    let needs_previous =
        |before: &[&String]| !before.is_empty() && before.iter().all(|c| *c != "start");
    if (starts(a, &before_a) && needs_previous(&before_b))
        || (starts(b, &before_b) && needs_previous(&before_a))
    {
        return true;
    }
    let ends = |w: &When| w.position == Some(Position::End);
    let needs_next = |after: &[&String]| !after.is_empty() && after.iter().all(|c| *c != "end");
    if (ends(a) && needs_next(&after_b)) || (ends(b) && needs_next(&after_a)) {
        return true;
    }
    matches!((a.position, b.position), (Some(x), Some(y)) if x != y)
        || matches!((a.in_duration, b.in_duration), (Some(x), Some(y)) if x != y)
}

fn class_example(class: &str) -> &str {
    match class {
        "number" => "5",
        "quantity" => "5 km",
        "unit" => "km",
        "duration" => "2 h",
        "time" => "14:30",
        "date" => "2026-01-01",
        "percent" => "15 %",
        "start" | "end" => "",
        _ => "x",
    }
}

fn example(form: &str, w: &When) -> String {
    let before = w
        .required_before()
        .first()
        .map_or("5", |c| class_example(c));
    let after = w.required_after().first().map_or("", |c| class_example(c));
    let before = if w.position == Some(Position::Start) {
        ""
    } else {
        before
    };
    [before, form, after]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lists the conflicts between active files (`common` included). Same-file
/// homonyms are listed too: they follow the same rules.
pub fn conflicts(files: &[&Lexicon]) -> Vec<Conflict> {
    // folded form → (file, word)
    let mut by_form: BTreeMap<String, Vec<(&str, &Word, &str)>> = BTreeMap::new();
    for file in files {
        for word in &file.words {
            for form in &word.forms {
                // Folded even for case-sensitive entries: `To` (terabyte) and
                // `to` (conversion) both match what the user types as `To`.
                by_form
                    .entry(fold(form))
                    .or_default()
                    .push((&file.meta.code, word, form));
            }
        }
    }
    let mut out = Vec::new();
    for (form, entries) in by_form {
        for i in 0..entries.len() {
            for j in i + 1..entries.len() {
                let ((fa, a, form_a), (fb, b, form_b)) = (entries[i], entries[j]);
                if a.sense == b.sense || (fa == fb && std::ptr::eq(a, b)) {
                    continue;
                }
                let (resolved, reason) =
                    if a.is_case_sensitive() && b.is_case_sensitive() && form_a != form_b {
                        (true, format!("case-sensitive {form_a:?} and {form_b:?}"))
                    } else if a.priority != b.priority {
                        (
                            true,
                            format!("priorities {} and {}", a.priority, b.priority),
                        )
                    } else if mutually_exclusive(&a.when, &b.when) {
                        (true, "conditions are mutually exclusive".to_owned())
                    } else {
                        (false, "same priority and overlapping conditions".to_owned())
                    };
                out.push(Conflict {
                    example: example(&form, &a.when),
                    form: form.clone(),
                    a: (fa.to_owned(), a.sense.clone(), a.priority, a.when.clone()),
                    b: (fb.to_owned(), b.sense.clone(), b.priority, b.when.clone()),
                    resolved,
                    reason,
                });
            }
        }
    }
    out
}

/// Reads the user's language files (`*.toml` of a folder). Invalid files
/// are returned apart, with the reason, to be reported.
pub fn load_dir(dir: &std::path::Path) -> (Vec<Lexicon>, Vec<(std::path::PathBuf, String)>) {
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (valid, invalid);
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    paths.sort();
    for path in paths {
        match std::fs::read_to_string(&path) {
            Ok(text) => match Lexicon::parse(&text) {
                Ok(lexicon) => valid.push(lexicon),
                Err(e) => invalid.push((path, e.to_string())),
            },
            Err(e) => invalid.push((path, e.to_string())),
        }
    }
    (valid, invalid)
}

/// The built-in lexicon of a language code.
pub fn builtin(code: &str) -> Option<Lexicon> {
    BUILTIN
        .iter()
        .find(|(c, _)| *c == code)
        .and_then(|(_, text)| Lexicon::parse(text).ok())
}

/// Codes of the built-in input languages (not `common`).
pub fn builtin_languages() -> Vec<&'static str> {
    BUILTIN
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| *c != COMMON)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_files_are_valid() {
        for (code, text) in BUILTIN {
            let lexicon = Lexicon::parse(text).unwrap_or_else(|e| panic!("{code}: {e}"));
            assert_eq!(lexicon.meta.code, *code);
            if *code != COMMON {
                assert!(lexicon.meta.decimal_separator.is_some(), "{code}");
                assert!(lexicon.meta.date_order.is_some(), "{code}");
            }
        }
    }

    /// `flair lexicon check` for every combination of built-in languages.
    #[test]
    fn no_unresolved_conflicts_in_any_combination() {
        let langs = builtin_languages();
        let common = builtin(COMMON).expect("common");
        let count = 1usize << langs.len();
        for mask in 1..count {
            let mut files: Vec<Lexicon> = vec![common.clone()];
            files.extend(
                langs
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .filter_map(|(_, c)| builtin(c)),
            );
            let refs: Vec<&Lexicon> = files.iter().collect();
            let unresolved: Vec<String> = conflicts(&refs)
                .into_iter()
                .filter(|c| !c.resolved)
                .map(|c| format!("{} ({} {} / {} {})", c.form, c.a.0, c.a.1, c.b.0, c.b.1))
                .collect();
            assert!(unresolved.is_empty(), "mask {mask}: {unresolved:?}");
        }
    }

    #[test]
    fn invalid_files_are_rejected_with_a_reason() {
        let bad_sense = "[meta]\ncode = \"xx\"\n[[word]]\nforms = [\"a\"]\nsense = \"teleport\"\n";
        assert!(matches!(
            Lexicon::parse(bad_sense),
            Err(LexiconError::Word { .. })
        ));
        let bad_class = "[meta]\ncode = \"xx\"\n[[word]]\nforms = [\"a\"]\nsense = \"one\"\nwhen = { after = \"banana\" }\n";
        assert!(Lexicon::parse(bad_class).is_err());
        let bad_key = "[meta]\ncode = \"xx\"\n[[word]]\nforms = [\"a\"]\nsense = \"one\"\nwhen = { regex = \".*\" }\n";
        assert!(Lexicon::parse(bad_key).is_err());
        assert!(matches!(
            Lexicon::parse("[meta]\ncode = \"Fr!\"\n"),
            Err(LexiconError::Code(_))
        ));
        assert!(matches!(
            Lexicon::parse("not toml ["),
            Err(LexiconError::Toml(_))
        ));
    }

    #[test]
    fn folding_removes_case_and_accents() {
        assert_eq!(fold("Année"), "annee");
        assert_eq!(fold("DIVISÉ"), "divise");
        assert_eq!(fold("jusqu’au"), "jusqu'au");
    }

    fn when(text: &str) -> When {
        toml::from_str(text).expect("when")
    }

    #[test]
    fn exclusive_conditions() {
        let inch = when("after = \"number\"");
        let relative = when("position = \"start\"\nbefore = \"duration\"");
        let convert = when("between = [\"quantity\", \"unit\"]");
        assert!(mutually_exclusive(&inch, &relative));
        assert!(mutually_exclusive(&inch, &convert));
        assert!(mutually_exclusive(&relative, &convert));
        let unconditioned = When::default();
        assert!(!mutually_exclusive(&inch, &unconditioned));
        assert!(!mutually_exclusive(
            &when("after = \"unit\""),
            &when("after = \"quantity\"")
        ));
    }

    #[test]
    fn conflicts_are_found_and_classified() {
        let a = Lexicon::parse(
            "[meta]\ncode = \"aa\"\n[[word]]\nforms = [\"in\"]\nsense = \"convert_to\"\n",
        )
        .expect("a");
        let b = Lexicon::parse(
            "[meta]\ncode = \"bb\"\n[[word]]\nforms = [\"IN\"]\nsense = \"unit:inch\"\n",
        )
        .expect("b");
        let c = conflicts(&[&a, &b]);
        assert_eq!(c.len(), 1);
        assert!(!c[0].resolved);
        assert_eq!(c[0].example, "5 in");
        let b2 = Lexicon::parse("[meta]\ncode = \"bb\"\n[[word]]\nforms = [\"in\"]\nsense = \"unit:inch\"\npriority = 5\n").expect("b2");
        assert!(conflicts(&[&a, &b2])[0].resolved);
    }
}

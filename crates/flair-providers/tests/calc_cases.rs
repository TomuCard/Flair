//! The calculator test table: `lexicon/tests/*.toml`, run with every
//! combination of input languages and both interface languages.

// Helpers of a test file: panicking on a broken test table is intended.
#![allow(clippy::expect_used)]

use std::collections::HashMap;

use flair_core::i18n::{I18n, Lang};
use flair_core::settings::config::{AngleUnit, DecimalSeparator};
use flair_core::{CancellationToken, Provider, Query};
use flair_providers::calc::{CalcProvider, Calculator, Settings, Texts};
use serde::Deserialize;

const COMMON: &str = include_str!("../../../lexicon/tests/common.toml");
const LANGS: &[(&str, &str)] = &[
    ("fr", include_str!("../../../lexicon/tests/fr.toml")),
    ("en", include_str!("../../../lexicon/tests/en.toml")),
];
/// Combinations of input languages, in order of preference.
const COMBINATIONS: &[&[&str]] = &[&["fr"], &["en"], &["fr", "en"], &["en", "fr"]];

#[derive(Debug, Deserialize)]
struct File {
    case: Vec<Case>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: Option<String>,
    input: String,
    expect: Option<String>,
    expect_fr: Option<String>,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default)]
    shared: bool,
    twin: Option<bool>,
    #[serde(default)]
    none: bool,
    #[serde(default)]
    too_big: bool,
    #[serde(default)]
    langs: Vec<String>,
    decimal: Option<String>,
    system: Option<String>,
    #[serde(default)]
    binary_prefixes: bool,
    angle: Option<String>,
    digits: Option<usize>,
}

fn parse(text: &str) -> Vec<Case> {
    toml::from_str::<File>(text).expect("test file").case
}

/// `1.5` → `1,5` (numbers only).
fn french_decimals(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let between = i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if *c == '.' && between { ',' } else { *c }
        })
        .collect()
}

fn settings(case: &Case, langs: &[&str], french: bool) -> Settings {
    Settings {
        input_languages: langs.iter().map(|l| (*l).to_owned()).collect(),
        decimal_separator: match case.decimal.as_deref() {
            Some(",") => DecimalSeparator::Comma,
            Some(".") => DecimalSeparator::Dot,
            _ => DecimalSeparator::Auto,
        },
        system_language: case.system.clone().unwrap_or_else(|| "fr".into()),
        french_interface: french,
        angle_unit: if case.angle.as_deref() == Some("rad") {
            AngleUnit::Rad
        } else {
            AngleUnit::Deg
        },
        significant_digits: case.digits.unwrap_or(10),
        binary_prefixes: case.binary_prefixes,
    }
}

/// Titles of the calculator rows (main first).
fn titles(case: &Case, langs: &[&str], french: bool) -> Vec<String> {
    let i18n = I18n::new(if french { Lang::Fr } else { Lang::En });
    let provider = CalcProvider::new(
        Calculator::new(&settings(case, langs, french), &[]),
        Texts::new(&i18n),
    );
    let mut items = provider.query(&Query::parse(&case.input), &CancellationToken::new());
    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    items.into_iter().map(|i| i.title).collect()
}

/// Checks one case in one combination; returns the problems found.
fn check(case: &Case, langs: &[&str], expected_valid: bool) -> Vec<String> {
    let mut problems = Vec::new();
    for french in [false, true] {
        let got = titles(case, langs, french);
        let ui = if french { "fr" } else { "en" };
        let context = format!("{:?} [{}] ui={ui}", case.input, langs.join(","));
        if !expected_valid || case.none {
            if !got.is_empty() {
                problems.push(format!("{context}: expected no result, got {got:?}"));
            }
            continue;
        }
        if case.too_big {
            let too_big = I18n::new(if french { Lang::Fr } else { Lang::En }).t("calc-too-big");
            if got.first() != Some(&too_big) {
                problems.push(format!("{context}: expected too big, got {got:?}"));
            }
            continue;
        }
        let Some(expect) = &case.expect else {
            problems.push(format!("{context}: no expectation"));
            continue;
        };
        let expected = if french {
            case.expect_fr
                .clone()
                .unwrap_or_else(|| french_decimals(expect))
        } else {
            expect.clone()
        };
        if got.first() != Some(&expected) {
            problems.push(format!("{context}: expected {expected:?}, got {got:?}"));
            continue;
        }
        // Variants are written for the English interface.
        if french {
            continue;
        }
        for v in &case.variants {
            if !got[1..].contains(v) {
                problems.push(format!("{context}: variant {v:?} missing in {got:?}"));
            }
        }
    }
    problems
}

#[test]
fn table_has_enough_cases() {
    let count = parse(COMMON).len() + LANGS.iter().map(|(_, t)| parse(t).len()).sum::<usize>();
    assert!(count >= 150, "{count} cases");
}

#[test]
fn common_cases() {
    let mut problems = Vec::new();
    for case in parse(COMMON) {
        for langs in COMBINATIONS {
            if case.langs.iter().all(|l| langs.contains(&l.as_str())) {
                problems.extend(check(&case, langs, true));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn language_cases_alone_and_combined() {
    let mut problems = Vec::new();
    for (lang, text) in LANGS {
        for case in parse(text) {
            for langs in COMBINATIONS {
                let active = langs.contains(lang);
                problems.extend(check(&case, langs, active || case.shared));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// Every French case has an English twin with the same result.
#[test]
fn bilingual_twins() {
    let files: HashMap<&str, Vec<Case>> = LANGS.iter().map(|(l, t)| (*l, parse(t))).collect();
    let mut problems = Vec::new();
    for (lang, cases) in &files {
        for case in cases {
            if case.twin == Some(false) {
                continue;
            }
            let Some(id) = &case.id else {
                problems.push(format!("{lang}: {:?} has no id", case.input));
                continue;
            };
            for (other, other_cases) in &files {
                if other == lang {
                    continue;
                }
                match other_cases.iter().find(|c| c.id.as_ref() == Some(id)) {
                    None => problems.push(format!("{lang}:{id} has no twin in {other}")),
                    Some(twin)
                        if (twin.expect.clone(), twin.expect_fr.clone())
                            != (case.expect.clone(), case.expect_fr.clone()) =>
                    {
                        problems.push(format!("{lang}:{id}: results differ from {other}"));
                    }
                    Some(_) => {}
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

mod robustness {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]

        /// Normalization and evaluation never panic, whatever the input.
        #[test]
        fn never_panics(input in "\\PC{0,40}") {
            let calculator = Calculator::new(&Settings::default(), &[]);
            let _ = calculator.calculate(&input, &CancellationToken::new());
        }

        #[test]
        fn never_panics_on_calculation_like_input(input in "[0-9 ,.+*/^()%!a-zé°-]{0,30}") {
            let calculator = Calculator::new(&Settings::default(), &[]);
            let _ = calculator.calculate(&input, &CancellationToken::new());
        }
    }
}

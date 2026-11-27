//! `flair lexicon check [--langs fr,en]`: lists the conflicts between
//! language files, with an example input for each, and fails when one is
//! unresolved. Without `--langs`, every combination of the available
//! languages is checked (the CI runs it).

use std::process::ExitCode;

use flair_core::lexicon::{self, Lexicon};
use flair_platform::console;
use flair_platform::paths::Paths;

const USAGE: &str = "usage: flair lexicon check [--langs fr,en]";

pub fn run(args: &[String], paths: Option<&Paths>) -> ExitCode {
    let mut langs: Option<Vec<String>> = None;
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        Some("check") => {}
        _ => {
            console::eprintln(USAGE);
            return ExitCode::FAILURE;
        }
    }
    while let Some(arg) = iter.next() {
        let value = match arg.strip_prefix("--langs=") {
            Some(v) => Some(v.to_owned()),
            None if arg == "--langs" => iter.next().cloned(),
            None => None,
        };
        match value {
            Some(v) => {
                langs = Some(
                    v.split(',')
                        .map(|l| l.trim().to_owned())
                        .filter(|l| !l.is_empty())
                        .collect(),
                )
            }
            None => {
                console::eprintln(USAGE);
                return ExitCode::FAILURE;
            }
        }
    }

    // Built-in files, replaced or completed by the user's.
    let mut user = Vec::new();
    if let Some(paths) = paths {
        let (valid, invalid) = lexicon::load_dir(&paths.lexicon_dir());
        for (path, reason) in &invalid {
            console::eprintln(&format!("invalid: {}: {reason}", path.display()));
        }
        user = valid;
    }
    let all = flair_providers::calc::lexicons(&user);
    let Some(common) = all.get(lexicon::COMMON) else {
        console::eprintln("no common lexicon");
        return ExitCode::FAILURE;
    };
    let mut available: Vec<&String> = all.keys().filter(|c| *c != lexicon::COMMON).collect();
    available.sort();

    let combinations: Vec<Vec<String>> = match langs {
        Some(list) => {
            if let Some(unknown) = list.iter().find(|l| !all.contains_key(*l)) {
                console::eprintln(&format!("unknown language: {unknown}"));
                return ExitCode::FAILURE;
            }
            vec![list]
        }
        None => (1..1usize << available.len())
            .map(|mask| {
                available
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, c)| (*c).clone())
                    .collect()
            })
            .collect(),
    };

    let mut unresolved = 0;
    for combination in &combinations {
        let files: Vec<&Lexicon> = std::iter::once(common)
            .chain(combination.iter().filter_map(|c| all.get(c)))
            .collect();
        let conflicts = lexicon::conflicts(&files);
        console::println(&format!(
            "common + {}: {} conflict(s)",
            combination.join(", "),
            conflicts.len()
        ));
        for c in conflicts {
            let status = if c.resolved { "resolved" } else { "UNRESOLVED" };
            console::println(&format!(
                "  {:<12} {}:{} (priority {}) / {}:{} (priority {}) → {status}: {}; example: {:?}",
                c.form, c.a.0, c.a.1, c.a.2, c.b.0, c.b.1, c.b.2, c.reason, c.example
            ));
            unresolved += usize::from(!c.resolved);
        }
    }
    if unresolved > 0 {
        console::eprintln(&format!("{unresolved} unresolved conflict(s)"));
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

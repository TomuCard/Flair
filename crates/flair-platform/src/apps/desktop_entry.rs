//! Freedesktop `.desktop` files: parsing the `[Desktop Entry]` group and the
//! `Exec` line (Desktop Entry Specification 1.5).
//!
//! Written here rather than with `freedesktop-desktop-entry`: Flair needs the
//! French and English names at the same time (that crate picks one locale),
//! and full control over `Exec` expansion; the format is small.

use std::collections::HashMap;

/// The keys Flair uses from a `[Desktop Entry]` group.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    /// `key` or `key[locale]` → raw value (already unescaped).
    values: HashMap<String, String>,
}

impl DesktopEntry {
    /// Parses the `[Desktop Entry]` group; `None` if there is none.
    pub fn parse(text: &str) -> Option<Self> {
        let mut values = HashMap::new();
        let mut in_group = false;
        let mut found = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_group = line == "[Desktop Entry]";
                found |= in_group;
                continue;
            }
            if !in_group {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                // The first occurrence wins (duplicates are invalid anyway).
                values
                    .entry(key.trim().to_owned())
                    .or_insert_with(|| unescape(value.trim()));
            }
        }
        found.then_some(Self { values })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Localized value for a language (`fr`): `key[fr_FR]`-like variants
    /// first, then `key[fr]`. `None` if there is no translation.
    pub fn localized(&self, key: &str, lang: &str) -> Option<&str> {
        let prefix = format!("{key}[{lang}");
        let exact = format!("{key}[{lang}]");
        self.get(&exact).or_else(|| {
            self.values
                .iter()
                .filter(|(k, _)| {
                    k.starts_with(&prefix)
                        && matches!(k.as_bytes().get(prefix.len()), Some(b'_' | b'@'))
                })
                // Deterministic choice between several regional variants.
                .min_by(|a, b| a.0.cmp(b.0))
                .map(|(_, v)| v.as_str())
        })
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key) == Some("true")
    }

    /// A `;`-separated list (`Keywords`, `OnlyShowIn`…).
    pub fn list(value: Option<&str>) -> Vec<String> {
        value
            .unwrap_or_default()
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// True if the entry should appear in the given desktops
    /// (`XDG_CURRENT_DESKTOP`, `:`-separated).
    pub fn shown_in(&self, current_desktops: &[String]) -> bool {
        let only = Self::list(self.get("OnlyShowIn"));
        let not = Self::list(self.get("NotShowIn"));
        let current = |d: &String| current_desktops.iter().any(|c| c.eq_ignore_ascii_case(d));
        if !only.is_empty() && !only.iter().any(current) {
            return false;
        }
        !not.iter().any(current)
    }
}

/// General escapes of string values: `\s`, `\n`, `\t`, `\r`, `\\`. Other
/// sequences (like `\;` in lists) are kept for later stages.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Values substituted for the field codes of `Exec`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecContext<'a> {
    /// `Icon` key, for `%i`.
    pub icon: Option<&'a str>,
    /// Translated name, for `%c`.
    pub name: &'a str,
    /// Path of the `.desktop` file, for `%k`.
    pub location: &'a str,
}

/// Splits an `Exec` value into a program and its arguments, expanding field
/// codes. Flair launches without files or URLs, so `%f %F %u %U` disappear.
/// `None` if the line is empty or badly quoted.
pub fn parse_exec(exec: &str, cx: &ExecContext<'_>) -> Option<Vec<String>> {
    let mut args = Vec::new();
    for word in split_quoted(exec)? {
        // A field code alone in its argument: replaced by 0, 1 or 2 arguments.
        match word.as_str() {
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => continue,
            "%i" => {
                if let Some(icon) = cx.icon.filter(|i| !i.is_empty()) {
                    args.push("--icon".to_owned());
                    args.push(icon.to_owned());
                }
                continue;
            }
            _ => {}
        }
        let mut out = String::with_capacity(word.len());
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => out.push('%'),
                Some('c') => out.push_str(cx.name),
                Some('k') => out.push_str(cx.location),
                // Inside an argument, other codes expand to nothing.
                _ => {}
            }
        }
        args.push(out);
    }
    (!args.is_empty()).then_some(args)
}

/// Shell-like splitting of `Exec`: spaces separate arguments, double quotes
/// group them, and inside quotes `\"`, `` \` ``, `\$`, `\\` are escapes.
fn split_quoted(exec: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut has_word = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                has_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            e @ ('"' | '`' | '$' | '\\') => current.push(e),
                            other => {
                                current.push('\\');
                                current.push(other);
                            }
                        },
                        other => current.push(other),
                    }
                }
            }
            ' ' | '\t' => {
                if has_word {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            _ => {
                has_word = true;
                current.push(c);
            }
        }
    }
    if has_word {
        words.push(current);
    }
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &str = "\
[Desktop Entry]
Name=Files
Name[fr]=Fichiers
Name[de_DE]=Dateien
GenericName=File Manager
GenericName[fr_FR]=Gestionnaire de fichiers
Keywords=folder;manager;explore;
Keywords[fr]=dossier;gestionnaire;
Exec=nautilus --new-window %U
Icon=org.gnome.Nautilus
Type=Application
OnlyShowIn=GNOME;Unity;

[Desktop Action new-window]
Name=New Window
Exec=nautilus --new-window
";

    #[test]
    fn names_in_both_languages() {
        let e = DesktopEntry::parse(FILES).expect("entry");
        assert_eq!(e.get("Name"), Some("Files"));
        assert_eq!(e.localized("Name", "fr"), Some("Fichiers"));
        assert_eq!(e.localized("Name", "de"), Some("Dateien"));
        assert_eq!(e.localized("Name", "es"), None);
        assert_eq!(
            e.localized("GenericName", "fr"),
            Some("Gestionnaire de fichiers")
        );
        assert_eq!(
            DesktopEntry::list(e.localized("Keywords", "fr")),
            ["dossier", "gestionnaire"]
        );
        // Values of other groups are ignored.
        assert_eq!(e.get("Exec"), Some("nautilus --new-window %U"));
    }

    #[test]
    fn localized_key_prefix_is_not_a_language() {
        let e =
            DesktopEntry::parse("[Desktop Entry]\nName[fry]=X\nName[fr@latin]=Y\n").expect("entry");
        assert_eq!(e.localized("Name", "fr"), Some("Y"));
    }

    #[test]
    fn no_desktop_entry_group() {
        assert!(DesktopEntry::parse("[Other]\nName=x\n").is_none());
        assert!(DesktopEntry::parse("").is_none());
    }

    #[test]
    fn show_in_rules() {
        let e = DesktopEntry::parse(FILES).expect("entry");
        assert!(e.shown_in(&["GNOME".into()]));
        assert!(e.shown_in(&["ubuntu".into(), "gnome".into()]));
        assert!(!e.shown_in(&["KDE".into()]));
        assert!(!e.shown_in(&[]));
        let kde_hidden = DesktopEntry::parse("[Desktop Entry]\nNotShowIn=KDE;\n").expect("entry");
        assert!(kde_hidden.shown_in(&[]));
        assert!(!kde_hidden.shown_in(&["KDE".into()]));
    }

    #[test]
    fn string_escapes() {
        let e = DesktopEntry::parse("[Desktop Entry]\nName=a\\sb\\\\c\\;d\n").expect("entry");
        assert_eq!(e.get("Name"), Some("a b\\c\\;d"));
    }

    fn exec(line: &str) -> Option<Vec<String>> {
        parse_exec(
            line,
            &ExecContext {
                icon: Some("gimp"),
                name: "GIMP",
                location: "/usr/share/applications/gimp.desktop",
            },
        )
    }

    #[test]
    fn exec_field_codes() {
        assert_eq!(
            exec("nautilus --new-window %U"),
            Some(vec!["nautilus".into(), "--new-window".into()])
        );
        assert_eq!(
            exec("gimp %i -n %c %k"),
            Some(
                [
                    "gimp",
                    "--icon",
                    "gimp",
                    "-n",
                    "GIMP",
                    "/usr/share/applications/gimp.desktop"
                ]
                .map(String::from)
                .to_vec()
            )
        );
        assert_eq!(
            exec("app --rate=100%% x%fy"),
            Some(vec!["app".into(), "--rate=100%".into(), "xy".into()])
        );
    }

    #[test]
    fn exec_quoting() {
        assert_eq!(
            exec(r#""/opt/My App/run" --title "a \"b\" \\ \$c" %F"#),
            Some(vec![
                "/opt/My App/run".into(),
                "--title".into(),
                r#"a "b" \ $c"#.into()
            ])
        );
        assert_eq!(
            exec(r#"sh -c "echo ''""#),
            Some(vec!["sh".into(), "-c".into(), "echo ''".into()])
        );
        assert_eq!(exec(r#"x """#), Some(vec!["x".into(), String::new()]));
        assert_eq!(exec(r#"broken "quote"#), None);
        assert_eq!(exec("   "), None);
        assert_eq!(exec("%U"), None);
    }

    #[test]
    fn flatpak_exec_line() {
        let line = "/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=firefox --file-forwarding org.mozilla.firefox @@u %u @@";
        let args = exec(line).expect("args");
        assert_eq!(args[0], "/usr/bin/flatpak");
        assert_eq!(args.last().map(String::as_str), Some("@@"));
        assert!(!args.iter().any(|a| a.contains('%')));
    }
}

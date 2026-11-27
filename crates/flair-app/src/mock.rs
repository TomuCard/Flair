//! Mock results for the UI tests: fixed apps, files and calculations, so the
//! interface and keyboard navigation are tested without the real sources.

//! interface and keyboard navigation are tested without the real sources.

use std::path::PathBuf;
use std::sync::Arc;

use flair_core::i18n::I18n;
use flair_core::result::BuiltinIcon;
use flair_core::{
    Action, CancellationToken, IconSource, LaunchTarget, Provider, ProviderKind, Query, ResultItem,
};

/// Fake apps: (French name, English name).
const APPS: &[(&str, &str)] = &[
    ("Visual Studio Code", "Visual Studio Code"),
    ("Firefox", "Firefox"),
    ("Fichiers", "Files"),
    ("Terminal", "Terminal"),
    ("Calculatrice", "Calculator"),
    ("Paramètres", "Settings"),
    ("Explorateur de fichiers", "File Explorer"),
    ("Gestionnaire des tâches", "Task Manager"),
    ("Spotify", "Spotify"),
    ("Discord", "Discord"),
    ("GIMP", "GIMP"),
    ("LibreOffice Writer", "LibreOffice Writer"),
];

/// Fake files: (name, parent folder, is folder).
const FILES: &[(&str, &str, bool)] = &[
    ("vsc-backup.zip", "~/Documents/archives", false),
    ("rapport 2024.pdf", "~/Documents", false),
    ("notes.md", "~/Documents", false),
    ("photo-vacances.jpg", "~/Images", false),
    ("Projets", "~", true),
    ("flair", "~/code", true),
    ("Factures", "~/Documents", true),
];

/// Fake calculations, to show the calculator layout: (input, results).
const CALCS: &[(&str, &[&str])] = &[
    ("5h36 + 65min", &["6h41", "401 min", "6.683 h"]),
    ("15% de 240", &["36"]),
    ("15% of 240", &["36"]),
    ("1/3 + 1/6", &["1/2", "0.5"]),
    ("255 en hex", &["0xff", "0b11111111", "0o377"]),
];

pub struct MockProvider {
    i18n: Arc<I18n>,
    max_results: usize,
}

impl MockProvider {
    pub fn new(i18n: Arc<I18n>, max_results: usize) -> Self {
        Self { i18n, max_results }
    }

    fn apps(&self, text: &str, out: &mut Vec<ResultItem>) {
        for (fr, en) in APPS {
            // Both names are searchable; the interface language picks the title.
            let title = match self.i18n.lang() {
                flair_core::i18n::Lang::Fr => *fr,
                flair_core::i18n::Lang::En => *en,
            };
            let other = if title == *fr { *en } else { *fr };
            let found = fuzzy(text, title).or_else(|| fuzzy(text, other).map(|(s, _)| (s, vec![])));
            if let Some((score, indices)) = found {
                out.push(ResultItem {
                    id: format!("app:{en}"),
                    title: title.to_owned(),
                    subtitle: Some(self.i18n.t("kind-application")),
                    icon: IconSource::Builtin(BuiltinIcon::App),
                    score: score + ProviderKind::App.rank_bonus(),
                    match_indices: indices,
                    actions: vec![Action::Launch {
                        target: LaunchTarget::Path {
                            path: PathBuf::from(en),
                            args: vec![],
                        },
                    }],
                    provider: ProviderKind::App,
                });
            }
        }
    }

    fn files(&self, text: &str, out: &mut Vec<ResultItem>) {
        for (name, parent, is_dir) in FILES {
            if let Some((score, indices)) = fuzzy(text, name) {
                let path = PathBuf::from(parent).join(name);
                let (icon, bonus) = if *is_dir {
                    (BuiltinIcon::Folder, 0.75)
                } else {
                    (BuiltinIcon::File, 0.5)
                };
                out.push(ResultItem {
                    id: format!("file:{}", path.display()),
                    title: (*name).to_owned(),
                    subtitle: Some((*parent).to_owned()),
                    icon: IconSource::Builtin(icon),
                    score: score + bonus,
                    match_indices: indices,
                    actions: vec![
                        Action::Open { path: path.clone() },
                        Action::RevealInFolder { path },
                    ],
                    provider: ProviderKind::File,
                });
            }
        }
    }

    fn calc(&self, text: &str, out: &mut Vec<ResultItem>) {
        let Some((input, results)) = CALCS.iter().find(|(input, _)| *input == text) else {
            return;
        };
        for (i, value) in results.iter().enumerate() {
            let main = i == 0;
            out.push(ResultItem {
                id: if main {
                    flair_providers::calc::MAIN_ID.to_owned()
                } else {
                    format!("calc:{i}")
                },
                title: (*value).to_owned(),
                // Only the main result shows the expression under it.
                subtitle: main.then(|| (*input).to_owned()),
                icon: IconSource::Builtin(BuiltinIcon::Calculator),
                score: 100.0 - i as f32,
                match_indices: vec![],
                actions: vec![
                    Action::CopyToClipboard {
                        text: (*value).to_owned(),
                    },
                    Action::CopyToClipboard {
                        text: format!("{input} = {value}"),
                    },
                ],
                provider: ProviderKind::Calculator,
            });
        }
    }

    fn web(&self, text: &str, out: &mut Vec<ResultItem>) {
        out.push(ResultItem {
            id: "web:default".into(),
            title: self.i18n.t_args("web-search", &[("query", text)]),
            subtitle: None,
            icon: IconSource::Builtin(BuiltinIcon::Web),
            score: f32::MIN,
            match_indices: vec![],
            actions: vec![Action::OpenUrl {
                url: format!("https://duckduckgo.com/?q={}", text.replace(' ', "+")),
            }],
            provider: ProviderKind::Web,
        });
    }
}

impl Provider for MockProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::App
    }

    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem> {
        let mut out = Vec::new();
        if q.is_empty() || cancel.is_cancelled() {
            return out;
        }
        if q.accepts(ProviderKind::Calculator) {
            self.calc(&q.text, &mut out);
        }
        if q.accepts(ProviderKind::App) {
            self.apps(&q.text, &mut out);
        }
        if q.accepts(ProviderKind::File) {
            self.files(&q.text, &mut out);
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(self.max_results.saturating_sub(1).max(1));
        if q.accepts(ProviderKind::Web) {
            self.web(&q.text, &mut out);
        }
        out
    }
}

/// Minimal fuzzy matcher for mock data (the real ranking uses nucleo-matcher,
/// milestone 4). Tries, in order: prefix, word initials (`vsc` → **V**isual
/// **S**tudio **C**ode), then any subsequence. Returns a score and the matched
/// char indices.
fn fuzzy(query: &str, title: &str) -> Option<(f32, Vec<u32>)> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return None;
    }
    let t: Vec<char> = title.to_lowercase().chars().collect();
    let len = q.len() as u32;

    if t.iter()
        .filter(|c| !c.is_whitespace())
        .take(q.len())
        .eq(q.iter())
        && !t[0].is_whitespace()
    {
        let mut indices = Vec::new();
        let mut qi = 0;
        for (i, c) in t.iter().enumerate() {
            if qi < q.len() && *c == q[qi] {
                indices.push(i as u32);
                qi += 1;
            }
        }
        return Some((10.0 + len as f32, indices));
    }

    let starts: Vec<usize> = (0..t.len())
        .filter(|&i| t[i].is_alphanumeric() && (i == 0 || !t[i - 1].is_alphanumeric()))
        .collect();
    if starts.len() >= q.len() {
        let mut indices = Vec::new();
        let mut qi = 0;
        for &i in &starts {
            if qi < q.len() && t[i] == q[qi] {
                indices.push(i as u32);
                qi += 1;
            }
        }
        if qi == q.len() {
            // Same bonus as a prefix: the spec rewards matching word starts.
            return Some((10.0 + len as f32, indices));
        }
    }

    let mut indices = Vec::new();
    let mut qi = 0;
    for (i, c) in t.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            indices.push(i as u32);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| (len as f32 / t.len() as f32, indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flair_core::i18n::Lang;

    #[test]
    fn initials_match_highlights_word_starts() {
        let (_, indices) = fuzzy("vsc", "Visual Studio Code").expect("match");
        assert_eq!(indices, [0, 7, 14]);
    }

    #[test]
    fn prefix_beats_subsequence() {
        let (prefix, _) = fuzzy("fi", "Firefox").expect("match");
        let (sub, _) = fuzzy("fi", "Fichiers").expect("match");
        let (loose, _) = fuzzy("fx", "Firefox").expect("match");
        assert!(prefix >= sub && sub > loose);
        assert!(fuzzy("zz", "Firefox").is_none());
    }

    #[test]
    fn vsc_finds_the_app_first_then_the_file_then_web() {
        let p = MockProvider::new(Arc::new(I18n::new(Lang::Fr)), 8);
        let r = p.query(&Query::parse("vsc"), &CancellationToken::new());
        assert_eq!(r[0].title, "Visual Studio Code");
        assert_eq!(r[1].title, "vsc-backup.zip");
        assert_eq!(r.last().map(|i| i.provider), Some(ProviderKind::Web));
    }

    #[test]
    fn both_languages_find_apps() {
        let p = MockProvider::new(Arc::new(I18n::new(Lang::Fr)), 8);
        let r = p.query(&Query::parse("files"), &CancellationToken::new());
        assert_eq!(r[0].title, "Fichiers");
    }

    #[test]
    fn calc_prefix_filters_and_cancel_stops() {
        let p = MockProvider::new(Arc::new(I18n::new(Lang::Fr)), 8);
        let r = p.query(&Query::parse("= 5h36 + 65min"), &CancellationToken::new());
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].title, "6h41");
        let token = CancellationToken::new();
        token.cancel();
        assert!(p.query(&Query::parse("vsc"), &token).is_empty());
    }
}

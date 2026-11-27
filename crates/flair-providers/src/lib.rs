//! Result sources implementing [`flair_core::Provider`], and [`Providers`],
//! which merges them into the list shown by the interface.
//!
//! Milestone 4: applications; the calculator follows, files come with
//! milestone 6, web and system commands with milestone 7.

pub mod apps;
pub mod calc;

use std::sync::Arc;

use flair_core::result::BuiltinIcon;
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem,
};

/// All sources, queried together, ranked and cut to the list size.
pub struct Providers {
    sources: Vec<Arc<dyn Provider>>,
    max_results: usize,
    /// Label of the "search the web" row (`{ $query }` already replaced by
    /// `%s`), shown last; `None` when web search is disabled.
    web_label: Option<String>,
}

impl Providers {
    pub fn new(
        sources: Vec<Arc<dyn Provider>>,
        max_results: usize,
        web_label: Option<String>,
    ) -> Self {
        Self {
            sources,
            max_results: max_results.max(1),
            web_label,
        }
    }

    /// Web search row: always offered last, until the web provider of
    /// milestone 7 (configurable engines and keywords).
    fn web(&self, label: &str, text: &str) -> ResultItem {
        let encoded: String = text
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                b' ' => "+".into(),
                _ => format!("%{b:02X}"),
            })
            .collect();
        ResultItem {
            id: "web:default".into(),
            title: label.replace("%s", text),
            subtitle: None,
            icon: IconSource::Builtin(BuiltinIcon::Web),
            score: f32::MIN,
            match_indices: vec![],
            actions: vec![Action::OpenUrl {
                url: format!("https://duckduckgo.com/?q={encoded}"),
            }],
            provider: ProviderKind::Web,
        }
    }
}

impl Provider for Providers {
    fn kind(&self) -> ProviderKind {
        ProviderKind::App
    }

    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem> {
        if q.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<ResultItem> = Vec::new();
        for source in &self.sources {
            if cancel.is_cancelled() {
                return Vec::new();
            }
            if q.accepts(source.kind()) {
                out.extend(source.query(q, cancel));
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        let web = self
            .web_label
            .as_deref()
            .filter(|_| q.accepts(ProviderKind::Web));
        out.truncate(
            if web.is_some() {
                self.max_results - 1
            } else {
                self.max_results
            }
            .max(1),
        );
        if let Some(label) = web {
            out.push(self.web(label, &q.text));
        }
        out
    }

    fn refresh(&self) {
        for source in &self.sources {
            source.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Vec<f32>);

    impl Provider for Fixed {
        fn kind(&self) -> ProviderKind {
            ProviderKind::App
        }

        fn query(&self, _q: &Query, _cancel: &CancellationToken) -> Vec<ResultItem> {
            self.0
                .iter()
                .map(|&score| ResultItem {
                    id: format!("{score}"),
                    title: format!("{score}"),
                    subtitle: None,
                    icon: IconSource::Builtin(BuiltinIcon::App),
                    score,
                    match_indices: vec![],
                    actions: vec![],
                    provider: ProviderKind::App,
                })
                .collect()
        }
    }

    fn providers(max: usize) -> Providers {
        Providers::new(
            vec![
                Arc::new(Fixed(vec![1.0, 3.0])),
                Arc::new(Fixed(vec![2.0, 0.5])),
            ],
            max,
            Some("Search the web for “%s”".into()),
        )
    }

    #[test]
    fn merged_ranked_and_cut_with_the_web_last() {
        let r = providers(3).query(&Query::parse("a b&c"), &CancellationToken::new());
        let titles: Vec<&str> = r.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["3", "2", "Search the web for “a b&c”"]);
        assert_eq!(
            r[2].primary_action(),
            Some(&Action::OpenUrl {
                url: "https://duckduckgo.com/?q=a+b%26c".into()
            })
        );
    }

    #[test]
    fn prefixes_filter_sources() {
        let p = providers(8);
        let web_only = p.query(&Query::parse("? rust"), &CancellationToken::new());
        assert_eq!(web_only.len(), 1);
        assert_eq!(web_only[0].provider, ProviderKind::Web);
        // The calculator prefix excludes apps and the web.
        assert!(
            p.query(&Query::parse("= 1+1"), &CancellationToken::new())
                .is_empty()
        );
        assert!(
            p.query(&Query::parse(""), &CancellationToken::new())
                .is_empty()
        );
        let no_web = Providers::new(vec![Arc::new(Fixed(vec![1.0]))], 8, None);
        assert_eq!(
            no_web
                .query(&Query::parse("a"), &CancellationToken::new())
                .len(),
            1
        );
    }
}

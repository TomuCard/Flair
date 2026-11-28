//! Result sources implementing [`flair_core::Provider`], and [`Providers`],
//! which merges them into the list shown by the interface.
//!

pub mod apps;
pub mod calc;
pub mod files;
pub mod system;
pub mod web;

use std::sync::Arc;

use flair_core::{CancellationToken, Provider, ProviderKind, Query, ResultItem};

/// All sources, queried together, ranked and cut to the list size.
pub struct Providers {
    sources: Vec<Arc<dyn Provider>>,
    max_results: usize,
    /// Web search, whose "search the web" row is offered last; `None` when
    /// web search is disabled.
    web: Option<Arc<web::WebProvider>>,
}

impl Providers {
    pub fn new(
        sources: Vec<Arc<dyn Provider>>,
        max_results: usize,
        web: Option<Arc<web::WebProvider>>,
    ) -> Self {
        Self {
            sources,
            max_results: max_results.max(1),
            web,
        }
    }
}

impl Provider for Providers {
    fn kind(&self) -> ProviderKind {
        ProviderKind::App
    }

    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem> {
        // `=` alone lists the calculation history.
        if q.is_empty() && q.filter != Some(ProviderKind::Calculator) {
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
        if let Some(web) = &self.web {
            out.extend(web.query(q, cancel));
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        // The fallback row, unless the web is already offered (`?`, keyword).
        let fallback = self
            .web
            .as_ref()
            .filter(|_| q.filter.is_none() && !out.iter().any(|r| r.provider == ProviderKind::Web))
            .and_then(|w| w.fallback(&q.text));
        out.truncate(
            if fallback.is_some() {
                self.max_results - 1
            } else {
                self.max_results
            }
            .max(1),
        );
        out.extend(fallback);
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
    use flair_core::result::BuiltinIcon;
    use flair_core::{Action, IconSource};

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
            Some(Arc::new(web::WebProvider::new(
                vec![web::Engine {
                    keyword: "ddg".into(),
                    name: "DuckDuckGo".into(),
                    url: "https://duckduckgo.com/?q={query}".into(),
                }],
                web::Texts {
                    search_with: "Search “%q” with %e".into(),
                    search_web: "Search the web for “%q”".into(),
                },
            ))),
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

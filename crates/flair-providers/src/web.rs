//! Web search: keywords of the configured engines (`g rust iced` searches
//! Google), every engine after `?`, and the "search the web" row offered last.

use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem,
};

/// One search engine of the configuration (`[[web]]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub keyword: String,
    pub name: String,
    /// `{query}` is replaced by the encoded query.
    pub url: String,
}

/// Texts of the rows (`%q`: query, `%e`: engine).
#[derive(Debug, Clone)]
pub struct Texts {
    /// `Rechercher « %q » avec %e`.
    pub search_with: String,
    /// `Rechercher « %q » sur le web`.
    pub search_web: String,
}

/// Percent-encoding of a query (UTF-8 bytes, spaces as `+`).
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn url(engine: &Engine, query: &str) -> String {
    engine.url.replace("{query}", &encode(query))
}

/// Keyword rows come before everything else; `?` rows after the keyword one.
const KEYWORD_SCORE: f32 = 80.0;

pub struct WebProvider {
    engines: Vec<Engine>,
    texts: Texts,
}

impl WebProvider {
    pub fn new(engines: Vec<Engine>, texts: Texts) -> Self {
        Self { engines, texts }
    }

    /// The row searching `query` with `engine`.
    pub fn row(&self, engine: &Engine, query: &str, score: f32) -> ResultItem {
        ResultItem {
            id: format!("web:{}", engine.keyword),
            title: self
                .texts
                .search_with
                .replace("%q", query)
                .replace("%e", &engine.name),
            subtitle: Some(format!("{} {query}", engine.keyword)),
            icon: IconSource::Builtin(BuiltinIcon::Web),
            score,
            match_indices: Vec::new(),
            actions: vec![
                Action::OpenUrl {
                    url: url(engine, query),
                },
                Action::CopyToClipboard {
                    text: url(engine, query),
                },
            ],
            provider: ProviderKind::Web,
        }
    }

    /// "Search the web" with the first engine, offered last.
    pub fn fallback(&self, query: &str) -> Option<ResultItem> {
        let engine = self.engines.first()?;
        let mut row = self.row(engine, query, f32::MIN);
        row.id = "web:default".into();
        row.subtitle = Some(engine.name.clone());
        row.title = self.texts.search_web.replace("%q", query);
        Some(row)
    }
}

impl Provider for WebProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Web
    }

    fn query(&self, q: &Query, _cancel: &CancellationToken) -> Vec<ResultItem> {
        if q.is_empty() || !q.accepts(ProviderKind::Web) {
            return Vec::new();
        }
        let text = q.text.as_str();
        let (first, rest) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(f, r)| (f, r.trim()));
        let keyword = self
            .engines
            .iter()
            .find(|e| e.keyword.eq_ignore_ascii_case(first));
        // `?`: every engine (the keyword one first if typed).
        if q.filter == Some(ProviderKind::Web) {
            return match keyword {
                Some(engine) if !rest.is_empty() => vec![self.row(engine, rest, KEYWORD_SCORE)],
                _ =>
                {
                    #[allow(clippy::cast_precision_loss)]
                    self.engines
                        .iter()
                        .enumerate()
                        .map(|(i, e)| self.row(e, text, KEYWORD_SCORE - i as f32))
                        .collect()
                }
            };
        }
        match keyword {
            Some(engine) if !rest.is_empty() => vec![self.row(engine, rest, KEYWORD_SCORE)],
            _ => Vec::new(),
        }
    }
}

/// Engines whose name or keyword matches (`> web` lists them).
pub fn matching<'a>(engines: &'a [Engine], text: &str) -> Vec<&'a Engine> {
    if text.trim().is_empty() {
        return engines.iter().collect();
    }
    let prepared = Prepared::new(text);
    let mut ranker = Ranker::new();
    engines
        .iter()
        .filter(|e| {
            e.keyword.eq_ignore_ascii_case(text.trim())
                || ranker.match_text(&prepared, &e.name).is_some()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> WebProvider {
        WebProvider::new(
            vec![
                Engine {
                    keyword: "g".into(),
                    name: "Google".into(),
                    url: "https://www.google.com/search?q={query}".into(),
                },
                Engine {
                    keyword: "yt".into(),
                    name: "YouTube".into(),
                    url: "https://www.youtube.com/results?search_query={query}".into(),
                },
            ],
            Texts {
                search_with: "Search “%q” with %e".into(),
                search_web: "Search the web for “%q”".into(),
            },
        )
    }

    fn query(text: &str) -> Vec<ResultItem> {
        provider().query(&Query::parse(text), &CancellationToken::new())
    }

    #[test]
    fn keywords_open_their_engine_with_an_encoded_query() {
        let rows = query("g rust iced");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Search “rust iced” with Google");
        assert_eq!(
            rows[0].primary_action(),
            Some(&Action::OpenUrl {
                url: "https://www.google.com/search?q=rust+iced".into()
            })
        );
        assert_eq!(
            url(&provider().engines[1], "été & co"),
            "https://www.youtube.com/results?search_query=%C3%A9t%C3%A9+%26+co"
        );
        assert!(query("g").is_empty(), "keyword alone");
        assert!(query("golang tips").is_empty(), "not a keyword");
        assert_eq!(query("YT lofi").len(), 1);
    }

    #[test]
    fn question_mark_lists_every_engine() {
        let rows = query("? rust");
        assert_eq!(rows.len(), 2);
        assert!(rows[0].score > rows[1].score);
        assert_eq!(query("?yt lofi").len(), 1);
        let fallback = provider().fallback("x y").expect("row");
        assert_eq!(fallback.title, "Search the web for “x y”");
        assert_eq!(fallback.score, f32::MIN);
    }
}

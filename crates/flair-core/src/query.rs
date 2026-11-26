//! User query and optional filtering prefixes.

use crate::result::ProviderKind;

/// A query typed in the search field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// Exactly what the user typed.
    pub raw: String,
    /// Text without the filtering prefix, trimmed.
    pub text: String,
    /// Source restriction requested by a prefix, if any.
    pub filter: Option<ProviderKind>,
}

impl Query {
    /// Parses a raw input, recognizing the optional filtering prefixes:
    ///
    /// | Prefix | Filter |
    /// |---|---|
    /// | `=` | calculator |
    /// | `/` or `~` | files and paths (the prefix is kept: it is part of the path) |
    /// | `>` | commands |
    /// | `?` | web search |
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim_start();
        let (filter, text) = match trimmed.chars().next() {
            Some('=') => (Some(ProviderKind::Calculator), &trimmed[1..]),
            Some('>') => (Some(ProviderKind::Command), &trimmed[1..]),
            Some('?') => (Some(ProviderKind::Web), &trimmed[1..]),
            // Paths keep their leading character: `/etc` and `~/Doc` are real paths.
            Some('/' | '~') => (Some(ProviderKind::File), trimmed),
            _ => (None, trimmed),
        };
        Self {
            raw: raw.to_owned(),
            text: text.trim().to_owned(),
            filter,
        }
    }

    /// True when nothing meaningful was typed (prefix alone counts as empty text).
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// True if the given source may answer this query.
    pub fn accepts(&self, kind: ProviderKind) -> bool {
        self.filter.is_none_or(|f| f == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_prefix() {
        let q = Query::parse("  firefox ");
        assert_eq!(q.text, "firefox");
        assert_eq!(q.filter, None);
        assert!(q.accepts(ProviderKind::App));
    }

    #[test]
    fn calc_prefix() {
        let q = Query::parse("= 15% de 240");
        assert_eq!(q.filter, Some(ProviderKind::Calculator));
        assert_eq!(q.text, "15% de 240");
        assert!(!q.accepts(ProviderKind::App));
    }

    #[test]
    fn command_and_web_prefixes() {
        assert_eq!(Query::parse(">help").filter, Some(ProviderKind::Command));
        assert_eq!(Query::parse("> aide").text, "aide");
        assert_eq!(Query::parse("? rust iced").filter, Some(ProviderKind::Web));
    }

    #[test]
    fn path_prefixes_keep_the_path() {
        let q = Query::parse("~/Doc");
        assert_eq!(q.filter, Some(ProviderKind::File));
        assert_eq!(q.text, "~/Doc");
        assert_eq!(Query::parse("/etc/").text, "/etc/");
    }

    #[test]
    fn prefix_alone_is_empty() {
        assert!(Query::parse("=").is_empty());
        assert!(Query::parse(">  ").is_empty());
        assert!(!Query::parse("x").is_empty());
    }
}

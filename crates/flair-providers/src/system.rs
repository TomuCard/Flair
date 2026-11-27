//! System commands found without the `>` prefix (`verrouiller`, `lock`,
//! `éteindre`…). Their words come from the language files, through the
//! application; the destructive ones are confirmed by a second `Enter`.

use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::{
    Action, CancellationToken, IconSource, Provider, ProviderKind, Query, ResultItem, SystemCommand,
};

/// One system command and the words that find it.
#[derive(Debug, Clone)]
pub struct Entry {
    pub command: SystemCommand,
    pub title: String,
    pub subtitle: String,
    pub words: Vec<String>,
}

pub fn icon(command: SystemCommand) -> BuiltinIcon {
    match command {
        SystemCommand::Lock => BuiltinIcon::Lock,
        SystemCommand::Sleep => BuiltinIcon::Sleep,
        SystemCommand::Shutdown => BuiltinIcon::Power,
        SystemCommand::Restart => BuiltinIcon::Restart,
        SystemCommand::LogOut => BuiltinIcon::LogOut,
        SystemCommand::EmptyTrash => BuiltinIcon::Trash,
    }
}

/// The row of a system command.
pub fn row(entry: &Entry, score: f32, indices: Vec<u32>) -> ResultItem {
    ResultItem {
        id: format!("system:{:?}", entry.command).to_lowercase(),
        title: entry.title.clone(),
        subtitle: Some(entry.subtitle.clone()),
        icon: IconSource::Builtin(icon(entry.command)),
        score,
        match_indices: indices,
        actions: vec![Action::SystemCommand(entry.command)],
        provider: ProviderKind::System,
    }
}

pub struct SystemProvider {
    entries: Vec<Entry>,
}

impl SystemProvider {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self { entries }
    }
}

impl Provider for SystemProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::System
    }

    fn query(&self, q: &Query, _cancel: &CancellationToken) -> Vec<ResultItem> {
        // Two letters at least: `v` alone should not offer to lock.
        if q.filter.is_some() || q.text.chars().count() < 2 {
            return Vec::new();
        }
        let prepared = Prepared::new(&q.text);
        let mut ranker = Ranker::new();
        let typed = flair_core::lexicon::fold(&q.text);
        self.entries
            .iter()
            .filter_map(|e| {
                // Prefix of one of its words (`verr`, `shut d`), not scattered
                // letters: a system command must be meant.
                let meant = e
                    .words
                    .iter()
                    .any(|w| flair_core::lexicon::fold(w).starts_with(&typed));
                if !meant {
                    return None;
                }
                let m = ranker.match_names(&prepared, &e.title, e.words.iter().map(String::as_str));
                let (score, indices) = m.map_or((1.0, Vec::new()), |m| (m.score, m.indices));
                Some(row(e, score + ProviderKind::System.rank_bonus(), indices))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> SystemProvider {
        SystemProvider::new(vec![
            Entry {
                command: SystemCommand::Lock,
                title: "Verrouiller la session".into(),
                subtitle: "Système".into(),
                words: vec!["verrouiller".into(), "lock".into()],
            },
            Entry {
                command: SystemCommand::Shutdown,
                title: "Éteindre".into(),
                subtitle: "Système".into(),
                words: vec!["éteindre".into(), "shutdown".into(), "power off".into()],
            },
        ])
    }

    fn found(text: &str) -> Vec<String> {
        provider()
            .query(&Query::parse(text), &CancellationToken::new())
            .into_iter()
            .map(|r| r.id)
            .collect()
    }

    #[test]
    fn found_by_the_start_of_their_words() {
        assert_eq!(found("verr"), ["system:lock"]);
        assert_eq!(found("lock"), ["system:lock"]);
        assert_eq!(found("eteindre"), ["system:shutdown"]);
        assert_eq!(found("power"), ["system:shutdown"]);
        assert!(found("l").is_empty());
        assert!(found("vlc").is_empty());
        assert!(found("> lock").is_empty(), "the `>` list has its own rows");
    }
}

//! Result items shown in the list and the actions they carry.

use std::path::PathBuf;

/// Kind of source that produced a result. Also used by filtering prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    App,
    File,
    Calculator,
    Web,
    System,
    Command,
}

impl ProviderKind {
    /// Ranking bonus at equal match score: apps > folders > files (folders are
    /// distinguished by the file provider itself).
    pub fn rank_bonus(self) -> f32 {
        match self {
            Self::Calculator => 3.0,
            Self::App => 2.0,
            Self::Command | Self::System => 1.0,
            Self::File => 0.5,
            Self::Web => 0.0,
        }
    }
}

/// Where an icon comes from. Loading is always done off the UI thread.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconSource {
    /// Image file on disk (PNG, SVG, ICO…).
    Path(PathBuf),
    /// Already decoded RGBA pixels.
    Rgba {
        width: u32,
        height: u32,
        pixels: std::sync::Arc<[u8]>,
    },
    /// System icon name (freedesktop icon theme) or Windows shell item.
    System(String),
    /// One of Flair's own monochrome UI icons.
    Builtin(BuiltinIcon),
}

/// Monochrome UI icons embedded in the binary (Lucide set + Flair logo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinIcon {
    App,
    File,
    Folder,
    Calculator,
    Web,
    Command,
    Settings,
    Lock,
    Sleep,
    Power,
    Restart,
    LogOut,
    Trash,
}

/// What to start for [`Action::Launch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// Executable or shortcut path, with arguments.
    Path { path: PathBuf, args: Vec<String> },
    /// Linux `.desktop` entry (file path), launched with its `Exec` line.
    DesktopEntry(PathBuf),
    /// Windows Store app, launched by its AppUserModelID.
    AppUserModelId(String),
}

/// System commands (section 5.5 of the spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemCommand {
    Lock,
    Sleep,
    Shutdown,
    Restart,
    LogOut,
    EmptyTrash,
}

impl SystemCommand {
    /// Destructive commands require a confirmation (second `Enter`).
    pub fn needs_confirmation(self) -> bool {
        matches!(
            self,
            Self::Shutdown | Self::Restart | Self::LogOut | Self::EmptyTrash
        )
    }
}

/// Something a result can do. `actions[0]` is the main (`Enter`) action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Launch {
        target: LaunchTarget,
    },
    /// Windows: start as administrator (`runas`).
    LaunchAsAdmin {
        target: LaunchTarget,
    },
    Open {
        path: PathBuf,
    },
    RevealInFolder {
        path: PathBuf,
    },
    CopyToClipboard {
        text: String,
    },
    OpenUrl {
        url: String,
    },
    SystemCommand(SystemCommand),
    /// One of Flair's own commands (`> update`, `> export`…), identified by a
    /// stable id interpreted by the application.
    Internal {
        command: String,
    },
}

impl Action {
    /// Translation id of the short label shown at the right of the selected row
    /// (`↵ Ouvrir`, `↵ Copier`).
    pub fn label_id(&self) -> &'static str {
        match self {
            Self::Launch { .. } | Self::Open { .. } | Self::OpenUrl { .. } => "action-open",
            Self::LaunchAsAdmin { .. } => "action-run-admin",
            Self::RevealInFolder { .. } => "action-reveal",
            Self::CopyToClipboard { .. } => "action-copy",
            Self::SystemCommand(_) | Self::Internal { .. } => "action-run",
        }
    }
}

/// One line of the result list.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultItem {
    /// Stable id, used for history and frecency.
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: IconSource,
    pub score: f32,
    /// Indices (in chars) of `title` that match the query, highlighted in the UI.
    pub match_indices: Vec<u32>,
    /// `[0]` is the main action.
    pub actions: Vec<Action>,
    pub provider: ProviderKind,
}

impl ResultItem {
    /// The main (`Enter`) action, if any.
    pub fn primary_action(&self) -> Option<&Action> {
        self.actions.first()
    }

    /// The secondary (`Ctrl+Enter`) action, if any.
    pub fn secondary_action(&self) -> Option<&Action> {
        self.actions.get(1)
    }

    /// Text copied by `Ctrl+C`: the path or value behind the result.
    pub fn copy_text(&self) -> String {
        for action in &self.actions {
            match action {
                Action::CopyToClipboard { text } => return text.clone(),
                Action::Open { path } | Action::RevealInFolder { path } => {
                    return path.display().to_string();
                }
                Action::Launch {
                    target: LaunchTarget::Path { path, .. } | LaunchTarget::DesktopEntry(path),
                } => return path.display().to_string(),
                Action::Launch {
                    target: LaunchTarget::AppUserModelId(id),
                } => return id.clone(),
                Action::OpenUrl { url } => return url.clone(),
                _ => {}
            }
        }
        self.title.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_commands_need_confirmation() {
        assert!(SystemCommand::Shutdown.needs_confirmation());
        assert!(SystemCommand::EmptyTrash.needs_confirmation());
        assert!(!SystemCommand::Lock.needs_confirmation());
        assert!(!SystemCommand::Sleep.needs_confirmation());
    }

    #[test]
    fn copy_text_prefers_value_then_path() {
        let item = ResultItem {
            id: "x".into(),
            title: "6h41".into(),
            subtitle: None,
            icon: IconSource::Builtin(BuiltinIcon::Calculator),
            score: 0.0,
            match_indices: vec![],
            actions: vec![Action::CopyToClipboard {
                text: "6h41".into(),
            }],
            provider: ProviderKind::Calculator,
        };
        assert_eq!(item.copy_text(), "6h41");
        let file = ResultItem {
            actions: vec![Action::Open {
                path: PathBuf::from("/tmp/a.txt"),
            }],
            ..item
        };
        assert_eq!(file.copy_text(), "/tmp/a.txt");
    }
}

//! What the file index leaves out: patterns of the configuration
//! (`node_modules`, `target`…), noisy system folders built in, hidden files.
//!
//! A pattern without separator matches a file or folder name anywhere
//! (`node_modules`, `*.tmp`); with separators it matches the end of a path
//! (`AppData/Roaming/*/Cache`); starting with `/` it is an absolute path
//! (`/proc`). `*` matches any part of one name. Case is ignored.

use std::path::{Component, Path};

/// Built in, besides the configured patterns: caches, trash, temporary and
/// system folders that only bring noise and wake-ups.
#[cfg(windows)]
const BUILTIN: &[&str] = &[
    "AppData/Local",
    "AppData/LocalLow",
    "AppData/Roaming/*/Cache",
    "AppData/Roaming/*/*/Cache",
    "$Recycle.Bin",
    "System Volume Information",
    "cache2",
    "GPUCache",
    "Code Cache",
];
#[cfg(not(windows))]
const BUILTIN: &[&str] = &[
    "/proc",
    "/sys",
    "/dev",
    "/run",
    "/tmp",
    "/var/tmp",
    ".local/share/Trash",
    "cache2",
    "GPUCache",
    "Code Cache",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// A name anywhere.
    Name(String),
    /// The last components of a path.
    Tail(Vec<String>),
    /// An absolute path and everything under it.
    Absolute(Vec<String>),
}

/// The exclusion rules of an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    patterns: Vec<Pattern>,
    pub include_hidden: bool,
}

fn components(path: &str) -> Vec<String> {
    path.split(['/', '\\'])
        .filter(|c| !c.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// `*` matches any run of characters (case already lowered).
fn glob(pattern: &str, text: &str) -> bool {
    let Some((head, rest)) = pattern.split_once('*') else {
        return pattern == text;
    };
    let Some(mut text) = text.strip_prefix(head) else {
        return false;
    };
    let parts: Vec<&str> = rest.split('*').collect();
    let (last, middle) = parts.split_last().unwrap_or((&"", &[]));
    for part in middle {
        match text.find(part) {
            Some(at) => text = &text[at + part.len()..],
            None => return false,
        }
    }
    text.len() >= last.len() && text.ends_with(last)
}

impl Rules {
    pub fn new(configured: &[String], include_hidden: bool) -> Self {
        let patterns = configured
            .iter()
            .map(String::as_str)
            .chain(BUILTIN.iter().copied())
            .filter(|p| !p.trim().is_empty())
            .map(|p| {
                let p = p.trim();
                let parts = components(p);
                if p.starts_with('/') {
                    Pattern::Absolute(parts)
                } else if parts.len() > 1 {
                    Pattern::Tail(parts)
                } else {
                    Pattern::Name(parts.concat())
                }
            })
            .collect();
        Self {
            patterns,
            include_hidden,
        }
    }

    /// True when `name`, in folder `parent`, is left out of the index.
    pub fn excluded(
        &self,
        parent: &Path,
        name: &str,
        metadata: Option<&std::fs::Metadata>,
    ) -> bool {
        if !self.include_hidden && flair_platform::fs::is_hidden(name, metadata) {
            return true;
        }
        let lower = name.to_lowercase();
        let mut path: Option<Vec<String>> = None;
        self.patterns.iter().any(|p| match p {
            Pattern::Name(n) => glob(n, &lower),
            Pattern::Tail(parts) | Pattern::Absolute(parts) => {
                let path = path.get_or_insert_with(|| {
                    let mut all: Vec<String> = parent
                        .components()
                        .filter_map(|c| match c {
                            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
                            _ => None,
                        })
                        .collect();
                    all.push(lower.clone());
                    all
                });
                match p {
                    Pattern::Absolute(_) => {
                        // `/proc` excludes `/proc` itself (its content is
                        // never reached).
                        path.len() == parts.len() && path.iter().zip(parts).all(|(c, g)| glob(g, c))
                    }
                    _ => {
                        path.len() >= parts.len()
                            && path[path.len() - parts.len()..]
                                .iter()
                                .zip(parts)
                                .all(|(c, g)| glob(g, c))
                    }
                }
            }
        })
    }

    /// True when some component of `path` below `root` is excluded.
    pub fn excludes_path(&self, root: &Path, path: &Path) -> bool {
        let Ok(below) = path.strip_prefix(root) else {
            return true;
        };
        let mut parent = root.to_path_buf();
        for component in below.components() {
            if let Component::Normal(name) = component
                && let Some(name) = name.to_str()
                && self.excluded(&parent, name, None)
            {
                return true;
            }
            parent.push(component);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules::new(
            &["node_modules".into(), "*.tmp".into(), "build/out".into()],
            false,
        )
    }

    #[test]
    fn globs() {
        assert!(glob("*.tmp", "a.tmp"));
        assert!(!glob("*.tmp", "a.tmpx"));
        assert!(glob("a*b*c", "axxbyyc"));
        assert!(glob("*", "anything"));
        assert!(!glob("cache", "caches"));
    }

    #[test]
    fn names_tails_and_hidden() {
        let r = rules();
        let home = Path::new("/home/me/project");
        assert!(r.excluded(home, "node_modules", None));
        assert!(r.excluded(home, "Node_Modules", None));
        assert!(r.excluded(home, "x.tmp", None));
        assert!(r.excluded(home, ".git", None));
        assert!(!r.excluded(home, "src", None));
        assert!(r.excluded(Path::new("/home/me/build"), "out", None));
        assert!(!r.excluded(Path::new("/home/me"), "out", None));
        assert!(Rules::new(&[], true).excluded(home, "cache2", None));
        assert!(!Rules::new(&[], true).excluded(home, ".config", None));
    }

    #[cfg(not(windows))]
    #[test]
    fn absolute_and_builtin() {
        let r = rules();
        assert!(r.excluded(Path::new("/"), "proc", None));
        assert!(!r.excluded(Path::new("/home"), "proc", None));
        assert!(r.excluded(Path::new("/home/me/.local/share"), "Trash", None));
        let home = Path::new("/home/me");
        assert!(r.excludes_path(home, Path::new("/home/me/project/node_modules/x/y.js")));
        assert!(!r.excludes_path(home, Path::new("/home/me/project/src/main.rs")));
        assert!(r.excludes_path(home, Path::new("/elsewhere/a.txt")));
    }
}

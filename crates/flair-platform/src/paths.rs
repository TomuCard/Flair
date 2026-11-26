//! Standard locations of Flair's files.
//!
//! | | Windows | Linux |
//! |---|---|---|
//! | config | `%AppData%\Flair\` | `~/.config/flair/` |
//! | data (index, frecency, logs, backups) | `%LocalAppData%\Flair\` | `~/.local/share/flair/` |
//! | cache (icons) | `%LocalAppData%\Flair\cache\` | `~/.cache/flair/` |

use std::path::{Path, PathBuf};

use directories::BaseDirs;

#[derive(Debug, thiserror::Error)]
#[error("cannot determine the home directory")]
pub struct NoHomeDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

#[cfg(windows)]
const APP_DIR: &str = "Flair";
#[cfg(not(windows))]
const APP_DIR: &str = "flair";

impl Paths {
    pub fn new() -> Result<Self, NoHomeDir> {
        let base = BaseDirs::new().ok_or(NoHomeDir)?;
        let data_dir = base.data_local_dir().join(APP_DIR);
        #[cfg(windows)]
        let cache_dir = data_dir.join("cache");
        #[cfg(not(windows))]
        let cache_dir = base.cache_dir().join(APP_DIR);
        Ok(Self {
            config_dir: base.config_dir().join(APP_DIR),
            data_dir,
            cache_dir,
        })
    }

    /// Paths rooted in a single directory (tests, portable mode).
    pub fn rooted(root: &Path) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    pub fn lexicon_dir(&self) -> PathBuf {
        self.config_dir.join("lexicon")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_layout() {
        let p = Paths::rooted(Path::new("/x"));
        assert_eq!(p.config_file(), Path::new("/x/config/config.toml"));
        assert_eq!(p.log_dir(), Path::new("/x/data/logs"));
    }

    #[test]
    fn standard_paths_end_with_the_app_dir() {
        if let Ok(p) = Paths::new() {
            assert!(p.config_dir.ends_with(APP_DIR));
            assert!(p.data_dir.ends_with(APP_DIR));
        }
    }
}

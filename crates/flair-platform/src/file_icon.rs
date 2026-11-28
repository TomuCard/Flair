//! Icons of files and folders, as the system shows them: the shell icon of
//! the type under Windows, the icon theme entry of the MIME type under Linux.

use std::path::Path;

use crate::image::IconRgba;

/// Files sharing a key share an icon (cache key): the type, or the file
/// itself when it carries its own icon (`.exe`, `.lnk` under Windows).
pub fn key(path: &Path, dir: bool) -> String {
    imp::key(path, dir)
}

/// The icon at `size` pixels. Slow (shell, disk): call it off the UI thread.
pub fn load(path: &Path, dir: bool, size: u32) -> Option<IconRgba> {
    imp::load(path, dir, size)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use crate::apps::{self, AppIcon};
    use crate::image::IconRgba;

    /// Types whose files each have their own icon.
    const OWN_ICON: &[&str] = &["exe", "lnk", "ico", "url", "appref-ms", "cpl", "scr"];

    pub fn key(path: &Path, dir: bool) -> String {
        let ext = super::extension(path);
        if dir {
            "dir".into()
        } else if OWN_ICON.contains(&ext.as_str()) {
            format!("path:{}", path.display())
        } else {
            format!("ext:{ext}")
        }
    }

    pub fn load(path: &Path, _dir: bool, size: u32) -> Option<IconRgba> {
        apps::load_icon(&AppIcon::Shell(path.display().to_string()), size)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::OnceLock;

    use crate::apps::linux::{find_themed, load_file};
    use crate::image::IconRgba;

    /// Extension → (MIME type, generic icon), from shared-mime-info.
    struct MimeDb {
        by_ext: HashMap<String, String>,
        generic: HashMap<String, String>,
    }

    fn db() -> &'static MimeDb {
        static DB: OnceLock<MimeDb> = OnceLock::new();
        DB.get_or_init(|| {
            let dirs = std::env::var("XDG_DATA_DIRS")
                .unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
            let mut by_ext = HashMap::new();
            let mut generic = HashMap::new();
            for dir in dirs.split(':').filter(|d| !d.is_empty()) {
                let mime = Path::new(dir).join("mime");
                if let Ok(text) = std::fs::read_to_string(mime.join("globs2")) {
                    parse_globs(&text, &mut by_ext);
                }
                if let Ok(text) = std::fs::read_to_string(mime.join("generic-icons")) {
                    for line in text.lines() {
                        if let Some((m, icon)) = line.split_once(':') {
                            generic
                                .entry(m.to_owned())
                                .or_insert_with(|| icon.to_owned());
                        }
                    }
                }
            }
            MimeDb { by_ext, generic }
        })
    }

    /// `50:application/pdf:*.pdf` lines (highest weight first in the file).
    fn parse_globs(text: &str, by_ext: &mut HashMap<String, String>) {
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let mut parts = line.split(':');
            let (Some(_), Some(mime), Some(glob)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let Some(ext) = glob.strip_prefix("*.")
                && !ext.contains(['*', '?', '['])
            {
                by_ext
                    .entry(ext.to_lowercase())
                    .or_insert_with(|| mime.to_owned());
            }
        }
    }

    /// Icon names to try, most specific first.
    fn names(path: &Path, dir: bool) -> Vec<String> {
        if dir {
            return vec!["folder".into()];
        }
        let db = db();
        let mut out = Vec::new();
        if let Some(mime) = db.by_ext.get(&super::extension(path)) {
            out.push(mime.replace('/', "-"));
            if let Some(g) = db.generic.get(mime) {
                out.push(g.clone());
            }
            if let Some((top, _)) = mime.split_once('/') {
                out.push(format!("{top}-x-generic"));
            }
        }
        out.push("text-x-generic".into());
        out
    }

    pub fn key(path: &Path, dir: bool) -> String {
        names(path, dir).join("|")
    }

    pub fn load(path: &Path, dir: bool, size: u32) -> Option<IconRgba> {
        names(path, dir)
            .iter()
            .find_map(|n| find_themed(n, size).and_then(|p| load_file(&p, size)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn globs_give_mime_types() {
            let mut by_ext = HashMap::new();
            parse_globs(
                "# comment\n50:application/pdf:*.pdf\n50:image/png:*.PNG\n10:text/x-c:*.[ch]\n50:application/x-tar:*.tar\n",
                &mut by_ext,
            );
            assert_eq!(
                by_ext.get("pdf").map(String::as_str),
                Some("application/pdf")
            );
            assert_eq!(by_ext.get("png").map(String::as_str), Some("image/png"));
            assert!(!by_ext.contains_key("[ch]"));
        }

        #[test]
        fn folders_and_unknown_types() {
            assert_eq!(key(Path::new("/x"), true), "folder");
            assert!(key(Path::new("/x/file.zzqq"), false).ends_with("text-x-generic"));
        }
    }
}

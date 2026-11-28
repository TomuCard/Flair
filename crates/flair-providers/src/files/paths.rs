//! Queries that look like a path (`~/Doc`, `/etc/`, `C:\Us`): the content of
//! the folder, read directly (not from the index), with completion by `Tab`.

use std::path::{Path, PathBuf};

/// True when the query is a path being typed.
pub fn is_path(text: &str) -> bool {
    let b = text.as_bytes();
    text.starts_with('~')
        || text.starts_with('/')
        || text.starts_with("\\\\")
        || (b.len() >= 2
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b.len() == 2 || matches!(b[2], b'\\' | b'/')))
}

/// `~` and `~/x` with the home folder.
pub fn expand(text: &str, home: Option<&Path>) -> Option<PathBuf> {
    match text.strip_prefix('~') {
        Some(rest) => {
            let home = home?;
            let rest = rest.trim_start_matches(['/', '\\']);
            Some(if rest.is_empty() {
                home.to_owned()
            } else {
                home.join(rest)
            })
        }
        None if text.len() == 2 && text.ends_with(':') => Some(PathBuf::from(format!("{text}\\"))),
        None => Some(PathBuf::from(text)),
    }
}

/// `/home/me/Documents` → `~/Documents` (shorter subtitles).
pub fn tilde(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        let rest = rest.to_string_lossy();
        if rest.is_empty() {
            return "~".into();
        }
        return format!("~{}{rest}", std::path::MAIN_SEPARATOR);
    }
    path.display().to_string()
}

/// One entry of a listed folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
}

/// The folder typed (`~/Documents/` lists it) or the entries of its parent
/// starting with the last part (`~/Doc` → `Documents`, `Docker`…), folders
/// first. Hidden entries only when the part starts with `.`.
pub fn list(text: &str, home: Option<&Path>, limit: usize) -> Vec<Listed> {
    // Split the text as typed (`Path` would drop a trailing `.`).
    let (dir_text, part) = match text.rfind(['/', '\\']) {
        Some(at) => (&text[..=at], &text[at + 1..]),
        None => (text, ""),
    };
    let Some(dir) = expand(dir_text, home) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let wanted = flair_core::lexicon::fold(part);
    let show_hidden = part.starts_with('.');
    let mut out: Vec<Listed> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            if !flair_core::lexicon::fold(&name).starts_with(&wanted) {
                return None;
            }
            // Links to folders count as folders here (navigation follows them).
            let dir = e.path().is_dir();
            Some(Listed {
                path: e.path(),
                name,
                dir,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_like_queries() {
        assert!(is_path("~/Doc"));
        assert!(is_path("~"));
        assert!(is_path("/etc/"));
        assert!(is_path("C:\\Us"));
        assert!(is_path("c:/users"));
        assert!(is_path("D:"));
        assert!(is_path("\\\\server\\share"));
        assert!(!is_path("rapport"));
        assert!(!is_path("a:b"));
        assert!(!is_path("12:30"));
    }

    #[test]
    fn expands_home() {
        let home = Path::new("/home/me");
        assert_eq!(expand("~", Some(home)), Some(PathBuf::from("/home/me")));
        assert_eq!(
            expand("~/Doc", Some(home)),
            Some(PathBuf::from("/home/me/Doc"))
        );
        assert_eq!(expand("~/Doc", None), None);
        assert_eq!(
            tilde(Path::new("/home/me/Documents"), Some(home)).replace('\\', "/"),
            "~/Documents"
        );
        assert_eq!(tilde(Path::new("/etc"), Some(home)), "/etc");
    }

    #[test]
    fn lists_a_folder() {
        let root = crate::files::walk::tests::tree("paths");
        let home = Some(root.as_path());
        let names = |text: &str| -> Vec<String> {
            list(text, home, 20).into_iter().map(|l| l.name).collect()
        };
        assert_eq!(
            names("~/"),
            ["Documents", "node_modules", "Photos", "notes.md"]
        );
        assert_eq!(names("~/doc"), ["Documents"]);
        assert_eq!(names("~/."), [".hidden"]);
        assert_eq!(names("~/Documents/"), ["Projets", "rapport.pdf"]);
        assert!(names("~/nothing/").is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}

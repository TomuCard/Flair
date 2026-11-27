//! `.flair` files: everything the user customized, in one ZIP archive of
//! readable text files. Used for `> export` / `> import`, for moving to
//! another PC, and for the automatic backups (before an update, a reset or an
//! import).
//!
//! ```text
//! manifest.json      format, Flair version, config_version, date, PC, OS, contents
//! config.toml        all settings (web engines included)
//! themes/*.toml      custom themes
//! lexicon/*.toml     personal language files
//! data/history.json  calculation history
//! data/frecency.json usage data
//! ```
//!
//! A `.flair` file is untrusted: only the paths above are read, any absolute
//! or `..` path makes the whole archive refused (zip slip), sizes are capped
//! (zip bombs), and every file is validated like a hand-written one. Nothing
//! is written before the whole archive has been read and checked.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::settings::config::CONFIG_VERSION;
use crate::settings::migrate;
use crate::theme::Theme;
use crate::time::UtcDateTime;

/// Archive format written by this version.
pub const FORMAT: u32 = 1;
/// Largest accepted file inside an archive, once decompressed.
pub const MAX_ENTRY_BYTES: u64 = 10 * 1024 * 1024;
/// Largest accepted archive, once decompressed.
pub const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;
const MAX_ENTRIES: usize = 2000;
/// Automatic backups kept in `backups/`.
pub const KEPT_BACKUPS: usize = 10;

const HISTORY: &str = "history.json";
const FRECENCY: &str = "frecency.json";

/// Where the user files live (from `flair_platform::paths`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub config_file: PathBuf,
    pub themes_dir: PathBuf,
    pub lexicon_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Home directory, written `~` in exported paths.
    pub home: PathBuf,
}

/// Facts about the exporting machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub flair_version: String,
    pub host: String,
    pub os: String,
}

impl Origin {
    pub fn current(flair_version: &str) -> Self {
        let host = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
            .or_else(|| fs::read_to_string("/etc/hostname").ok())
            .map(|h| h.trim().to_owned())
            .unwrap_or_default();
        let os = if cfg!(windows) { "Windows" } else { "Linux" };
        Self {
            flair_version: flair_version.to_owned(),
            host,
            os: os.to_owned(),
        }
    }
}

/// What an archive contains (`manifest.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contents {
    pub config: bool,
    pub themes: Vec<String>,
    pub lexicons: Vec<String>,
    pub history_entries: Option<usize>,
    pub frecency: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArchiveManifest {
    pub format: u32,
    pub flair_version: String,
    pub config_version: u32,
    pub date: String,
    pub host: String,
    pub os: String,
    pub settings_only: bool,
    pub contents: Contents,
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not a valid .flair file: {0}")]
    Invalid(String),
    #[error("unsafe path in the archive: {0}")]
    UnsafePath(String),
    #[error("the archive is too large once decompressed")]
    TooLarge,
    /// Exported by a more recent Flair: update first.
    #[error("this file comes from a more recent version of Flair")]
    Newer,
}

impl From<zip::result::ZipError> for ArchiveError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Invalid(e.to_string())
    }
}

/// Something in an archive that will not be applied as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// Unknown file, ignored.
    Ignored(String),
    /// Invalid file (syntax, colors…), ignored.
    Invalid { path: String, reason: String },
    /// A `files.roots` folder that does not exist on this PC.
    MissingRoot(String),
}

/// A `.flair` file, read and validated, ready to apply.
#[derive(Debug, Clone, Default)]
pub struct Archive {
    pub manifest: ArchiveManifest,
    /// Configuration, already migrated to the current version.
    pub config: Option<String>,
    pub themes: Vec<(String, String)>,
    pub lexicons: Vec<(String, String)>,
    pub history: Option<String>,
    pub frecency: Option<String>,
    pub warnings: Vec<Warning>,
}

/// How to apply an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Configuration, themes, lexicons and data become those of the file.
    ReplaceAll,
    /// Only add the themes and lexicons; current settings are kept.
    AddThemesAndLexicons,
}

// ---------------------------------------------------------------- export

/// `*.toml` files of a directory, sorted (missing directory: none).
fn toml_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml") && p.is_file())
        .filter_map(|p| Some((p.file_name()?.to_str()?.to_owned(), p)))
        .collect();
    files.sort();
    files
}

/// Writes the home directory as `~` in `files.roots`, so paths stay valid on
/// another PC.
fn portable_config(text: &str, home: &Path) -> String {
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return text.to_owned();
    };
    let home = home.to_string_lossy().replace('\\', "/");
    if home.is_empty() {
        return text.to_owned();
    }
    let roots = doc
        .get_mut("files")
        .and_then(|f| f.get_mut("roots"))
        .and_then(toml_edit::Item::as_array_mut);
    if let Some(roots) = roots {
        for root in roots.iter_mut() {
            let Some(path) = root.as_str() else { continue };
            let normalized = path.replace('\\', "/");
            if let Some(rest) = normalized.strip_prefix(&home)
                && (rest.is_empty() || rest.starts_with('/'))
            {
                let decor = root.decor().clone();
                *root = format!("~{rest}").into();
                *root.decor_mut() = decor;
            }
        }
    }
    doc.to_string()
}

fn history_entries(json: &str) -> Option<usize> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.as_array().map(Vec::len))
}

/// Exports everything (or the settings only) into `dest`.
pub fn export(
    layout: &Layout,
    dest: &Path,
    settings_only: bool,
    origin: &Origin,
) -> Result<ArchiveManifest, ArchiveError> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut contents = Contents::default();

    if let Ok(text) = fs::read_to_string(&layout.config_file) {
        files.push((
            "config.toml".into(),
            portable_config(&text, &layout.home).into_bytes(),
        ));
        contents.config = true;
    }
    for (name, path) in toml_files(&layout.themes_dir) {
        files.push((format!("themes/{name}"), fs::read(path)?));
        contents.themes.push(name);
    }
    for (name, path) in toml_files(&layout.lexicon_dir) {
        files.push((format!("lexicon/{name}"), fs::read(path)?));
        contents.lexicons.push(name);
    }
    if !settings_only {
        if let Ok(text) = fs::read_to_string(layout.data_dir.join(HISTORY)) {
            contents.history_entries = history_entries(&text);
            files.push((format!("data/{HISTORY}"), text.into_bytes()));
        }
        if let Ok(bytes) = fs::read(layout.data_dir.join(FRECENCY)) {
            contents.frecency = true;
            files.push((format!("data/{FRECENCY}"), bytes));
        }
    }

    let manifest = ArchiveManifest {
        format: FORMAT,
        flair_version: origin.flair_version.clone(),
        config_version: CONFIG_VERSION,
        date: UtcDateTime::now().iso8601(),
        host: origin.host.clone(),
        os: origin.os.clone(),
        settings_only,
        contents,
    };

    // Written next to the destination, then renamed: never a partial file.
    let tmp = dest.with_extension("flair.tmp");
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&tmp)?);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json", options)?;
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| ArchiveError::Invalid(e.to_string()))?;
        zip.write_all(&json)?;
        for (name, data) in &files {
            zip.start_file(name.as_str(), options)?;
            zip.write_all(data)?;
        }
        zip.finish()?;
    }
    fs::rename(&tmp, dest)?;
    Ok(manifest)
}

// ---------------------------------------------------------------- import

/// A file name usable for a theme or lexicon: no separator, no hidden file.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        && name.ends_with(".toml")
}

enum Slot {
    Manifest,
    Config,
    Theme(String),
    Lexicon(String),
    History,
    Frecency,
    Unknown,
}

/// Classifies an entry name; any unsafe path refuses the whole archive.
fn slot(name: &str) -> Result<Slot, ArchiveError> {
    let unsafe_path = name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.split('/').any(|part| part == "..")
        || name.as_bytes().get(1) == Some(&b':');
    if unsafe_path {
        return Err(ArchiveError::UnsafePath(name.to_owned()));
    }
    Ok(match name.split_once('/') {
        None if name == "manifest.json" => Slot::Manifest,
        None if name == "config.toml" => Slot::Config,
        Some(("themes", file)) if is_plain_name(file) => Slot::Theme(file.to_owned()),
        Some(("lexicon", file)) if is_plain_name(file) => Slot::Lexicon(file.to_owned()),
        Some(("data", HISTORY)) => Slot::History,
        Some(("data", FRECENCY)) => Slot::Frecency,
        _ => Slot::Unknown,
    })
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(path),
    }
}

/// Reads and validates a `.flair` file. Writes nothing.
pub fn read(path: &Path, home: &Path) -> Result<Archive, ArchiveError> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    if zip.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooLarge);
    }
    let mut archive = Archive::default();
    let mut manifest = None;
    let mut total: u64 = 0;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.into_owned();
        let slot = slot(&name)?;
        if let Slot::Unknown = slot {
            archive.warnings.push(Warning::Ignored(name));
            continue;
        }
        // Never trust the declared size: read at most the limit + 1 byte.
        let mut data = Vec::new();
        (&mut entry)
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut data)?;
        total += data.len() as u64;
        if data.len() as u64 > MAX_ENTRY_BYTES || total > MAX_TOTAL_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        let Ok(text) = String::from_utf8(data) else {
            archive.warnings.push(Warning::Invalid {
                path: name,
                reason: "not UTF-8 text".into(),
            });
            continue;
        };
        let invalid = |reason: String| Warning::Invalid {
            path: name.clone(),
            reason,
        };
        match slot {
            Slot::Manifest => {
                manifest = Some(
                    serde_json::from_str::<ArchiveManifest>(&text)
                        .map_err(|e| ArchiveError::Invalid(format!("manifest.json: {e}")))?,
                );
            }
            Slot::Config => archive.config = Some(text),
            Slot::Theme(file) => match Theme::from_toml(&text) {
                Ok(_) => archive.themes.push((file, text)),
                Err(e) => archive.warnings.push(invalid(e.to_string())),
            },
            Slot::Lexicon(file) => match text.parse::<toml::Table>() {
                Ok(_) => archive.lexicons.push((file, text)),
                Err(e) => archive.warnings.push(invalid(e.message().to_owned())),
            },
            Slot::History | Slot::Frecency => {
                if let Err(e) = serde_json::from_str::<serde_json::Value>(&text) {
                    archive.warnings.push(invalid(e.to_string()));
                } else if matches!(slot, Slot::History) {
                    archive.history = Some(text);
                } else {
                    archive.frecency = Some(text);
                }
            }
            Slot::Unknown => {}
        }
    }

    let manifest = manifest.ok_or_else(|| ArchiveError::Invalid("no manifest.json".into()))?;
    if manifest.format > FORMAT || manifest.config_version > CONFIG_VERSION {
        return Err(ArchiveError::Newer);
    }

    // Older configuration: same migrations as for an update.
    if let Some(text) = archive.config.take() {
        match migrate::migrate(&text) {
            Ok(Some(migrated)) => archive.config = Some(migrated.text),
            Ok(None) => archive.config = Some(text),
            Err(e) => archive.warnings.push(Warning::Invalid {
                path: "config.toml".into(),
                reason: e.to_string(),
            }),
        }
    }
    if let Some(text) = &archive.config {
        let parsed = crate::settings::parse_config(text);
        if parsed.is_syntax_error() {
            archive.warnings.push(Warning::Invalid {
                path: "config.toml".into(),
                reason: parsed
                    .issues
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            });
            archive.config = None;
        } else {
            for root in &parsed.config.files.roots {
                if !expand_home(root, home).exists() {
                    archive.warnings.push(Warning::MissingRoot(root.clone()));
                }
            }
        }
    }
    archive.manifest = manifest;
    Ok(archive)
}

/// Result of applying an archive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// Imported themes/lexicons renamed because the name existed (`from`, `to`).
    pub renamed: Vec<(String, String)>,
}

/// Writes through a temporary file then renames, so readers never see half a file.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// `lavande.toml` → `lavande-2.toml`, `lavande-3.toml`… until free.
fn free_name(dir: &Path, file: &str) -> String {
    if !dir.join(file).exists() {
        return file.to_owned();
    }
    let stem = file.trim_end_matches(".toml");
    (2..)
        .map(|n| format!("{stem}-{n}.toml"))
        .find(|candidate| !dir.join(candidate).exists())
        .unwrap_or_else(|| file.to_owned())
}

/// Applies a validated archive.
pub fn apply(
    archive: &Archive,
    layout: &Layout,
    mode: ImportMode,
) -> Result<Applied, ArchiveError> {
    let mut applied = Applied::default();
    let groups = [
        (&archive.themes, &layout.themes_dir),
        (&archive.lexicons, &layout.lexicon_dir),
    ];
    match mode {
        ImportMode::ReplaceAll => {
            if let Some(config) = &archive.config {
                write_atomic(&layout.config_file, config.as_bytes())?;
            }
            for (files, dir) in groups {
                for (_, old) in toml_files(dir) {
                    fs::remove_file(old)?;
                }
                for (name, text) in files {
                    write_atomic(&dir.join(name), text.as_bytes())?;
                }
            }
            // A "settings only" file has no data: keep the current data.
            if let Some(history) = &archive.history {
                write_atomic(&layout.data_dir.join(HISTORY), history.as_bytes())?;
            }
            if let Some(frecency) = &archive.frecency {
                write_atomic(&layout.data_dir.join(FRECENCY), frecency.as_bytes())?;
            }
        }
        ImportMode::AddThemesAndLexicons => {
            for (files, dir) in groups {
                for (name, text) in files {
                    let target = free_name(dir, name);
                    if &target != name {
                        applied.renamed.push((name.clone(), target.clone()));
                    }
                    write_atomic(&dir.join(&target), text.as_bytes())?;
                }
            }
        }
    }
    Ok(applied)
}

// --------------------------------------------------------------- backups

/// One automatic backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub path: PathBuf,
    /// `20261126-141200`, from the file name.
    pub stamp: String,
    /// Why it was made (`update`, `import`, `reset`, `migration`).
    pub reason: String,
}

/// Saves the current state to `backups/backup-<date>-<reason>.flair` and keeps
/// the [`KEPT_BACKUPS`] most recent ones.
pub fn backup(
    layout: &Layout,
    backups_dir: &Path,
    origin: &Origin,
    reason: &str,
) -> Result<PathBuf, ArchiveError> {
    fs::create_dir_all(backups_dir)?;
    let mut path = backups_dir.join(format!(
        "backup-{}-{reason}.flair",
        UtcDateTime::now().compact()
    ));
    // Two backups in the same second: suffix the second one.
    let mut n = 2;
    while path.exists() {
        path = backups_dir.join(format!(
            "backup-{}-{reason}-{n}.flair",
            UtcDateTime::now().compact()
        ));
        n += 1;
    }
    export(layout, &path, false, origin)?;
    for old in list_backups(backups_dir).into_iter().skip(KEPT_BACKUPS) {
        let _ = fs::remove_file(old.path);
    }
    Ok(path)
}

/// Automatic backups, most recent first.
pub fn list_backups(backups_dir: &Path) -> Vec<Backup> {
    let mut backups: Vec<Backup> = fs::read_dir(backups_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            let rest = name.strip_prefix("backup-")?.strip_suffix(".flair")?;
            // backup-YYYYMMDD-HHMMSS-reason[-n]
            let stamp = rest.get(..15)?.to_owned();
            let reason = rest.get(16..).unwrap_or_default().to_owned();
            Some(Backup {
                path: path.clone(),
                stamp,
                reason,
            })
        })
        .collect();
    backups.sort_by(|a, b| b.path.cmp(&a.path));
    backups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::settings::generate;

    struct Sandbox {
        root: PathBuf,
        layout: Layout,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("flair-archive-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let home = root.join("home");
            fs::create_dir_all(home.join("Projets")).expect("home");
            let layout = Layout {
                config_file: root.join("config/config.toml"),
                themes_dir: root.join("config/themes"),
                lexicon_dir: root.join("config/lexicon"),
                data_dir: root.join("data"),
                home,
            };
            Self { root, layout }
        }

        fn populate(&self) {
            let home = self.layout.home.to_string_lossy().replace('\\', "/");
            let config = generate::default_config_text(Lang::Fr).replace(
                "roots = [\"~\"]",
                &format!("roots = [\"{home}/Projets\", \"/srv/data\"]"),
            );
            write_atomic(&self.layout.config_file, config.as_bytes()).expect("config");
            write_atomic(
                &self.layout.themes_dir.join("lavande.toml"),
                b"name = \"Lavande\"\nbackground = \"#1E1B2E\"\nforeground = \"#E6E1F5\"\naccent = \"#B79CFF\"\n",
            )
            .expect("theme");
            write_atomic(
                &self.layout.lexicon_dir.join("es.toml"),
                b"[meta]\ncode = \"es\"\n",
            )
            .expect("lexicon");
            write_atomic(&self.layout.data_dir.join(HISTORY), b"[\"1+1\", \"2*3\"]")
                .expect("history");
            write_atomic(
                &self.layout.data_dir.join(FRECENCY),
                b"{\"app:Firefox\": 3}",
            )
            .expect("frecency");
        }

        fn origin() -> Origin {
            Origin {
                flair_version: "0.4.0".into(),
                host: "PC-BUREAU".into(),
                os: "Windows".into(),
            }
        }

        fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
            let mut files = Vec::new();
            for (dir, prefix) in [
                (&self.layout.themes_dir, "themes"),
                (&self.layout.lexicon_dir, "lexicon"),
            ] {
                for (name, path) in toml_files(dir) {
                    files.push((format!("{prefix}/{name}"), fs::read(path).expect("read")));
                }
            }
            for file in [HISTORY, FRECENCY] {
                if let Ok(data) = fs::read(self.layout.data_dir.join(file)) {
                    files.push((file.into(), data));
                }
            }
            if let Ok(data) = fs::read(&self.layout.config_file) {
                files.push(("config".into(), data));
            }
            files
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// Writes a raw ZIP with the given entries (for crafted archives).
    fn raw_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).expect("create"));
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default())
                .expect("entry");
            zip.write_all(data).expect("write");
        }
        zip.finish().expect("finish");
    }

    fn manifest_json(format: u32, config_version: u32) -> Vec<u8> {
        serde_json::to_vec(&ArchiveManifest {
            format,
            config_version,
            ..ArchiveManifest::default()
        })
        .expect("json")
    }

    #[test]
    fn export_then_import_gives_exactly_the_same_state() {
        let source = Sandbox::new("src");
        source.populate();
        let file = source.root.join("export.flair");
        let manifest = export(&source.layout, &file, false, &Sandbox::origin()).expect("export");
        assert_eq!(manifest.contents.themes, ["lavande.toml"]);
        assert_eq!(manifest.contents.history_entries, Some(2));
        assert!(manifest.contents.frecency);

        // Same PC (same home): byte for byte identical, except `~` in roots.
        let target = Sandbox::new("dst");
        let mut layout = target.layout.clone();
        layout.home = source.layout.home.clone();
        let archive = read(&file, &layout.home).expect("read");
        assert!(
            archive
                .warnings
                .iter()
                .all(|w| matches!(w, Warning::MissingRoot(_))),
            "{:?}",
            archive.warnings
        );
        apply(&archive, &layout, ImportMode::ReplaceAll).expect("apply");
        let imported = Sandbox {
            root: target.root.clone(),
            layout,
        };
        let before = source.snapshot();
        let after = imported.snapshot();
        std::mem::forget(imported); // same root as `target`, dropped once
        for ((name_a, a), (name_b, b)) in before.iter().zip(&after) {
            assert_eq!(name_a, name_b);
            if name_a == "config" {
                let (a, b) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
                let parsed_a = crate::settings::parse_config(&a).config;
                let mut parsed_b = crate::settings::parse_config(&b).config;
                parsed_b.files.roots = parsed_b
                    .files
                    .roots
                    .iter()
                    .map(|r| {
                        expand_home(r, &source.layout.home)
                            .to_string_lossy()
                            .replace('\\', "/")
                    })
                    .collect();
                assert_eq!(parsed_a, parsed_b);
            } else {
                assert_eq!(a, b, "{name_a}");
            }
        }
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn home_paths_become_portable() {
        let s = Sandbox::new("portable");
        s.populate();
        let file = s.root.join("e.flair");
        export(&s.layout, &file, false, &Sandbox::origin()).expect("export");
        let archive = read(&file, Path::new("/nonexistent-home")).expect("read");
        let config = archive.config.expect("config");
        assert!(
            config.contains("roots = [\"~/Projets\", \"/srv/data\"]"),
            "{config}"
        );
        // On another PC, both roots are missing and reported.
        assert_eq!(
            archive
                .warnings
                .iter()
                .filter(|w| matches!(w, Warning::MissingRoot(_)))
                .count(),
            2
        );
    }

    #[test]
    fn windows_export_imports_on_linux() {
        // Paths written by a Windows Flair: `~` with backslashes.
        let s = Sandbox::new("winexport");
        let file = s.root.join("win.flair");
        let config = "config_version = 2\n[files]\nroots = [\"~\\\\Projets\"]\n";
        raw_zip(
            &file,
            &[
                ("manifest.json", &manifest_json(1, 2)),
                ("config.toml", config.as_bytes()),
                (
                    "themes/lavande.toml",
                    b"name = \"Lavande\"\nbackground = \"#000\"\nforeground = \"#fff\"\n",
                ),
            ],
        );
        let archive = read(&file, &s.layout.home).expect("read");
        assert!(archive.warnings.is_empty(), "{:?}", archive.warnings);
        apply(&archive, &s.layout, ImportMode::ReplaceAll).expect("apply");
        assert!(s.layout.themes_dir.join("lavande.toml").exists());
    }

    #[test]
    fn settings_only_has_no_history_nor_frecency() {
        let s = Sandbox::new("settingsonly");
        s.populate();
        let file = s.root.join("settings.flair");
        let manifest = export(&s.layout, &file, true, &Sandbox::origin()).expect("export");
        assert!(manifest.settings_only);
        let mut zip = zip::ZipArchive::new(fs::File::open(&file).expect("open")).expect("zip");
        let names: Vec<String> = (0..zip.len())
            .map(|i| {
                zip.by_index(i)
                    .expect("entry")
                    .name()
                    .expect("name")
                    .into_owned()
            })
            .collect();
        assert!(names.iter().all(|n| !n.starts_with("data/")), "{names:?}");
        assert!(names.contains(&"config.toml".to_owned()));
    }

    #[test]
    fn older_archive_is_migrated() {
        let s = Sandbox::new("older");
        let file = s.root.join("old.flair");
        raw_zip(
            &file,
            &[
                ("manifest.json", &manifest_json(1, 1)),
                (
                    "config.toml",
                    b"config_version = 1\n[theme]\nanimations = true\nmode = \"light\"\n",
                ),
            ],
        );
        let archive = read(&file, &s.layout.home).expect("read");
        let config = archive.config.expect("config");
        assert!(!config.contains("animations"));
        assert!(config.contains(&format!("config_version = {CONFIG_VERSION}")));
    }

    #[test]
    fn newer_archive_is_refused() {
        let s = Sandbox::new("newer");
        let file = s.root.join("new.flair");
        raw_zip(
            &file,
            &[("manifest.json", &manifest_json(1, CONFIG_VERSION + 1))],
        );
        assert!(matches!(
            read(&file, &s.layout.home),
            Err(ArchiveError::Newer)
        ));
        raw_zip(&file, &[("manifest.json", &manifest_json(FORMAT + 1, 1))]);
        assert!(matches!(
            read(&file, &s.layout.home),
            Err(ArchiveError::Newer)
        ));
    }

    #[test]
    fn trapped_archives_are_refused_without_writing() {
        let s = Sandbox::new("trapped");
        s.populate();
        let before = s.snapshot();
        let file = s.root.join("trap.flair");
        let manifest = manifest_json(1, 2);
        for evil in [
            "../evil.toml",
            "themes/../../evil.toml",
            "/etc/evil",
            "C:/evil",
            "themes\\x.toml",
        ] {
            raw_zip(&file, &[("manifest.json", &manifest), (evil, b"x")]);
            assert!(
                matches!(
                    read(&file, &s.layout.home),
                    Err(ArchiveError::UnsafePath(_))
                ),
                "{evil}"
            );
        }
        // Zip bomb: highly compressible, larger than the limit once inflated.
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&file).expect("create"));
            let deflate =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest.json", deflate).expect("entry");
            zip.write_all(&manifest).expect("write");
            zip.start_file("data/history.json", deflate).expect("entry");
            let chunk = vec![b' '; 1024 * 1024];
            for _ in 0..11 {
                zip.write_all(&chunk).expect("write");
            }
            zip.finish().expect("finish");
        }
        assert!(
            fs::metadata(&file).expect("meta").len() < 200_000,
            "compressed bomb is small"
        );
        assert!(matches!(
            read(&file, &s.layout.home),
            Err(ArchiveError::TooLarge)
        ));
        assert_eq!(s.snapshot(), before, "nothing written");
    }

    #[test]
    fn invalid_parts_are_ignored_and_reported() {
        let s = Sandbox::new("invalidparts");
        let file = s.root.join("mixed.flair");
        raw_zip(
            &file,
            &[
                ("manifest.json", &manifest_json(1, 2)),
                (
                    "themes/bad.toml",
                    b"name = \"Bad\"\nbackground = \"#zzz\"\nforeground = \"#fff\"\n",
                ),
                (
                    "themes/good.toml",
                    b"name = \"Good\"\nbackground = \"#000\"\nforeground = \"#fff\"\n",
                ),
                ("data/history.json", b"{not json"),
                ("plugins/evil.dll", b"MZ"),
            ],
        );
        let archive = read(&file, &s.layout.home).expect("read");
        assert_eq!(archive.themes.len(), 1);
        assert!(archive.history.is_none());
        assert_eq!(archive.warnings.len(), 3, "{:?}", archive.warnings);
    }

    #[test]
    fn merge_renames_existing_names() {
        let s = Sandbox::new("merge");
        s.populate();
        let file = s.root.join("m.flair");
        raw_zip(
            &file,
            &[
                ("manifest.json", &manifest_json(1, 2)),
                ("config.toml", b"max_results = 3\n"),
                (
                    "themes/lavande.toml",
                    b"name = \"Lavande\"\nbackground = \"#000\"\nforeground = \"#fff\"\n",
                ),
            ],
        );
        let config_before = fs::read(&s.layout.config_file).expect("config");
        let archive = read(&file, &s.layout.home).expect("read");
        let applied = apply(&archive, &s.layout, ImportMode::AddThemesAndLexicons).expect("apply");
        assert_eq!(
            applied.renamed,
            [("lavande.toml".to_owned(), "lavande-2.toml".to_owned())]
        );
        assert!(s.layout.themes_dir.join("lavande-2.toml").exists());
        assert_eq!(
            fs::read(&s.layout.config_file).expect("config"),
            config_before,
            "settings kept"
        );
    }

    #[test]
    fn backups_keep_the_ten_most_recent() {
        let s = Sandbox::new("backups");
        s.populate();
        let dir = s.root.join("backups");
        for _ in 0..12 {
            backup(&s.layout, &dir, &Sandbox::origin(), "update").expect("backup");
        }
        let list = list_backups(&dir);
        assert_eq!(list.len(), KEPT_BACKUPS);
        assert!(
            list.iter().all(|b| b.reason.starts_with("update")),
            "{list:?}"
        );
        let archive = read(&list[0].path, &s.layout.home).expect("readable");
        assert!(archive.config.is_some());
    }
}

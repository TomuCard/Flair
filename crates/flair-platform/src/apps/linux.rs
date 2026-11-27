//! Linux applications: `.desktop` files (Desktop Entry Specification).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use flair_core::LaunchTarget;

use super::desktop_entry::{DesktopEntry, ExecContext, parse_exec};
use super::{App, AppIcon};
use crate::image::{self, IconRgba};

/// Languages whose names are indexed besides the default `Name`.
const LANGS: [&str; 2] = ["fr", "en"];

/// `applications` folders in priority order: an entry of an earlier folder
/// hides the entry with the same id in later ones.
fn application_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());

    let mut dirs: Vec<PathBuf> = Vec::new();
    dirs.extend(data_home);
    if let Some(h) = &home {
        dirs.push(h.join(".local/share/flatpak/exports/share"));
    }
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    dirs.push("/var/lib/flatpak/exports/share".into());
    let mut apps: Vec<PathBuf> = dirs.into_iter().map(|d| d.join("applications")).collect();
    apps.push("/var/lib/snapd/desktop/applications".into());
    let mut seen = HashSet::new();
    apps.retain(|d| seen.insert(d.clone()));
    apps
}

pub fn watch_roots() -> Vec<PathBuf> {
    application_dirs()
}

fn current_desktops() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn scan() -> Vec<App> {
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let path_dirs: Vec<PathBuf> = std::env::split_paths(&path_env).collect();
    scan_dirs(&application_dirs(), &current_desktops(), &path_dirs)
}

/// Scans `applications` folders (testable with any folders).
pub(crate) fn scan_dirs(dirs: &[PathBuf], desktops: &[String], path_dirs: &[PathBuf]) -> Vec<App> {
    let mut seen_ids = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        let mut files = Vec::new();
        collect_desktop_files(dir, dir, 0, &mut files);
        files.sort();
        for (id, path) in files {
            // Earlier folders win, even when their entry is hidden.
            if !seen_ids.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = app_from_entry(&id, &path, &text, desktops, path_dirs) {
                apps.push(app);
            }
        }
    }
    apps
}

/// Finds `.desktop` files; their id is the relative path with `/` → `-`.
fn collect_desktop_files(root: &Path, dir: &Path, depth: u32, out: &mut Vec<(String, PathBuf)>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // Follows symlinks (Flatpak and Snap exports are links).
        let is_dir = kind.is_dir() || (kind.is_symlink() && path.is_dir());
        if is_dir {
            collect_desktop_files(root, &path, depth + 1, out);
        } else if path.extension().is_some_and(|e| e == "desktop")
            && let Ok(rel) = path.strip_prefix(root)
        {
            let id = rel.to_string_lossy().replace('/', "-");
            out.push((id, path));
        }
    }
}

fn app_from_entry(
    id: &str,
    path: &Path,
    text: &str,
    desktops: &[String],
    path_dirs: &[PathBuf],
) -> Option<App> {
    let entry = DesktopEntry::parse(text)?;
    if entry.get("Type") != Some("Application")
        || entry.bool("NoDisplay")
        || entry.bool("Hidden")
        || !entry.shown_in(desktops)
    {
        return None;
    }
    if let Some(try_exec) = entry.get("TryExec")
        && find_program(try_exec, path_dirs).is_none()
    {
        return None;
    }
    let name = entry.get("Name")?.to_owned();
    let args = parse_exec(entry.get("Exec")?, &ExecContext::default())?;

    let localized: Vec<(String, String)> = LANGS
        .iter()
        .filter_map(|lang| {
            let n = entry.localized("Name", lang)?;
            (n != name).then(|| ((*lang).to_owned(), n.to_owned()))
        })
        .collect();

    let mut alternatives: Vec<String> = Vec::new();
    alternatives.extend(entry.get("GenericName").map(str::to_owned));
    alternatives.extend(DesktopEntry::list(entry.get("Keywords")));
    for lang in LANGS {
        alternatives.extend(entry.localized("GenericName", lang).map(str::to_owned));
        alternatives.extend(DesktopEntry::list(entry.localized("Keywords", lang)));
    }
    // The program name (`nautilus` finds Files), unless it is a wrapper.
    if let Some(program) = Path::new(&args[0]).file_name().and_then(|f| f.to_str())
        && !matches!(
            program,
            "env" | "flatpak" | "snap" | "sh" | "bash" | "python3" | "java"
        )
    {
        alternatives.push(program.to_owned());
    }
    let mut seen: HashSet<String> = std::iter::once(&name)
        .chain(localized.iter().map(|(_, n)| n))
        .map(|n| n.to_lowercase())
        .collect();
    alternatives.retain(|a| seen.insert(a.to_lowercase()));

    let icon = match entry.get("Icon").filter(|i| !i.is_empty()) {
        Some(i) if i.starts_with('/') => AppIcon::File(i.into()),
        Some(i) => AppIcon::Themed(i.to_owned()),
        None => AppIcon::None,
    };
    Some(App {
        id: format!("app:{id}"),
        name,
        localized,
        alternatives,
        target: LaunchTarget::DesktopEntry(path.to_owned()),
        icon,
        can_elevate: false,
    })
}

/// Absolute path of a program: as is if absolute, otherwise searched in `PATH`.
fn find_program(program: &str, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_owned());
    }
    path_dirs
        .iter()
        .map(|d| d.join(program))
        .find(|c| c.is_file())
}

// ---------------------------------------------------------------- icons

/// Icon theme of the desktop (GNOME/GTK settings, then KDE), `hicolor` if
/// unknown. Read once.
fn icon_theme() -> &'static str {
    static THEME: OnceLock<String> = OnceLock::new();
    THEME.get_or_init(|| {
        gsettings_icon_theme()
            .or_else(kde_icon_theme)
            .unwrap_or_else(|| "hicolor".into())
    })
}

fn gsettings_icon_theme() -> Option<String> {
    let out = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let name = String::from_utf8(out.stdout).ok()?;
    let name = name.trim().trim_matches('\'');
    (out.status.success() && !name.is_empty()).then(|| name.to_owned())
}

fn kde_icon_theme() -> Option<String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let text = std::fs::read_to_string(config.join("kdeglobals")).ok()?;
    let mut in_icons = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_icons = line == "[Icons]";
        } else if in_icons && let Some(theme) = line.strip_prefix("Theme=") {
            return Some(theme.trim().to_owned());
        }
    }
    None
}

pub fn load_icon(icon: &AppIcon, size: u32) -> Option<IconRgba> {
    let path = match icon {
        AppIcon::File(path) => path.clone(),
        AppIcon::Themed(name) => find_themed(name, size)?,
        AppIcon::None | AppIcon::Shell(_) => return None,
    };
    load_file(&path, size)
}

pub(crate) fn find_themed(name: &str, size: u32) -> Option<PathBuf> {
    // Some entries give a file name instead of an icon name.
    let name = name
        .strip_suffix(".png")
        .or_else(|| name.strip_suffix(".svg"))
        .or_else(|| name.strip_suffix(".xpm"))
        .unwrap_or(name);
    let size = u16::try_from(size).unwrap_or(u16::MAX);
    freedesktop_icons::lookup(name)
        .with_size(size)
        .with_theme(icon_theme())
        .with_cache()
        .find()
        .or_else(|| {
            ["png", "svg"]
                .iter()
                .map(|ext| PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}")))
                .find(|p| p.is_file())
        })
}

pub(crate) fn load_file(path: &Path, size: u32) -> Option<IconRgba> {
    let data = std::fs::read(path).ok()?;
    match path.extension()?.to_str()? {
        "svg" => image::render_svg(&data, size),
        "png" => image::fit(image::decode_png(&data)?, size),
        // XPM and other old formats are not supported: the generic icon is used.
        _ => None,
    }
}

// ---------------------------------------------------------------- launch

pub fn launch(target: &LaunchTarget, _elevated: bool) -> std::io::Result<()> {
    match target {
        LaunchTarget::DesktopEntry(path) => launch_entry(path),
        LaunchTarget::Path { path, args } => {
            let mut cmd = Command::new(path);
            cmd.args(args);
            spawn_detached(cmd)
        }
        LaunchTarget::AppUserModelId(_) => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Store apps only exist on Windows",
        )),
    }
}

fn launch_entry(path: &Path) -> std::io::Result<()> {
    let invalid =
        |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_owned());
    let text = std::fs::read_to_string(path)?;
    let entry = DesktopEntry::parse(&text).ok_or_else(|| invalid("no [Desktop Entry] group"))?;
    let location = path.to_string_lossy();
    let cx = ExecContext {
        icon: entry.get("Icon"),
        name: entry.get("Name").unwrap_or_default(),
        location: &location,
    };
    let mut args = entry
        .get("Exec")
        .and_then(|e| parse_exec(e, &cx))
        .ok_or_else(|| invalid("invalid Exec line"))?;
    if entry.bool("Terminal") {
        let mut wrapped = terminal_command().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no terminal emulator found")
        })?;
        wrapped.append(&mut args);
        args = wrapped;
    }
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]);
    if let Some(dir) = entry.get("Path").filter(|d| Path::new(d).is_dir()) {
        cmd.current_dir(dir);
    }
    spawn_detached(cmd)
}

/// Terminal emulator and the option that runs a command in it.
fn terminal_command() -> Option<Vec<String>> {
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let path_dirs: Vec<PathBuf> = std::env::split_paths(&path_env).collect();
    let user = std::env::var("TERMINAL").ok().filter(|t| !t.is_empty());
    let candidates = [
        ("x-terminal-emulator", "-e"),
        ("gnome-terminal", "--"),
        ("konsole", "-e"),
        ("xfce4-terminal", "-x"),
        ("kitty", "--"),
        ("alacritty", "-e"),
        ("foot", "--"),
        ("xterm", "-e"),
    ];
    if let Some(t) = user
        && find_program(&t, &path_dirs).is_some()
    {
        let flag = candidates
            .iter()
            .find(|(n, _)| t.ends_with(n))
            .map_or("-e", |c| c.1);
        return Some(vec![t, flag.into()]);
    }
    candidates
        .iter()
        .find(|(name, _)| find_program(name, &path_dirs).is_some())
        .map(|(name, flag)| vec![(*name).to_owned(), (*flag).to_owned()])
}

/// Starts a program in its own process group, detached from Flair's
/// standard streams; the child is reaped by a short-lived thread.
fn spawn_detached(mut cmd: Command) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dir");
        }
        std::fs::write(path, text).expect("write");
    }

    #[test]
    fn scan_respects_priority_and_visibility() {
        let root = std::env::temp_dir().join(format!("flair-apps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let user = root.join("user/applications");
        let system = root.join("system/applications");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("present"), "").expect("bin");

        write(
            &system,
            "org.gnome.Nautilus.desktop",
            "[Desktop Entry]\nType=Application\nName=Files\nName[fr]=Fichiers\nGenericName=File Manager\nKeywords=folder;Files;\nExec=nautilus --new-window %U\nIcon=org.gnome.Nautilus\n",
        );
        // Overridden by the user folder, which hides it.
        write(
            &system,
            "firefox.desktop",
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\n",
        );
        write(
            &user,
            "firefox.desktop",
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox\nHidden=true\n",
        );
        write(
            &system,
            "hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=x\nNoDisplay=true\n",
        );
        write(
            &system,
            "link.desktop",
            "[Desktop Entry]\nType=Link\nName=Web\nURL=https://x\n",
        );
        write(
            &system,
            "kde-only.desktop",
            "[Desktop Entry]\nType=Application\nName=Kate\nExec=kate\nOnlyShowIn=KDE;\n",
        );
        write(
            &system,
            "missing.desktop",
            "[Desktop Entry]\nType=Application\nName=Missing\nExec=missing\nTryExec=not-installed\n",
        );
        write(
            &system,
            "present.desktop",
            "[Desktop Entry]\nType=Application\nName=Present\nExec=present\nTryExec=present\nIcon=/opt/present/icon.png\n",
        );
        write(
            &system,
            "kde4/okular.desktop",
            "[Desktop Entry]\nType=Application\nName=Okular\nExec=okular\n",
        );

        let apps = scan_dirs(&[user, system], &["GNOME".into()], &[bin]);
        let ids: Vec<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "app:kde4-okular.desktop",
                "app:org.gnome.Nautilus.desktop",
                "app:present.desktop"
            ]
        );
        let files = &apps[1];
        assert_eq!(files.title("fr"), "Fichiers");
        assert_eq!(files.title("en"), "Files");
        // `Files` (keyword) is already the name: not repeated.
        assert_eq!(files.alternatives, ["File Manager", "folder", "nautilus"]);
        assert_eq!(files.icon, AppIcon::Themed("org.gnome.Nautilus".into()));
        assert_eq!(apps[2].icon, AppIcon::File("/opt/present/icon.png".into()));
        let _ = std::fs::remove_dir_all(root);
    }
}

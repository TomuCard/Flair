//! Self-installation, uninstallation and where Flair runs from.
//!
//! - Windows: `%LocalAppData%\Programs\Flair\flair.exe` (no administrator
//!   rights), Start menu shortcut, "Installed apps" entry, `.flair` association.
//! - Linux: `~/.local/bin/flair`, `.desktop` entry, scalable icon, MIME type.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::BaseDirs;

/// Where the running executable is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// In the installation folder.
    Installed,
    /// Anywhere else (downloads folder, USB key…): portable mode.
    Portable,
    /// Installed by a package manager (`/usr/...`): updates are its job.
    PackageManaged,
    /// A `cargo` build (`target/` folder): never installs nor updates.
    Development,
}

pub fn exe_name() -> &'static str {
    if cfg!(windows) { "flair.exe" } else { "flair" }
}

/// Installation folder.
pub fn install_dir() -> Option<PathBuf> {
    let base = BaseDirs::new()?;
    Some(if cfg!(windows) {
        base.data_local_dir().join("Programs").join("Flair")
    } else {
        base.home_dir().join(".local").join("bin")
    })
}

/// Installed executable path.
pub fn installed_exe() -> Option<PathBuf> {
    install_dir().map(|d| d.join(exe_name()))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn location(exe: &Path) -> Location {
    if exe.components().any(|c| c.as_os_str() == "target") {
        return Location::Development;
    }
    if cfg!(not(windows)) && exe.starts_with("/usr") {
        return Location::PackageManaged;
    }
    match installed_exe() {
        Some(installed) if same_file(exe, &installed) => Location::Installed,
        _ => Location::Portable,
    }
}

/// True when Flair can replace its own executable (updates).
pub fn can_self_update(exe: &Path) -> bool {
    if matches!(
        location(exe),
        Location::PackageManaged | Location::Development
    ) {
        return false;
    }
    let Some(dir) = exe.parent() else {
        return false;
    };
    let probe = dir.join(".flair-write-test");
    let writable = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(probe);
    writable
}

/// Copies `src` to `dest` through a temporary file in the same folder, then
/// renames it into place. A running `dest` is first renamed to
/// `flair.previous[.exe]` (allowed on Windows for a running executable).
pub fn replace_file(src: &Path, dest: &Path) -> io::Result<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| io::Error::other("no parent folder"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{}.new", exe_name()));
    fs::copy(src, &tmp)?;
    if dest.exists() {
        let previous = previous_exe(dest);
        let _ = fs::remove_file(&previous);
        fs::rename(dest, &previous)?;
    }
    fs::rename(&tmp, dest)
}

/// `flair.previous.exe` next to `exe`: the version before the last update.
pub fn previous_exe(exe: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "flair.previous.exe"
    } else {
        "flair.previous"
    };
    exe.with_file_name(name)
}

/// Options chosen on the installation screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub autostart: bool,
}

/// Installs `src` (the running executable); returns the installed path.
/// `icon_svg`: the logo, already in its fixed application color.
pub fn install(src: &Path, options: Options, version: &str, icon_svg: &str) -> io::Result<PathBuf> {
    let target = installed_exe().ok_or_else(|| io::Error::other("no home directory"))?;
    if !same_file(src, &target) {
        replace_file(src, &target)?;
    }
    imp::register(&target, version, icon_svg)?;
    crate::autostart::set(options.autostart, &target)?;
    tracing::info!(path = %target.display(), "installed");
    Ok(target)
}

/// Removes shortcuts, registrations and the executable (deleted after exit
/// on Windows). `data_dirs`: also removed when the user does not keep them.
pub fn uninstall(exe: &Path, data_dirs: &[PathBuf]) -> io::Result<()> {
    crate::autostart::set(false, exe)?;
    imp::unregister()?;
    for dir in data_dirs {
        match fs::remove_dir_all(dir) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                tracing::warn!(dir = %dir.display(), "cannot remove: {e}");
            }
            _ => {}
        }
    }
    let _ = fs::remove_file(previous_exe(exe));
    let dir = (location(exe) == Location::Installed && cfg!(windows))
        .then(|| exe.parent())
        .flatten();
    crate::process::delete_after_exit(exe, dir)?;
    tracing::info!(path = %exe.display(), "uninstalled");
    Ok(())
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW,
    };
    use windows::Win32::UI::Shell::{
        IShellLinkW, SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify, ShellLink,
    };
    use windows::core::{HSTRING, Interface, PCWSTR};

    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Flair";
    const EXT_KEY: &str = r"Software\Classes\.flair";
    const PROGID_KEY: &str = r"Software\Classes\Flair.Export";

    fn io(e: windows::core::Error) -> io::Error {
        io::Error::other(e)
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(Some(0))
            .collect()
    }

    /// Opens (creating) `HKCU\<path>`, writes the values, closes it.
    fn write_key(path: &str, strings: &[(&str, &str)], dwords: &[(&str, u32)]) -> io::Result<()> {
        let mut key = HKEY::default();
        // SAFETY: valid NUL-terminated path and out parameter.
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        }
        .ok()
        .map_err(io)?;
        let mut result = Ok(());
        for (name, value) in strings {
            let data = wide(value);
            // SAFETY: the byte view covers exactly the live `data` buffer.
            let bytes =
                unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
            let name = (!name.is_empty()).then(|| HSTRING::from(*name));
            let name_ptr = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
            // SAFETY: open key, valid name (or null for the default value) and data.
            let r = unsafe { RegSetValueExW(key, name_ptr, None, REG_SZ, Some(bytes)) };
            if result.is_ok() {
                result = r.ok().map_err(io);
            }
        }
        for (name, value) in dwords {
            let bytes = value.to_le_bytes();
            // SAFETY: open key, valid name and 4 bytes of data.
            let r = unsafe {
                RegSetValueExW(key, &HSTRING::from(*name), None, REG_DWORD, Some(&bytes))
            };
            if result.is_ok() {
                result = r.ok().map_err(io);
            }
        }
        // SAFETY: closing the key opened above.
        let _ = unsafe { RegCloseKey(key) };
        result
    }

    fn delete_tree(path: &str) {
        // SAFETY: constant root and valid path; a missing key is fine.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) };
    }

    fn start_menu_shortcut() -> Option<std::path::PathBuf> {
        directories::BaseDirs::new().map(|b| {
            b.config_dir()
                .join(r"Microsoft\Windows\Start Menu\Programs\Flair.lnk")
        })
    }

    fn create_shortcut(target: &Path, link: &Path) -> io::Result<()> {
        // SAFETY: COM is initialized for this thread for the duration of the
        // calls and uninitialized afterwards; all strings outlive the calls.
        unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let result = (|| -> windows::core::Result<()> {
                let shell_link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                shell_link.SetPath(&HSTRING::from(target.as_os_str()))?;
                if let Some(dir) = target.parent() {
                    shell_link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
                }
                shell_link.SetDescription(&HSTRING::from("Flair"))?;
                shell_link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0)?;
                let file: IPersistFile = shell_link.cast()?;
                file.Save(&HSTRING::from(link.as_os_str()), true)
            })();
            if init.is_ok() {
                CoUninitialize();
            }
            result.map_err(io)
        }
    }

    pub fn register(target: &Path, version: &str, _icon_svg: &str) -> io::Result<()> {
        let exe = target.display().to_string();
        let dir = target
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        if let Some(link) = start_menu_shortcut() {
            create_shortcut(target, &link)?;
        }
        let size_kb = std::fs::metadata(target)
            .map_or(0, |m| u32::try_from(m.len() / 1024).unwrap_or(u32::MAX));
        write_key(
            UNINSTALL_KEY,
            &[
                ("DisplayName", "Flair"),
                ("DisplayVersion", version),
                ("Publisher", "Flair"),
                ("DisplayIcon", &format!("{exe},0")),
                ("InstallLocation", &dir),
                ("UninstallString", &format!("\"{exe}\" --uninstall")),
                (
                    "URLInfoAbout",
                    &format!("https://github.com/{}", flair_core::update::REPOSITORY),
                ),
            ],
            &[("NoModify", 1), ("NoRepair", 1), ("EstimatedSize", size_kb)],
        )?;
        // Double-click on a `.flair` file opens Flair on the import summary.
        write_key(
            EXT_KEY,
            &[
                ("", "Flair.Export"),
                ("Content Type", "application/x-flair"),
            ],
            &[],
        )?;
        write_key(PROGID_KEY, &[("", "Flair")], &[])?;
        write_key(
            &format!(r"{PROGID_KEY}\DefaultIcon"),
            &[("", &format!("{exe},0"))],
            &[],
        )?;
        write_key(
            &format!(r"{PROGID_KEY}\shell\open\command"),
            &[("", &format!("\"{exe}\" \"%1\""))],
            &[],
        )?;
        // SAFETY: documented notification without item arguments.
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }

    pub fn unregister() -> io::Result<()> {
        if let Some(link) = start_menu_shortcut() {
            let _ = std::fs::remove_file(link);
        }
        delete_tree(UNINSTALL_KEY);
        delete_tree(EXT_KEY);
        delete_tree(PROGID_KEY);
        // SAFETY: documented notification without item arguments.
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    fn share_dir() -> io::Result<PathBuf> {
        directories::BaseDirs::new()
            .map(|b| b.data_dir().to_path_buf())
            .ok_or_else(|| io::Error::other("no home directory"))
    }

    fn files() -> io::Result<[PathBuf; 3]> {
        let share = share_dir()?;
        Ok([
            share.join("applications/flair.desktop"),
            share.join("icons/hicolor/scalable/apps/flair.svg"),
            share.join("mime/packages/flair.xml"),
        ])
    }

    fn write(path: &Path, data: &str) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, data)
    }

    fn refresh_databases() {
        // Best effort: the desktop picks the files up at next login anyway.
        if let Ok(share) = share_dir() {
            let _ = std::process::Command::new("update-mime-database")
                .arg(share.join("mime"))
                .output();
            let _ = std::process::Command::new("update-desktop-database")
                .arg(share.join("applications"))
                .output();
        }
    }

    pub fn register(target: &Path, _version: &str, icon_svg: &str) -> io::Result<()> {
        let [desktop, icon, mime] = files()?;
        let exec = target.display().to_string().replace('"', "\\\"");
        write(
            &desktop,
            &format!(
                "[Desktop Entry]\nType=Application\nName=Flair\nComment=Keyboard launcher\n\
                 Exec=\"{exec}\" %f\nIcon=flair\nTerminal=false\nCategories=Utility;\n\
                 MimeType=application/x-flair;\nStartupNotify=false\n"
            ),
        )?;
        write(&icon, icon_svg)?;
        write(
            &mime,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n\
             \x20 <mime-type type=\"application/x-flair\">\n\
             \x20   <comment>Flair export</comment>\n\
             \x20   <glob pattern=\"*.flair\"/>\n\
             \x20 </mime-type>\n</mime-info>\n",
        )?;
        refresh_databases();
        Ok(())
    }

    pub fn unregister() -> io::Result<()> {
        for file in files()? {
            let _ = fs::remove_file(file);
        }
        refresh_databases();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_builds_are_recognized() {
        assert_eq!(
            location(Path::new("/home/u/code/flair/target/release/flair")),
            Location::Development
        );
        assert!(!can_self_update(Path::new(
            "/home/u/code/flair/target/release/flair"
        )));
        if cfg!(not(windows)) {
            assert_eq!(
                location(Path::new("/usr/bin/flair")),
                Location::PackageManaged
            );
        }
        assert_eq!(
            location(Path::new("/tmp/Downloads/flair")),
            Location::Portable
        );
    }

    #[test]
    fn replace_keeps_the_previous_version() {
        let dir = std::env::temp_dir().join(format!("flair-replace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let (new, dest) = (dir.join("download"), dir.join(exe_name()));
        fs::write(&dest, b"v1").expect("v1");
        fs::write(&new, b"v2").expect("v2");
        replace_file(&new, &dest).expect("replaced");
        assert_eq!(fs::read(&dest).expect("dest"), b"v2");
        assert_eq!(fs::read(previous_exe(&dest)).expect("previous"), b"v1");
        let _ = fs::remove_dir_all(dir);
    }
}

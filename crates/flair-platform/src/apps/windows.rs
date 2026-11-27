//! Windows applications: Start menu shortcuts and Store apps.
//!
//! Shortcuts are read with the shell's own `IShellLinkW` (handles every
//! `.lnk` variant, environment variables included) rather than the `lnk`
//! crate; names come from the shell, so they are localized like in the Start
//! menu (`Explorateur de fichiers`), and the shortcut file name and target
//! executable are indexed too (`File Explorer`, `explorer`).

use std::collections::HashSet;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use flair_core::LaunchTarget;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAGS_AND_ATTRIBUTES, WIN32_FIND_DATAW};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, FOLDERID_CommonPrograms, FOLDERID_Programs,
    IEnumShellItems, IShellItem, IShellItemImageFactory, IShellLinkW, KF_FLAG_DEFAULT,
    SHCreateItemFromParsingName, SHFILEINFOW, SHGFI_DISPLAYNAME, SHGetFileInfoW,
    SHGetKnownFolderItem, SHGetKnownFolderPath, SIGDN, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, ShellExecuteW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use super::{App, AppIcon};
use crate::image::{self, IconRgba};

/// English names of Windows built-in apps, whose Start menu names are
/// translated. Keyed by executable name or Store package name (before `_`),
/// lowercase.
const ALIASES: &[(&str, &[&str])] = &[
    ("explorer.exe", &["File Explorer", "Explorer"]),
    ("taskmgr.exe", &["Task Manager"]),
    ("control.exe", &["Control Panel"]),
    ("notepad.exe", &["Notepad"]),
    ("mspaint.exe", &["Paint"]),
    ("cmd.exe", &["Command Prompt"]),
    ("regedit.exe", &["Registry Editor"]),
    ("msconfig.exe", &["System Configuration"]),
    ("snippingtool.exe", &["Snipping Tool"]),
    ("charmap.exe", &["Character Map"]),
    ("magnify.exe", &["Magnifier"]),
    ("osk.exe", &["On-Screen Keyboard"]),
    ("narrator.exe", &["Narrator"]),
    ("mstsc.exe", &["Remote Desktop Connection"]),
    ("perfmon.exe", &["Performance Monitor"]),
    ("resmon.exe", &["Resource Monitor"]),
    ("cleanmgr.exe", &["Disk Cleanup"]),
    ("dfrgui.exe", &["Defragment and Optimize Drives"]),
    ("msinfo32.exe", &["System Information"]),
    ("calc.exe", &["Calculator"]),
    ("windows.immersivecontrolpanel", &["Settings"]),
    ("microsoft.windowscalculator", &["Calculator"]),
    ("microsoft.windowsstore", &["Microsoft Store", "Store"]),
    (
        "microsoft.windowsterminal",
        &["Terminal", "Windows Terminal"],
    ),
    ("microsoft.paint", &["Paint"]),
    ("microsoft.windowsnotepad", &["Notepad"]),
    ("microsoft.screensketch", &["Snipping Tool"]),
    ("microsoft.windows.photos", &["Photos"]),
    ("microsoft.windowsalarms", &["Clock", "Alarms"]),
    ("microsoft.windowscamera", &["Camera"]),
    ("microsoft.windowssoundrecorder", &["Sound Recorder"]),
    ("microsoft.windowscommunicationsapps", &["Mail", "Calendar"]),
    ("microsoft.bingweather", &["Weather"]),
    ("microsoft.zunemusic", &["Media Player"]),
    ("microsoft.zunevideo", &["Movies & TV"]),
    ("microsoft.windowsmaps", &["Maps"]),
    ("microsoft.gethelp", &["Get Help"]),
    ("microsoft.getstarted", &["Tips"]),
    ("microsoft.microsoftstickynotes", &["Sticky Notes"]),
    ("microsoft.todos", &["To Do"]),
    ("microsoft.windowsfeedbackhub", &["Feedback Hub"]),
    ("microsoft.sechealthui", &["Windows Security"]),
    ("microsoft.yourphone", &["Phone Link"]),
    ("microsoft.outlookforwindows", &["Outlook"]),
    ("microsoft.windows.devhome", &["Dev Home"]),
    ("microsoftcorporationii.quickassist", &["Quick Assist"]),
];

fn aliases(key: &str) -> &'static [&'static str] {
    ALIASES
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map_or(&[], |(_, names)| *names)
}

/// Shortcut targets that are documents, not applications.
const DOCUMENT_EXTENSIONS: &[&str] = &[
    "chm", "txt", "pdf", "htm", "html", "url", "ini", "log", "rtf", "md", "hlp",
];

/// COM initialized for the current thread while alive.
struct Com(bool);

impl Com {
    fn init() -> Self {
        // SAFETY: plain COM initialization of the current thread; balanced by
        // `CoUninitialize` in `drop` when it succeeded.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        Self(hr.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` of `init`.
            unsafe { CoUninitialize() };
        }
    }
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    // SAFETY: `id` is a valid GUID; the returned string is freed with
    // `CoTaskMemFree` after being copied.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let path = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as *const _));
        path
    }
}

fn start_menu_dirs() -> Vec<PathBuf> {
    [&FOLDERID_Programs, &FOLDERID_CommonPrograms]
        .into_iter()
        .filter_map(known_folder)
        .collect()
}

pub fn watch_roots() -> Vec<PathBuf> {
    start_menu_dirs()
}

pub fn scan() -> Vec<App> {
    let _com = Com::init();
    let mut apps = Vec::new();
    let mut seen_names = HashSet::new();
    for root in start_menu_dirs() {
        let mut files = Vec::new();
        collect_shortcuts(&root, 0, &mut files);
        files.sort();
        for file in files {
            if let Some(app) = shortcut_app(&root, &file)
                && seen_names.insert(app.name.to_lowercase())
            {
                apps.push(app);
            }
        }
    }
    for app in store_apps() {
        if seen_names.insert(app.name.to_lowercase()) {
            apps.push(app);
        }
    }
    apps
}

fn collect_shortcuts(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            collect_shortcuts(&path, depth + 1, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        {
            out.push(path);
        }
    }
}

/// Name shown by the shell (localized through `desktop.ini`, no `.lnk`).
fn display_name(path: &Path) -> Option<String> {
    let wide_path = wide(path.as_os_str());
    let mut info = SHFILEINFOW::default();
    // SAFETY: `wide_path` is NUL-terminated and `info` is a valid, correctly
    // sized output structure for the duration of the call.
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide_path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            u32::try_from(std::mem::size_of::<SHFILEINFOW>()).unwrap_or(0),
            SHGFI_DISPLAYNAME,
        )
    };
    let name = from_wide(&info.szDisplayName);
    (ok != 0 && !name.is_empty()).then_some(name)
}

/// Target of a shortcut (empty for "advertised" installer shortcuts).
fn shortcut_target(path: &Path) -> Option<PathBuf> {
    let wide_path = wide(path.as_os_str());
    // SAFETY: COM is initialized on this thread (`Com`); the interfaces are
    // released when dropped; the buffers outlive the calls.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = windows::core::Interface::cast(&link).ok()?;
        file.Load(PCWSTR(wide_path.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        let mut data = WIN32_FIND_DATAW::default();
        link.GetPath(&mut buf, &mut data, 0).ok()?;
        let target = from_wide(&buf);
        (!target.is_empty()).then(|| PathBuf::from(target))
    }
}

fn is_uninstaller(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["uninstall", "désinstall", "desinstall", "unins000"]
        .iter()
        .any(|w| lower.contains(w))
}

fn shortcut_app(root: &Path, path: &Path) -> Option<App> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let name = display_name(path).unwrap_or_else(|| stem.clone());
    if is_uninstaller(&name) || is_uninstaller(&stem) {
        return None;
    }
    let target = shortcut_target(path);
    let target_file = target
        .as_deref()
        .and_then(Path::file_name)
        .map(|f| f.to_string_lossy().into_owned());
    if let Some(t) = &target {
        let ext = t
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if DOCUMENT_EXTENSIONS.contains(&ext.as_str()) {
            return None;
        }
        if target_file.as_deref().is_some_and(is_uninstaller) {
            return None;
        }
    }

    let mut alternatives = vec![stem];
    if let Some(file) = &target_file {
        alternatives.extend(aliases(file).iter().map(|a| (*a).to_owned()));
        if let Some(exe_stem) = Path::new(file).file_stem() {
            alternatives.push(exe_stem.to_string_lossy().into_owned());
        }
    }
    let mut seen = HashSet::from([name.to_lowercase()]);
    alternatives.retain(|a| seen.insert(a.to_lowercase()));

    let rel = path.strip_prefix(root).unwrap_or(path);
    let can_elevate = target
        .as_deref()
        .is_none_or(|t| t.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")));
    Some(App {
        id: format!("app:lnk:{}", rel.to_string_lossy().replace('\\', "/")),
        name,
        localized: Vec::new(),
        alternatives,
        target: LaunchTarget::Path {
            path: path.to_owned(),
            args: Vec::new(),
        },
        icon: AppIcon::Shell(path.to_string_lossy().into_owned()),
        can_elevate,
    })
}

fn item_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    // SAFETY: COM is initialized; the returned string is copied then freed
    // with `CoTaskMemFree`.
    unsafe {
        let p = item.GetDisplayName(kind).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// Packaged (Store / MSIX) apps of `shell:AppsFolder`: their parsing name is
/// an AppUserModelID, `PackageFamilyName!AppId`.
fn store_apps() -> Vec<App> {
    let mut apps = Vec::new();
    // SAFETY: COM is initialized on this thread by the caller; interfaces are
    // released when dropped; `Next` writes at most one item into `slot`.
    unsafe {
        let Ok(folder) =
            SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)
        else {
            return apps;
        };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else {
            return apps;
        };
        loop {
            let mut slot = [None];
            let mut fetched = 0;
            if items.Next(&mut slot, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = slot[0].take() else {
                break;
            };
            let (Some(name), Some(aumid)) = (
                item_name(&item, SIGDN_NORMALDISPLAY),
                item_name(&item, SIGDN_PARENTRELATIVEPARSING),
            ) else {
                continue;
            };
            if let Some(app) = store_app(name, aumid) {
                apps.push(app);
            }
        }
    }
    apps
}

fn store_app(name: String, aumid: String) -> Option<App> {
    // Desktop apps also appear here; they come from the shortcuts already.
    let (family, _) = aumid.split_once('!')?;
    if name.is_empty() || is_uninstaller(&name) {
        return None;
    }
    let package = family.split('_').next().unwrap_or(family);
    let mut alternatives: Vec<String> = aliases(package).iter().map(|a| (*a).to_owned()).collect();
    // `Microsoft.WindowsCalculator` → `WindowsCalculator`, unless it is a
    // code (`4297127D64EC6`, `CBS`, a GUID…).
    if let Some(last) = package.rsplit('.').next()
        && is_meaningful_word(last)
    {
        alternatives.push(last.to_owned());
    }
    let mut seen = HashSet::from([name.to_lowercase()]);
    alternatives.retain(|a| seen.insert(a.to_lowercase()));
    Some(App {
        id: format!("app:uwp:{aumid}"),
        icon: AppIcon::Shell(format!("shell:AppsFolder\\{aumid}")),
        target: LaunchTarget::AppUserModelId(aumid),
        name,
        localized: Vec::new(),
        alternatives,
        can_elevate: false,
    })
}

/// A package name part that reads like a word: letters with some lowercase,
/// not a hexadecimal code.
fn is_meaningful_word(s: &str) -> bool {
    s.len() >= 4
        && s.chars().all(char::is_alphanumeric)
        && s.chars().any(|c| c.is_ascii_lowercase())
        && !s.chars().all(|c| c.is_ascii_hexdigit())
}

// ---------------------------------------------------------------- icons

pub fn load_icon(icon: &AppIcon, size: u32) -> Option<IconRgba> {
    let AppIcon::Shell(name) = icon else {
        return None;
    };
    let _com = Com::init();
    let wide_name = wide(std::ffi::OsStr::new(name));
    let side = i32::try_from(size).ok()?;
    // SAFETY: COM is initialized; `wide_name` is NUL-terminated; the bitmap
    // returned by `GetImage` is owned here and deleted after being read.
    let bitmap = unsafe {
        let item: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide_name.as_ptr()), None).ok()?;
        item.GetImage(
            SIZE { cx: side, cy: side },
            SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
        )
        .ok()?
    };
    let pixels = read_bitmap(bitmap);
    // SAFETY: the bitmap belongs to us and is no longer used.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
    }
    image::fit(pixels?, size)
}

/// Reads a 32-bit bitmap as straight-alpha RGBA.
fn read_bitmap(bitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<IconRgba> {
    let mut info = BITMAP::default();
    // SAFETY: `info` is a valid BITMAP output of the size given.
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            i32::try_from(std::mem::size_of::<BITMAP>()).ok()?,
            Some((&raw mut info).cast()),
        )
    };
    if got == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
        return None;
    }
    let (width, height) = (info.bmWidth, info.bmHeight.abs());
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).ok()?,
            biWidth: width,
            // Negative height: rows from top to bottom.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let w = u32::try_from(width).ok()?;
    let h = u32::try_from(height).ok()?;
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    // SAFETY: the screen DC is released below; `bgra` holds exactly
    // `height` rows of `width` 32-bit pixels as described by `header`.
    let rows = unsafe {
        let dc = GetDC(None);
        let rows = GetDIBits(
            dc,
            bitmap,
            0,
            h,
            Some(bgra.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        rows
    };
    if rows == 0 {
        return None;
    }
    Some(IconRgba {
        width: w,
        height: h,
        rgba: bgra_to_rgba(&bgra),
    })
}

/// Shell bitmaps are usually premultiplied BGRA; old icons have no alpha at
/// all (every alpha byte 0).
fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let no_alpha = bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
    let premultiplied = !no_alpha
        && bgra
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    bgra.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let (b, g, r, a) = (p[0], p[1], p[2], p[3]);
            if no_alpha {
                return [r, g, b, 255];
            }
            if premultiplied && a > 0 && a < 255 {
                let un = |c: u8| u8::try_from(u32::from(c) * 255 / u32::from(a)).unwrap_or(255);
                return [un(r), un(g), un(b), a];
            }
            [r, g, b, a]
        })
        .collect()
}

// ---------------------------------------------------------------- launch

pub fn launch(target: &LaunchTarget, elevated: bool) -> std::io::Result<()> {
    let _com = Com::init();
    let (file, params) = match target {
        LaunchTarget::Path { path, args } => (
            path.as_os_str().to_owned(),
            args.iter()
                .map(|a| quote_arg(a))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        LaunchTarget::AppUserModelId(aumid) => {
            (format!("shell:AppsFolder\\{aumid}").into(), String::new())
        }
        LaunchTarget::DesktopEntry(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                ".desktop files only exist on Linux",
            ));
        }
    };
    let file = wide(&file);
    let params = wide(std::ffi::OsStr::new(&params));
    let verb = if elevated { w!("runas") } else { w!("open") };
    // SAFETY: every string is NUL-terminated and alive for the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            verb,
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value > 32 on success.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Quotes an argument for the Windows command line when needed.
fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_case_insensitive() {
        assert_eq!(aliases("Explorer.EXE"), ["File Explorer", "Explorer"]);
        assert_eq!(aliases("windows.immersivecontrolpanel"), ["Settings"]);
        assert!(aliases("firefox.exe").is_empty());
    }

    #[test]
    fn store_app_names() {
        let app = store_app(
            "Calculatrice".into(),
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into(),
        )
        .expect("store app");
        assert_eq!(app.alternatives, ["Calculator", "WindowsCalculator"]);
        assert_eq!(
            app.target,
            LaunchTarget::AppUserModelId("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into())
        );
        // Desktop apps of the Apps folder are not Store apps.
        assert!(store_app("Code".into(), "Microsoft.VisualStudioCode".into()).is_none());
        let codes = [
            "Python.3.12",
            "Microsoft.4297127D64EC6",
            "nzxt.com-FA26FAF8",
            "MicrosoftWindows.Client.CBS",
        ];
        for package in codes {
            let app = store_app("X".into(), format!("{package}_abc!App")).expect("store app");
            assert!(
                app.alternatives.is_empty(),
                "{package}: {:?}",
                app.alternatives
            );
        }
    }

    #[test]
    fn uninstallers_are_skipped() {
        assert!(is_uninstaller("Uninstall Foo"));
        assert!(is_uninstaller("Désinstaller Foo"));
        assert!(!is_uninstaller("Firefox"));
    }

    #[test]
    fn premultiplied_pixels_are_restored() {
        // Half-transparent pure red, premultiplied: (B,G,R,A) = (0,0,128,128).
        assert_eq!(
            bgra_to_rgba(&[0, 0, 128, 128, 0, 0, 0, 0]),
            [255, 0, 0, 128, 0, 0, 0, 0]
        );
        // No alpha at all: opaque.
        assert_eq!(bgra_to_rgba(&[1, 2, 3, 0]), [3, 2, 1, 255]);
    }

    #[test]
    fn arguments_are_quoted() {
        assert_eq!(quote_arg("simple"), "simple");
        assert_eq!(quote_arg("with space"), "\"with space\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_arg("C:\\dir with\\"), "\"C:\\dir with\\\\\"");
        assert_eq!(quote_arg(""), "\"\"");
    }

    /// Lists the real apps of this machine (run on Windows, `--ignored`).
    #[test]
    #[ignore = "depends on the installed apps"]
    fn real_scan() {
        let apps = scan();
        for app in &apps {
            println!("{} | {:?} | {:?}", app.name, app.alternatives, app.target);
        }
        assert!(!apps.is_empty());
        let with_icon = apps
            .iter()
            .take(20)
            .filter(|a| load_icon(&a.icon, 48).is_some())
            .count();
        println!("{with_icon}/20 icons");
        assert!(with_icon > 0);
    }
}

//! Build script:
//! - embeds the version, channel, commit and build date (`flair --version`);
//! - on Windows targets, renders `flair.ico` from the SVG logo and embeds it
//!   in the executable with its version information.
//!
//! The release workflow sets `FLAIR_VERSION` (`0.4.0-main.57`, `0.4.0`) and
//! `FLAIR_CHANNEL` (`main`, `stable`); local builds are `<version>-dev`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixed application icon color, Flair's purple: readable on white (4.12:1)
/// and on a dark taskbar (3.77:1 on #202020), above the 3:1 recommended for
/// graphical objects. Keep in sync with `src/assets.rs`.
const APP_ICON_COLOR: &str = "#AD50D4";

fn main() {
    for var in [
        "FLAIR_VERSION",
        "FLAIR_CHANNEL",
        "GITHUB_SHA",
        "SOURCE_DATE_EPOCH",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    println!("cargo:rerun-if-changed=../../assets/icons");

    let package = env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let version = env::var("FLAIR_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("{package}-dev"));
    let channel = env::var("FLAIR_CHANNEL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "dev".into());
    let commit = env::var("GITHUB_SHA")
        .ok()
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .map(|c| c.trim().chars().take(7).collect::<String>())
        .unwrap_or_else(|| "unknown".into());
    let epoch = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });

    println!("cargo:rustc-env=FLAIR_BUILD_VERSION={version}");
    println!("cargo:rustc-env=FLAIR_BUILD_CHANNEL={channel}");
    println!("cargo:rustc-env=FLAIR_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=FLAIR_BUILD_EPOCH={epoch}");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_default());
        match write_ico(&out.join("flair.ico")) {
            Ok(ico) => embed_resources(&ico, &version),
            Err(e) => println!("cargo:warning=no application icon: {e}"),
        }
    }
}

fn render(svg: &str, size: u32) -> Result<ico::IconImage, String> {
    let svg = svg.replace("currentColor", APP_ICON_COLOR);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("bad size")?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok(ico::IconImage::from_rgba_data(size, size, rgba))
}

/// 16 px from the simplified logo, 20 to 256 px from the main one.
fn write_ico(path: &Path) -> Result<PathBuf, String> {
    let icons = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons");
    let main = std::fs::read_to_string(icons.join("flair.svg")).map_err(|e| e.to_string())?;
    let tiny = std::fs::read_to_string(icons.join("flair-tiny.svg")).map_err(|e| e.to_string())?;
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    dir.add_entry(ico::IconDirEntry::encode(&render(&tiny, 16)?).map_err(|e| e.to_string())?);
    for size in [20, 24, 32, 48, 64, 128, 256] {
        dir.add_entry(ico::IconDirEntry::encode(&render(&main, size)?).map_err(|e| e.to_string())?);
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    dir.write(file).map_err(|e| e.to_string())?;
    Ok(path.to_path_buf())
}

fn embed_resources(ico: &Path, version: &str) {
    let mut res = winresource::WindowsResource::new();
    res.set_icon(&ico.to_string_lossy())
        .set("ProductName", "Flair")
        .set("FileDescription", "Flair")
        .set("CompanyName", "Flair")
        .set("ProductVersion", version)
        .set("OriginalFilename", "flair.exe")
        .set("LegalCopyright", "Flair contributors");
    if let Err(e) = res.compile() {
        println!("cargo:warning=cannot embed Windows resources: {e}");
    }
}

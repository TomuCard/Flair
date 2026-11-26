//! Tray icon (notification area) with its menu.
//!
//! - Windows: `tray-icon`, created on the UI thread (its messages are pumped
//!   by the UI event loop).
//! - Linux: StatusNotifierItem over D-Bus (`ksni`), served by a single-thread
//!   Tokio runtime that sleeps until D-Bus traffic arrives.

use std::sync::Arc;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(windows)]
use windows as imp;

/// What the user picked in the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// Left click on the icon.
    Toggle,
    Show,
    Reindex,
    OpenConfig,
    CheckUpdate,
    Autostart,
    Stats,
    Quit,
}

/// Menu labels, in the interface language.
#[derive(Debug, Clone)]
pub struct Labels {
    pub tooltip: String,
    pub show: String,
    pub reindex: String,
    pub open_config: String,
    pub check_update: String,
    pub autostart: String,
    pub stats: String,
    pub quit: String,
}

/// Features not implemented yet appear disabled in the menu.
#[derive(Debug, Clone, Copy)]
pub struct Enabled {
    pub reindex: bool,
    pub check_update: bool,
}

/// Called with the user's choice, from a background thread.
pub type Callback = Arc<dyn Fn(TrayAction) + Send + Sync>;

/// Straight-alpha RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Tray {
    inner: imp::Tray,
}

impl Tray {
    pub fn new(
        labels: &Labels,
        enabled: Enabled,
        icon: IconRgba,
        autostart_checked: bool,
        on_action: Callback,
    ) -> Result<Self, String> {
        imp::Tray::new(labels, enabled, icon, autostart_checked, on_action)
            .map(|inner| Self { inner })
    }

    pub fn set_autostart_checked(&mut self, checked: bool) {
        self.inner.set_autostart_checked(checked);
    }

    pub fn set_icon(&mut self, icon: IconRgba) {
        self.inner.set_icon(icon);
    }
}

/// Size of the tray icon, in pixels.
pub fn icon_size() -> u32 {
    imp::icon_size()
}

/// Renders a single-color SVG (`currentColor`) at `size` px in `rgb`.
pub fn render_svg(svg: &str, rgb: [u8; 3], size: u32) -> Result<IconRgba, String> {
    let [r, g, b] = rgb;
    let svg = svg.replace("currentColor", &format!("#{r:02x}{g:02x}{b:02x}"));
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let mut pixmap = Pixmap::new(size, size).ok_or("invalid icon size")?;
    #[allow(clippy::cast_precision_loss)]
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied alpha; trays want straight alpha.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok(IconRgba {
        width: size,
        height: size,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="8" fill="currentColor"/></svg>"#;

    #[test]
    fn renders_in_the_requested_color() {
        let icon = render_svg(SVG, [0xE0, 0x6A, 0x28], 32).expect("valid");
        assert_eq!(
            (icon.width, icon.height, icon.rgba.len()),
            (32, 32, 32 * 32 * 4)
        );
        // Top half painted with the color, bottom half transparent.
        assert_eq!(&icon.rgba[..4], &[0xE0, 0x6A, 0x28, 0xFF]);
        assert_eq!(icon.rgba[icon.rgba.len() - 1], 0);
        assert!(render_svg("<not svg", [0, 0, 0], 16).is_err());
    }
}

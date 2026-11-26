//! Linux tray icon: StatusNotifierItem over D-Bus with `ksni`.
//!
//! D-Bus is served by a single-thread Tokio runtime on a dedicated thread.
//! It sleeps until D-Bus traffic arrives. The async-io stack used by
//! `tray-icon`'s KSNI backend was rejected: its `blocking` thread pool keeps
//! one thread waking up every 500 ms forever.

use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
use ksni::{Handle, TrayMethods};
use tokio::runtime;

use super::{Callback, Enabled, IconRgba, Labels, TrayAction};

struct FlairTray {
    labels: Labels,
    enabled: Enabled,
    icon: IconRgba,
    autostart: bool,
    on_action: Callback,
}

impl FlairTray {
    fn item(&self, label: &str, enabled: bool, action: TrayAction) -> MenuItem<Self> {
        StandardItem {
            label: label.to_owned(),
            enabled,
            activate: Box::new(move |tray: &mut Self| (tray.on_action)(action)),
            ..Default::default()
        }
        .into()
    }
}

/// ARGB32 in network byte order, as StatusNotifierItem wants it.
fn argb(icon: &IconRgba) -> ksni::Icon {
    let data = icon
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [a, r, g, b])
        .collect();
    ksni::Icon {
        width: i32::try_from(icon.width).unwrap_or(0),
        height: i32::try_from(icon.height).unwrap_or(0),
        data,
    }
}

impl ksni::Tray for FlairTray {
    fn id(&self) -> String {
        "flair".into()
    }

    fn title(&self) -> String {
        "Flair".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![argb(&self.icon)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.labels.tooltip.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.on_action)(TrayAction::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let l = &self.labels;
        vec![
            self.item(&l.show, true, TrayAction::Show),
            MenuItem::Separator,
            self.item(&l.reindex, self.enabled.reindex, TrayAction::Reindex),
            self.item(&l.open_config, true, TrayAction::OpenConfig),
            self.item(
                &l.check_update,
                self.enabled.check_update,
                TrayAction::CheckUpdate,
            ),
            CheckmarkItem {
                label: l.autostart.clone(),
                checked: self.autostart,
                activate: Box::new(|tray: &mut Self| (tray.on_action)(TrayAction::Autostart)),
                ..Default::default()
            }
            .into(),
            self.item(&l.stats, true, TrayAction::Stats),
            MenuItem::Separator,
            self.item(&l.quit, true, TrayAction::Quit),
        ]
    }
}

pub struct Tray {
    runtime: runtime::Handle,
    handle: Handle<FlairTray>,
}

impl Tray {
    pub fn new(
        labels: &Labels,
        enabled: Enabled,
        icon: IconRgba,
        autostart: bool,
        on_action: Callback,
    ) -> Result<Self, String> {
        let runtime = crate::dbus::runtime()?;
        let tray = FlairTray {
            labels: labels.clone(),
            enabled,
            icon,
            autostart,
            on_action,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        runtime.spawn(async move {
            let _ = tx.send(tray.spawn().await.map_err(|e| e.to_string()));
        });
        let handle = rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { runtime, handle })
    }

    pub fn set_autostart_checked(&mut self, checked: bool) {
        let handle = self.handle.clone();
        self.runtime.spawn(async move {
            handle.update(|tray| tray.autostart = checked).await;
        });
    }

    pub fn set_icon(&mut self, icon: IconRgba) {
        let handle = self.handle.clone();
        self.runtime.spawn(async move {
            handle.update(|tray| tray.icon = icon).await;
        });
    }
}

pub fn icon_size() -> u32 {
    // StatusNotifier hosts scale it; 48 px stays sharp on HiDPI panels.
    48
}

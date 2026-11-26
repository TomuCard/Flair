//! Windows tray icon through `tray-icon`.

use std::sync::{Arc, Mutex, OnceLock};

use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{Callback, Enabled, IconRgba, Labels, TrayAction};

const IDS: [(&str, TrayAction); 7] = [
    ("show", TrayAction::Show),
    ("reindex", TrayAction::Reindex),
    ("open-config", TrayAction::OpenConfig),
    ("check-update", TrayAction::CheckUpdate),
    ("autostart", TrayAction::Autostart),
    ("stats", TrayAction::Stats),
    ("quit", TrayAction::Quit),
];

/// `tray-icon` accepts one click handler per process, set once: it
/// dispatches to the callback of the current tray.
static CURRENT: Mutex<Option<Callback>> = Mutex::new(None);
static CLICK_HANDLER: OnceLock<()> = OnceLock::new();

fn dispatch(action: TrayAction) {
    let callback = CURRENT.lock().ok().and_then(|c| c.clone());
    if let Some(callback) = callback {
        callback(action);
    }
}

fn icon(icon: IconRgba) -> Result<Icon, String> {
    Icon::from_rgba(icon.rgba, icon.width, icon.height).map_err(|e| e.to_string())
}

pub struct Tray {
    icon: TrayIcon,
    autostart: CheckMenuItem,
}

impl Tray {
    pub fn new(
        labels: &Labels,
        enabled: Enabled,
        rgba: IconRgba,
        autostart_checked: bool,
        on_action: Callback,
    ) -> Result<Self, String> {
        let menu = Menu::new();
        let item = |id: &str, text: &str, on: bool| MenuItem::with_id(id, text, on, None);
        let autostart = CheckMenuItem::with_id(
            "autostart",
            &labels.autostart,
            true,
            autostart_checked,
            None,
        );
        let items: [&dyn IsMenuItem; 9] = [
            &item("show", &labels.show, true),
            &PredefinedMenuItem::separator(),
            &item("reindex", &labels.reindex, enabled.reindex),
            &item("open-config", &labels.open_config, true),
            &item("check-update", &labels.check_update, enabled.check_update),
            &autostart,
            &item("stats", &labels.stats, true),
            &PredefinedMenuItem::separator(),
            &item("quit", &labels.quit, true),
        ];
        menu.append_items(&items).map_err(|e| e.to_string())?;

        if let Ok(mut current) = CURRENT.lock() {
            *current = Some(on_action);
        }
        MenuEvent::set_event_handler(Some(Arc::new(|event: MenuEvent| {
            if let Some((_, action)) = IDS.iter().find(|(id, _)| event.id.0 == *id) {
                dispatch(*action);
            }
        })));
        CLICK_HANDLER.get_or_init(|| {
            TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    dispatch(TrayAction::Toggle);
                }
            }));
        });

        let tray = TrayIconBuilder::new()
            .with_id("flair")
            .with_tooltip(&labels.tooltip)
            .with_icon(icon(rgba)?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            icon: tray,
            autostart,
        })
    }

    pub fn set_autostart_checked(&mut self, checked: bool) {
        self.autostart.set_checked(checked);
    }

    pub fn set_icon(&mut self, rgba: IconRgba) {
        match icon(rgba).and_then(|i| self.icon.set_icon(Some(i)).map_err(|e| e.to_string())) {
            Ok(()) => {}
            Err(e) => tracing::warn!("cannot change the tray icon: {e}"),
        }
    }
}

pub fn icon_size() -> u32 {
    use ::windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    // SAFETY: no arguments besides a constant index.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    u32::try_from(size).ok().filter(|s| *s > 0).unwrap_or(16)
}

//! System appearance: the taskbar color (tray icon) and the dark or light
//! preference of applications, followed live (registry notification under
//! Windows, XDG Settings portal under Linux: no polling).

use std::sync::Arc;

/// Called with `true` when the system switches to dark, `false` to light.
pub type OnChange = Arc<dyn Fn(bool) + Send + Sync>;

/// The applications' preference: `Some(true)` dark, `Some(false)` light,
/// `None` unknown (no portal, no setting).
pub fn system_dark() -> Option<bool> {
    imp::system_dark()
}

/// Calls `on_change` each time the preference changes, from a background
/// thread that sleeps until the system reports a change.
pub fn watch(on_change: OnChange) {
    imp::watch(on_change);
}

/// True when the taskbar (Windows) / panel is light. Windows reads
/// `SystemUsesLightTheme`, which differs from the apps setting. Linux has no
/// standard way yet: panels are assumed dark (milestone 8 follows the portal).
pub fn taskbar_is_light() -> bool {
    imp::taskbar_is_light()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
        RRF_RT_REG_DWORD, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
    };
    use windows::core::{PCWSTR, w};

    const PERSONALIZE: PCWSTR =
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

    fn read(name: PCWSTR) -> Option<u32> {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: constant strings, out buffer of the announced size.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PERSONALIZE,
                name,
                RRF_RT_REG_DWORD,
                None,
                Some((&raw mut value).cast()),
                Some(&mut size),
            )
        };
        result.is_ok().then_some(value)
    }

    pub fn taskbar_is_light() -> bool {
        read(w!("SystemUsesLightTheme")) == Some(1)
    }

    pub fn system_dark() -> Option<bool> {
        read(w!("AppsUseLightTheme")).map(|v| v == 0)
    }

    pub fn watch(on_change: super::OnChange) {
        let spawned = std::thread::Builder::new()
            .name("theme-watch".into())
            .spawn(move || {
                let mut key = HKEY::default();
                // SAFETY: constant key path; `key` receives a handle closed below.
                let opened = unsafe {
                    RegOpenKeyExW(
                        HKEY_CURRENT_USER,
                        PERSONALIZE,
                        None,
                        KEY_NOTIFY | KEY_READ,
                        &mut key,
                    )
                };
                if opened.is_err() {
                    tracing::warn!("cannot watch the system theme");
                    return;
                }
                let mut last = system_dark();
                loop {
                    // Blocks until a value of the key changes (synchronous call).
                    // SAFETY: valid key handle, no event (synchronous).
                    let waited = unsafe {
                        RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, None, false)
                    };
                    if waited.is_err() {
                        break;
                    }
                    let now = system_dark();
                    if now != last
                        && let Some(dark) = now
                    {
                        on_change(dark);
                    }
                    last = now;
                }
                // SAFETY: the handle was opened above.
                unsafe {
                    let _ = RegCloseKey(key);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot start the theme watcher");
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use futures_lite::StreamExt;
    use zbus::zvariant::OwnedValue;

    pub fn taskbar_is_light() -> bool {
        false
    }

    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    /// `color-scheme`: 1 = prefer dark, 2 = prefer light, 0 = no preference.
    fn scheme(value: &OwnedValue) -> Option<bool> {
        // ReadOne returns the value itself; Read (older portals) wraps it once more.
        let inner = value
            .downcast_ref::<zbus::zvariant::Value<'_>>()
            .ok()
            .and_then(|v| u32::try_from(&v).ok());
        let n = u32::try_from(value).ok().or(inner)?;
        match n {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    async fn settings() -> zbus::Result<zbus::Proxy<'static>> {
        let connection = zbus::Connection::session().await?;
        zbus::Proxy::new_owned(
            connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
        .await
    }

    async fn read() -> Option<bool> {
        let proxy = settings().await.ok()?;
        let value: OwnedValue = proxy.call("ReadOne", &(NAMESPACE, KEY)).await.ok()?;
        scheme(&value)
    }

    /// GNOME without portal: `gsettings` (the `dark-light` approach, once).
    fn gsettings() -> Option<bool> {
        let out = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if text.contains("dark") {
            Some(true)
        } else if text.contains("light") {
            Some(false)
        } else {
            None
        }
    }

    pub fn system_dark() -> Option<bool> {
        let runtime = crate::dbus::runtime().ok()?;
        let (tx, rx) = std::sync::mpsc::channel();
        runtime.spawn(async move {
            let _ = tx.send(read().await);
        });
        // The portal answers in a few milliseconds; never block the start.
        rx.recv_timeout(std::time::Duration::from_millis(300))
            .ok()
            .flatten()
            .or_else(gsettings)
    }

    pub fn watch(on_change: super::OnChange) {
        let Ok(runtime) = crate::dbus::runtime() else {
            return;
        };
        runtime.spawn(async move {
            let Ok(proxy) = settings().await else {
                tracing::info!("no settings portal: the system theme is not followed");
                return;
            };
            let Ok(mut changes) = proxy.receive_signal("SettingChanged").await else {
                return;
            };
            // Sleeps until the portal emits a signal.
            while let Some(message) = changes.next().await {
                let Ok((namespace, key, value)) =
                    message.body().deserialize::<(String, String, OwnedValue)>()
                else {
                    continue;
                };
                if namespace == NAMESPACE
                    && key == KEY
                    && let Some(dark) = scheme(&value)
                {
                    on_change(dark);
                }
            }
        });
    }
}

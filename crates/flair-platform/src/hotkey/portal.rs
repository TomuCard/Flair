//! Wayland: the XDG GlobalShortcuts portal (GNOME 48+, KDE Plasma 5.27+,
//! Hyprland…). Applications cannot grab keys there: Flair asks the desktop to
//! bind its shortcut (the desktop may show a confirmation and let the user
//! choose another key), then listens to the `Activated` signal. The D-Bus
//! task sleeps until a signal arrives.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use flair_core::settings::Hotkey;
use futures_lite::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::{Callback, HotkeyError};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// Id of Flair's only shortcut.
const SHORTCUT_ID: &str = "toggle";

/// Kept while the shortcut is wanted; dropping it stops listening.
pub struct Registration {
    stop: Arc<AtomicBool>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// `Alt+Space` → `ALT+space` (shortcuts specification of the portals).
pub fn trigger(hotkey: &Hotkey) -> String {
    let mut parts = Vec::new();
    if hotkey.ctrl {
        parts.push("CTRL".to_owned());
    }
    if hotkey.alt {
        parts.push("ALT".to_owned());
    }
    if hotkey.shift {
        parts.push("SHIFT".to_owned());
    }
    if hotkey.super_key {
        parts.push("LOGO".to_owned());
    }
    let key = match hotkey.key.as_str() {
        "Space" => "space".to_owned(),
        "Enter" => "Return".to_owned(),
        "Backspace" => "BackSpace".to_owned(),
        "PageUp" => "Prior".to_owned(),
        "PageDown" => "Next".to_owned(),
        "Backquote" => "grave".to_owned(),
        k if k.len() == 1 => k.to_lowercase(),
        k => k.to_owned(),
    };
    parts.push(key);
    parts.join("+")
}

/// Waits for the `Response` of a portal request: `(code, results)`.
async fn request(
    connection: &zbus::Connection,
    token: &str,
    call: impl std::future::Future<Output = zbus::Result<OwnedObjectPath>>,
) -> zbus::Result<(u32, HashMap<String, OwnedValue>)> {
    // The request object path is known in advance: subscribe first, so the
    // answer cannot be missed.
    let sender = connection
        .unique_name()
        .map(|n| n.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    let path = format!("{PATH}/request/{sender}/{token}");
    let proxy = zbus::Proxy::new(
        connection,
        DESTINATION,
        path,
        "org.freedesktop.portal.Request",
    )
    .await?;
    let mut responses = proxy.receive_signal("Response").await?;
    call.await?;
    let message = responses
        .next()
        .await
        .ok_or_else(|| zbus::Error::Failure("no portal response".into()))?;
    message.body().deserialize()
}

async fn bind(
    hotkey: Hotkey,
    description: String,
    on_press: Callback,
    stop: Arc<AtomicBool>,
) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let portal = zbus::Proxy::new(&connection, DESTINATION, PATH, INTERFACE).await?;
    let pid = std::process::id();

    // 1. A session.
    let token = format!("flair_session_{pid}");
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("session_handle_token", Value::from(token.as_str()));
    let (code, results) = request(&connection, &token, async {
        portal.call("CreateSession", &(options,)).await
    })
    .await?;
    let session: OwnedObjectPath = results
        .get("session_handle")
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| String::try_from(v).ok())
        .and_then(|s| OwnedObjectPath::try_from(s).ok())
        .filter(|_| code == 0)
        .ok_or_else(|| zbus::Error::Failure(format!("session refused ({code})")))?;

    // 2. The shortcut, with its preferred trigger (the desktop decides).
    let token = format!("flair_bind_{pid}");
    let mut shortcut: HashMap<&str, Value<'_>> = HashMap::new();
    shortcut.insert("description", Value::from(description.as_str()));
    let preferred = trigger(&hotkey);
    shortcut.insert("preferred_trigger", Value::from(preferred.as_str()));
    let shortcuts = vec![(SHORTCUT_ID, shortcut)];
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let (code, _) = request(&connection, &token, async {
        portal
            .call("BindShortcuts", &(&session, shortcuts, "", options))
            .await
    })
    .await?;
    if code != 0 {
        return Err(zbus::Error::Failure(format!("shortcut refused ({code})")));
    }
    tracing::info!(trigger = %preferred, "global shortcut bound through the portal");

    // 3. Presses.
    let mut activated = portal.receive_signal("Activated").await?;
    while let Some(message) = activated.next().await {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let Ok((from, id, _time, _options)) =
            message
                .body()
                .deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()
        else {
            continue;
        };
        if from == session && id == SHORTCUT_ID {
            on_press();
        }
    }
    Ok(())
}

/// True when the desktop offers the GlobalShortcuts portal.
async fn available() -> bool {
    let Ok(connection) = zbus::Connection::session().await else {
        return false;
    };
    let Ok(portal) = zbus::Proxy::new(&connection, DESTINATION, PATH, INTERFACE).await else {
        return false;
    };
    portal.get_property::<u32>("version").await.is_ok()
}

/// Asks the portal for the shortcut. Fails at once when there is no portal;
/// the binding itself (maybe confirmed by the user) goes on in the background.
pub fn register(
    hotkey: &Hotkey,
    on_press: Callback,
    description: &str,
) -> Result<Registration, HotkeyError> {
    let runtime = crate::dbus::runtime().map_err(|e| HotkeyError::Failed(hotkey.to_string(), e))?;
    let (tx, rx) = std::sync::mpsc::channel();
    runtime.spawn(async move {
        let _ = tx.send(available().await);
    });
    let present = rx
        .recv_timeout(std::time::Duration::from_millis(500))
        .unwrap_or(false);
    if !present {
        return Err(HotkeyError::Unsupported);
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (hotkey, description, flag) = (hotkey.clone(), description.to_owned(), Arc::clone(&stop));
    runtime.spawn(async move {
        if let Err(e) = bind(hotkey, description, on_press, flag).await {
            tracing::warn!(error = %e, "global shortcut portal");
        }
    });
    Ok(Registration { stop })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_follow_the_portal_format() {
        let t = |s: &str| trigger(&Hotkey::parse(s).expect("hotkey"));
        assert_eq!(t("Alt+Space"), "ALT+space");
        assert_eq!(t("Ctrl+Shift+K"), "CTRL+SHIFT+k");
        assert_eq!(t("Super+Enter"), "LOGO+Return");
        assert_eq!(t("F12"), "F12");
    }
}

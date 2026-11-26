//! Windows global shortcut through `global-hotkey` (RegisterHotKey).

use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

use flair_core::settings::Hotkey;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use super::{Callback, HotkeyError};

/// Current callback and shortcut id. `global-hotkey` accepts a single event
/// handler for the whole process, so it dispatches through this.
static CURRENT: Mutex<Option<(u32, Callback)>> = Mutex::new(None);
static HANDLER_SET: OnceLock<()> = OnceLock::new();

pub struct Registration {
    manager: GlobalHotKeyManager,
    hotkey: HotKey,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = self.manager.unregister(self.hotkey);
        if let Ok(mut current) = CURRENT.lock()
            && current
                .as_ref()
                .is_some_and(|(id, _)| *id == self.hotkey.id())
        {
            *current = None;
        }
    }
}

fn code(key: &str) -> Option<Code> {
    let name = match key {
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_uppercase() => format!("Key{k}"),
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() => format!("Digit{k}"),
        "Up" | "Down" | "Left" | "Right" => format!("Arrow{key}"),
        other => other.to_owned(),
    };
    Code::from_str(&name).ok()
}

pub fn register(hotkey: &Hotkey, on_press: Callback) -> Result<Registration, HotkeyError> {
    let text = hotkey.to_string();
    let key =
        code(&hotkey.key).ok_or_else(|| HotkeyError::Failed(text.clone(), "unknown key".into()))?;
    let mut mods = Modifiers::empty();
    mods.set(Modifiers::CONTROL, hotkey.ctrl);
    mods.set(Modifiers::ALT, hotkey.alt);
    mods.set(Modifiers::SHIFT, hotkey.shift);
    mods.set(Modifiers::META, hotkey.super_key);
    let hk = HotKey::new(Some(mods), key);

    let manager =
        GlobalHotKeyManager::new().map_err(|e| HotkeyError::Failed(text.clone(), e.to_string()))?;
    manager.register(hk).map_err(|e| match e {
        global_hotkey::Error::AlreadyRegistered(_) | global_hotkey::Error::FailedToRegister(_) => {
            HotkeyError::Taken(text.clone())
        }
        other => HotkeyError::Failed(text.clone(), other.to_string()),
    })?;

    if let Ok(mut current) = CURRENT.lock() {
        *current = Some((hk.id(), on_press));
    }
    HANDLER_SET.get_or_init(|| {
        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            if event.state() != HotKeyState::Pressed {
                return;
            }
            let callback = CURRENT.lock().ok().and_then(|c| {
                c.as_ref()
                    .filter(|(id, _)| *id == event.id())
                    .map(|(_, f)| f.clone())
            });
            if let Some(callback) = callback {
                callback();
            }
        }));
    });
    Ok(Registration {
        manager,
        hotkey: hk,
    })
}

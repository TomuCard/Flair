//! X11 global shortcut with `x11rb`: `XGrabKey` on the root window and a
//! thread blocked in `wait_for_event` (no polling, no timer).

use std::sync::Arc;

use flair_core::settings::Hotkey;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, Keycode, ModMask,
    Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

use super::{Callback, HotkeyError};

/// Lock modifiers that must not prevent the shortcut: Caps Lock, Num Lock.
fn ignored() -> [ModMask; 4] {
    [
        ModMask::from(0_u16),
        ModMask::LOCK,
        ModMask::M2,
        ModMask::LOCK | ModMask::M2,
    ]
}

pub struct Registration {
    conn: Arc<RustConnection>,
    root: Window,
    keycode: Keycode,
    mods: ModMask,
    /// Hidden window used to wake the event thread up when unregistering.
    waker: Window,
}

impl Drop for Registration {
    fn drop(&mut self) {
        for extra in ignored() {
            let _ = self
                .conn
                .ungrab_key(self.keycode, self.root, self.mods | extra);
        }
        // Wake the event thread up so it exits.
        let event = ClientMessageEvent::new(32, self.waker, x11rb::NONE, [0_u32; 5]);
        let _ = self
            .conn
            .send_event(false, self.waker, EventMask::NO_EVENT, event);
        let _ = self.conn.flush();
    }
}

/// X11 keysym of a canonical key name (see `flair_core::settings::Hotkey`).
fn keysym(key: &str) -> Option<u32> {
    let b = key.as_bytes();
    if b.len() == 1 && b[0].is_ascii_uppercase() {
        return Some(u32::from(b[0].to_ascii_lowercase()));
    }
    if b.len() == 1 && b[0].is_ascii_digit() {
        return Some(u32::from(b[0]));
    }
    if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then_some(0xffbe + n - 1);
    }
    Some(match key {
        "Space" => 0x20,
        "Backquote" => 0x60,
        "Enter" => 0xff0d,
        "Tab" => 0xff09,
        "Escape" => 0xff1b,
        "Backspace" => 0xff08,
        "Insert" => 0xff63,
        "Delete" => 0xffff,
        "Home" => 0xff50,
        "End" => 0xff57,
        "PageUp" => 0xff55,
        "PageDown" => 0xff56,
        "Left" => 0xff51,
        "Up" => 0xff52,
        "Right" => 0xff53,
        "Down" => 0xff54,
        _ => return None,
    })
}

fn keycode_for(conn: &RustConnection, keysym: u32) -> Option<Keycode> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let mapping = conn
        .get_keyboard_mapping(min, max - min + 1)
        .ok()?
        .reply()
        .ok()?;
    let per = usize::from(mapping.keysyms_per_keycode).max(1);
    mapping
        .keysyms
        .chunks(per)
        .position(|syms| syms.contains(&keysym))
        .and_then(|i| u8::try_from(i).ok())
        .map(|i| min + i)
}

pub fn register(hotkey: &Hotkey, on_press: Callback) -> Result<Registration, HotkeyError> {
    let text = hotkey.to_string();
    let fail = |e: &dyn std::fmt::Display| HotkeyError::Failed(text.clone(), e.to_string());

    let (conn, screen) = RustConnection::connect(None).map_err(|e| fail(&e))?;
    let conn = Arc::new(conn);
    let root = conn.setup().roots[screen].root;

    // Report a held key once, not as press/release pairs (key repeat).
    if conn
        .xkb_use_extension(1, 0)
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some_and(|r| r.supported)
    {
        let _ = conn
            .xkb_per_client_flags(
                xkb::ID::USE_CORE_KBD.into(),
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .map(|c| c.reply());
    }

    let sym = keysym(&hotkey.key).ok_or_else(|| fail(&"unknown key"))?;
    let keycode = keycode_for(&conn, sym).ok_or_else(|| fail(&"key not on this keyboard"))?;
    let mut mods = ModMask::from(0_u16);
    if hotkey.ctrl {
        mods |= ModMask::CONTROL;
    }
    if hotkey.alt {
        mods |= ModMask::M1;
    }
    if hotkey.shift {
        mods |= ModMask::SHIFT;
    }
    if hotkey.super_key {
        mods |= ModMask::M4;
    }

    for extra in ignored() {
        let grabbed = conn
            .grab_key(
                true,
                root,
                mods | extra,
                keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )
            .map_err(|e| fail(&e))?
            .check();
        if grabbed.is_err() {
            // BadAccess: another client holds this combination.
            for undo in ignored() {
                let _ = conn.ungrab_key(keycode, root, mods | undo);
            }
            let _ = conn.flush();
            return Err(HotkeyError::Taken(text));
        }
    }

    let waker = conn.generate_id().map_err(|e| fail(&e))?;
    conn.create_window(
        0,
        waker,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new(),
    )
    .map_err(|e| fail(&e))?;
    conn.flush().map_err(|e| fail(&e))?;

    let thread_conn = Arc::clone(&conn);
    let relevant = ModMask::CONTROL | ModMask::SHIFT | ModMask::M1 | ModMask::M4;
    std::thread::Builder::new()
        .name("flair-hotkey".into())
        .spawn(move || {
            let mut pressed = false;
            // Blocks until the X server sends something: no wake-up otherwise.
            while let Ok(event) = thread_conn.wait_for_event() {
                match event {
                    Event::KeyPress(e)
                        if e.detail == keycode
                            && ModMask::from(u16::from(e.state)) & relevant == mods =>
                    {
                        if !pressed {
                            pressed = true;
                            on_press();
                        }
                    }
                    Event::KeyRelease(e) if e.detail == keycode => pressed = false,
                    Event::ClientMessage(e) if e.window == waker => break,
                    _ => {}
                }
            }
        })
        .map_err(|e| fail(&e))?;

    Ok(Registration {
        conn,
        root,
        keycode,
        mods,
        waker,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms_of_canonical_names() {
        assert_eq!(keysym("Space"), Some(0x20));
        assert_eq!(keysym("K"), Some(u32::from(b'k')));
        assert_eq!(keysym("7"), Some(u32::from(b'7')));
        assert_eq!(keysym("F1"), Some(0xffbe));
        assert_eq!(keysym("F12"), Some(0xffc9));
        assert_eq!(keysym("F25"), None);
        assert_eq!(keysym("Nope"), None);
    }
}

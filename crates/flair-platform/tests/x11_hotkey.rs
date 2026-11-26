//! X11 global shortcut, end to end: registers Ctrl+Alt+Shift+F12, fakes the
//! key press with XTEST, and expects exactly one callback (key repeat must not
//! trigger it again). Needs a real X server: `cargo test -- --ignored`.
#![cfg(target_os = "linux")]
#![allow(clippy::expect_used)]

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use flair_core::settings::Hotkey;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xtest::ConnectionExt as _;

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

fn keycode(conn: &impl Connection, keysym: u32) -> u8 {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = conn
        .get_keyboard_mapping(min, max - min + 1)
        .expect("request")
        .reply()
        .expect("reply");
    let per = usize::from(map.keysyms_per_keycode);
    let i = map
        .keysyms
        .chunks(per)
        .position(|s| s.contains(&keysym))
        .expect("key exists");
    min + u8::try_from(i).expect("small")
}

#[test]
#[ignore = "needs an X11 server"]
fn grabbed_shortcut_fires_once_per_press() {
    let hotkey = Hotkey::parse("Ctrl+Alt+Shift+F12").expect("valid");
    let (tx, rx) = mpsc::channel();
    let registration = flair_platform::hotkey::register_x11(
        &hotkey,
        Arc::new(move || {
            let _ = tx.send(());
        }),
    )
    .expect("registered");

    let (conn, _) = x11rb::connect(None).expect("X server");
    let codes = [
        keycode(&conn, 0xffe3), // Control_L
        keycode(&conn, 0xffe9), // Alt_L
        keycode(&conn, 0xffe1), // Shift_L
    ];
    let f12 = keycode(&conn, 0xffc9);
    let fake = |kind: u8, code: u8| {
        conn.xtest_fake_input(kind, code, 0, x11rb::NONE, 0, 0, 0)
            .expect("fake input");
    };
    for c in codes {
        fake(KEY_PRESS, c);
    }
    // Press, then two presses without release (auto-repeat), then release.
    fake(KEY_PRESS, f12);
    fake(KEY_PRESS, f12);
    fake(KEY_PRESS, f12);
    fake(KEY_RELEASE, f12);
    for c in codes.iter().rev() {
        fake(KEY_RELEASE, *c);
    }
    conn.flush().expect("flush");

    rx.recv_timeout(Duration::from_secs(2))
        .expect("shortcut fired");
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "fired more than once for one press"
    );
    drop(registration);
}

#[test]
#[ignore = "needs an X11 server"]
fn shortcut_held_by_another_client_is_reported_as_taken() {
    let hotkey = Hotkey::parse("Ctrl+Alt+Shift+F11").expect("valid");
    let first = flair_platform::hotkey::register_x11(&hotkey, Arc::new(|| {})).expect("first");
    let second = flair_platform::hotkey::register_x11(&hotkey, Arc::new(|| {}));
    assert!(
        matches!(second, Err(flair_platform::hotkey::HotkeyError::Taken(ref k)) if k == "Ctrl+Alt+Shift+F11"),
        "second registration must fail as Taken"
    );
    drop(first);
    // Released: can be registered again.
    let again = flair_platform::hotkey::register_x11(&hotkey, Arc::new(|| {}));
    assert!(again.is_ok(), "{:?}", again.err());
}

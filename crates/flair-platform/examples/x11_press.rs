//! Development helper: presses a shortcut through XTEST, as if typed on the
//! keyboard (`cargo run -p flair-platform --example x11_press -- alt space`).
//! Key names are X11 keysym values in hex or the names below.

#[cfg(target_os = "linux")]
fn main() {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::protocol::xtest::ConnectionExt as _;

    let keysym = |name: &str| -> u32 {
        match name {
            "ctrl" => 0xffe3,
            "alt" => 0xffe9,
            "shift" => 0xffe1,
            "super" => 0xffeb,
            "space" => 0x20,
            "escape" => 0xff1b,
            other => u32::from_str_radix(other.trim_start_matches("0x"), 16).unwrap_or(0),
        }
    };
    let Ok((conn, _)) = x11rb::connect(None) else {
        eprintln!("no X server");
        std::process::exit(1);
    };
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let Ok(Ok(map)) = conn
        .get_keyboard_mapping(min, max - min + 1)
        .map(|c| c.reply())
    else {
        std::process::exit(1);
    };
    let per = usize::from(map.keysyms_per_keycode);
    let codes: Vec<u8> = std::env::args()
        .skip(1)
        .filter_map(|n| {
            let sym = keysym(&n);
            map.keysyms
                .chunks(per)
                .position(|s| s.contains(&sym))
                .and_then(|i| u8::try_from(i).ok())
                .map(|i| min + i)
        })
        .collect();
    for &c in &codes {
        let _ = conn.xtest_fake_input(2, c, 0, x11rb::NONE, 0, 0, 0);
    }
    for &c in codes.iter().rev() {
        let _ = conn.xtest_fake_input(3, c, 0, x11rb::NONE, 0, 0, 0);
    }
    let _ = conn.flush();
}

#[cfg(not(target_os = "linux"))]
fn main() {}

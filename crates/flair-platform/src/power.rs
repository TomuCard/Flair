//! Power source, for `pause_on_battery`: checked when something happens (a
//! file change, the window shown), never polled.

/// True when the computer runs on battery.
pub fn on_battery() -> bool {
    imp::on_battery()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    pub fn on_battery() -> bool {
        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: valid pointer to a status struct for the call.
        if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
            return false;
        }
        // 0 = offline (battery), 1 = online, 255 = unknown.
        status.ACLineStatus == 0
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    pub fn on_battery() -> bool {
        on_battery_in(Path::new("/sys/class/power_supply"))
    }

    /// On battery: a battery discharges and no mains supply is online.
    pub fn on_battery_in(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        let read = |p: &Path, f: &str| {
            std::fs::read_to_string(p.join(f))
                .map(|s| s.trim().to_owned())
                .unwrap_or_default()
        };
        let mut discharging = false;
        for entry in entries.flatten() {
            let p = entry.path();
            match read(&p, "type").as_str() {
                "Mains" | "USB" if read(&p, "online") == "1" => return false,
                "Battery" if read(&p, "status") == "Discharging" => discharging = true,
                _ => {}
            }
        }
        discharging
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_power_supplies() {
            let dir = std::env::temp_dir().join(format!("flair-power-{}", std::process::id()));
            let bat = dir.join("BAT0");
            let ac = dir.join("AC");
            std::fs::create_dir_all(&bat).expect("dir");
            std::fs::create_dir_all(&ac).expect("dir");
            std::fs::write(bat.join("type"), "Battery\n").expect("write");
            std::fs::write(bat.join("status"), "Discharging\n").expect("write");
            std::fs::write(ac.join("type"), "Mains\n").expect("write");
            std::fs::write(ac.join("online"), "0\n").expect("write");
            assert!(on_battery_in(&dir));
            std::fs::write(ac.join("online"), "1\n").expect("write");
            assert!(!on_battery_in(&dir));
            assert!(!on_battery_in(&dir.join("missing")));
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

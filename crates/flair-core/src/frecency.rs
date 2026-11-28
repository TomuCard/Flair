//! Frecency: what the user launches often and recently ranks higher.
//!
//! Each launch records the result id, a count and the date of the last use.
//! The bonus grows with the count (logarithmically) and fades with time.
//! Stored as `frecency.json` in the data folder (exported in `.flair` files):
//! `{"app:firefox.desktop": {"count": 12, "last_use": 1800000000}}`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Highest frecency bonus, added to the match score.
pub const MAX_BONUS: f32 = 3.0;
/// Entries kept on disk (the least valuable ones are dropped beyond).
const MAX_ENTRIES: usize = 1000;
const DAY: u64 = 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub count: u32,
    /// Unix time (seconds) of the last use.
    pub last_use: u64,
}

/// Weight of a use made `age` seconds ago.
pub fn recency_weight(age: u64) -> f32 {
    match age / DAY {
        0..4 => 1.0,
        4..14 => 0.7,
        14..31 => 0.5,
        31..90 => 0.3,
        _ => 0.1,
    }
}

/// Bonus of an entry at time `now` (Unix seconds), between 0 and [`MAX_BONUS`].
pub fn bonus(entry: Entry, now: u64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let uses = (1.0 + entry.count as f32).ln();
    (uses * recency_weight(now.saturating_sub(entry.last_use))).min(MAX_BONUS)
}

/// Launch history, loaded from and saved to a file.
#[derive(Debug, Default, Clone)]
pub struct Frecency {
    entries: HashMap<String, Entry>,
    file: Option<PathBuf>,
}

impl Frecency {
    /// In memory only (tests, read-only modes).
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Loads the history from `file`; a missing or damaged file gives an empty
    /// history (frecency is a convenience, never a reason to fail).
    pub fn load(file: &Path) -> Self {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        Self {
            entries: parse(&text),
            file: Some(file.to_owned()),
        }
    }

    /// Reads the file again (after an import replaced it).
    pub fn reload(&mut self) {
        if let Some(file) = &self.file {
            self.entries = parse(&std::fs::read_to_string(file).unwrap_or_default());
        }
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        self.entries.get(id).copied()
    }

    /// Bonus of a result id at time `now`.
    pub fn bonus(&self, id: &str, now: u64) -> f32 {
        self.get(id).map_or(0.0, |e| bonus(e, now))
    }

    /// Records a launch, then saves the history if it has a file.
    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.entries.entry(id.to_owned()).or_insert(Entry {
            count: 0,
            last_use: now,
        });
        entry.count = entry.count.saturating_add(1);
        entry.last_use = now;
        self.prune(now);
        if let Some(file) = &self.file
            && let Err(e) = crate::archive::write_atomic(file, serialize(&self.entries).as_bytes())
        {
            tracing::warn!(error = %e, file = %file.display(), "cannot save the launch history");
        }
    }

    /// Forgets everything (`> history clear`), file included.
    pub fn clear(&mut self) {
        self.entries.clear();
        if let Some(file) = &self.file
            && let Err(e) = crate::archive::write_atomic(file, b"{}")
        {
            tracing::warn!(error = %e, "cannot clear the launch history");
        }
    }

    fn prune(&mut self, now: u64) {
        if self.entries.len() <= MAX_ENTRIES {
            return;
        }
        let mut ranked: Vec<(String, f32)> = self
            .entries
            .iter()
            .map(|(id, e)| (id.clone(), bonus(*e, now)))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (id, _) in ranked.drain(MAX_ENTRIES..) {
            self.entries.remove(&id);
        }
    }
}

/// Reads the JSON history. Lenient: unknown or damaged entries are skipped,
/// and a bare number is read as a count with no date.
fn parse(text: &str) -> HashMap<String, Entry> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str(text) else {
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(id, value)| {
            let entry = match value {
                serde_json::Value::Number(n) => Entry {
                    count: u32::try_from(n.as_u64()?).ok()?,
                    last_use: 0,
                },
                serde_json::Value::Object(o) => Entry {
                    count: u32::try_from(o.get("count")?.as_u64()?).ok()?,
                    last_use: o
                        .get("last_use")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                },
                _ => return None,
            };
            (!id.is_empty()).then_some((id, entry))
        })
        .collect()
}

fn serialize(entries: &HashMap<String, Entry>) -> String {
    let map: serde_json::Map<String, serde_json::Value> = entries
        .iter()
        .map(|(id, e)| {
            (
                id.clone(),
                serde_json::json!({ "count": e.count, "last_use": e.last_use }),
            )
        })
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn more_uses_and_recent_uses_rank_higher() {
        let often = bonus(
            Entry {
                count: 20,
                last_use: NOW,
            },
            NOW,
        );
        let once = bonus(
            Entry {
                count: 1,
                last_use: NOW,
            },
            NOW,
        );
        let old = bonus(
            Entry {
                count: 20,
                last_use: NOW - 200 * DAY,
            },
            NOW,
        );
        assert!(often > once);
        assert!(often > old);
        assert!(often <= MAX_BONUS);
        assert!(once > 0.0);
    }

    #[test]
    fn weights_decrease_with_age() {
        let ages = [0, 5 * DAY, 20 * DAY, 60 * DAY, 400 * DAY];
        let weights: Vec<f32> = ages.iter().map(|a| recency_weight(*a)).collect();
        assert!(weights.windows(2).all(|w| w[0] > w[1]), "{weights:?}");
    }

    #[test]
    fn unknown_ids_have_no_bonus() {
        assert_eq!(Frecency::in_memory().bonus("app:x", NOW), 0.0);
    }

    #[test]
    fn record_save_and_reload() {
        let dir = std::env::temp_dir().join(format!("flair-frecency-{}", std::process::id()));
        let file = dir.join("frecency.json");
        let mut f = Frecency::load(&file);
        f.record("app:firefox.desktop", NOW);
        f.record("app:firefox.desktop", NOW + 10);
        f.record("app:lnk:Code.lnk", NOW);
        let back = Frecency::load(&file);
        assert_eq!(
            back.get("app:firefox.desktop"),
            Some(Entry {
                count: 2,
                last_use: NOW + 10
            })
        );
        assert_eq!(back.get("app:lnk:Code.lnk").map(|e| e.count), Some(1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn damaged_entries_are_skipped() {
        let entries =
            parse(r#"{"app:a": {"count": 3, "last_use": 100}, "app:b": "x", "app:c": 2, "": 1}"#);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries["app:a"].last_use, 100);
        assert_eq!(entries["app:c"].count, 2);
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn history_is_bounded() {
        let mut f = Frecency::in_memory();
        for i in 0..MAX_ENTRIES + 10 {
            f.record(&format!("app:{i}"), NOW);
        }
        assert_eq!(f.entries.len(), MAX_ENTRIES);
    }
}

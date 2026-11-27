//! History of the calculations the user copied, most recent first, shown by
//! `=` with nothing after it. Stored as `history.json` in the data folder (a
//! JSON array of inputs, exported in `.flair` files).

use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct History {
    items: Vec<String>,
    max: usize,
    file: Option<PathBuf>,
}

impl History {
    pub fn in_memory(max: usize) -> Self {
        Self {
            items: Vec::new(),
            max,
            file: None,
        }
    }

    /// Loads the file; missing or damaged means empty.
    pub fn load(file: &Path, max: usize) -> Self {
        let mut history = Self {
            items: Vec::new(),
            max,
            file: Some(file.to_owned()),
        };
        history.reload();
        history
    }

    /// Reads the file again (after an import replaced it).
    pub fn reload(&mut self) {
        if let Some(file) = &self.file {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            self.items = serde_json::from_str::<Vec<String>>(&text).unwrap_or_default();
            self.items.truncate(self.max);
        }
    }

    pub fn set_max(&mut self, max: usize) {
        self.max = max;
        self.items.truncate(max);
    }

    /// Most recent first.
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Adds an input at the top (moving it if already there), then saves.
    pub fn record(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() || self.max == 0 {
            return;
        }
        self.items.retain(|i| i != input);
        self.items.insert(0, input.to_owned());
        self.items.truncate(self.max);
        if let Some(file) = &self.file {
            let json = serde_json::to_string(&self.items).unwrap_or_else(|_| "[]".into());
            if let Err(e) = crate::archive::write_atomic(file, json.as_bytes()) {
                tracing::warn!(error = %e, "cannot save the calculation history");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_first_without_duplicates_and_bounded() {
        let mut h = History::in_memory(3);
        for input in ["1+1", "2*3", "1+1", "4/2", "5-1"] {
            h.record(input);
        }
        assert_eq!(h.items(), ["5-1", "4/2", "1+1"]);
        h.record("   ");
        assert_eq!(h.items().len(), 3);
    }

    #[test]
    fn saved_and_reloaded() {
        let dir = std::env::temp_dir().join(format!("flair-history-{}", std::process::id()));
        let file = dir.join("history.json");
        let mut h = History::load(&file, 10);
        h.record("15% de 240");
        h.record("5 km en miles");
        let back = History::load(&file, 10);
        assert_eq!(back.items(), ["5 km en miles", "15% de 240"]);
        std::fs::write(&file, "not json").expect("write");
        assert!(History::load(&file, 10).items().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

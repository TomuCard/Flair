//! Native file dialogs ("Save as", "Open"): Windows common dialogs, XDG
//! portal on Linux through libdbus (works on Wayland). Blocking: call from a
//! background thread, never from the UI thread.

use std::path::{Path, PathBuf};

/// A file type filter: label and extensions without the dot.
#[derive(Debug, Clone)]
pub struct Filter {
    pub name: String,
    pub extensions: Vec<String>,
}

/// "Save as" dialog. `None` when cancelled or unavailable.
pub fn save_file(
    title: &str,
    file_name: &str,
    directory: Option<&Path>,
    filter: &Filter,
) -> Option<PathBuf> {
    imp::save_file(title, file_name, directory, filter)
}

/// "Open" dialog. `None` when cancelled or unavailable.
pub fn open_file(title: &str, directory: Option<&Path>, filter: &Filter) -> Option<PathBuf> {
    imp::open_file(title, directory, filter)
}

mod imp {
    use std::path::{Path, PathBuf};

    use super::Filter;

    fn dialog(title: &str, directory: Option<&Path>, filter: &Filter) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .add_filter(&filter.name, &filter.extensions);
        if let Some(dir) = directory {
            dialog = dialog.set_directory(dir);
        }
        dialog
    }

    pub fn save_file(
        title: &str,
        file_name: &str,
        directory: Option<&Path>,
        filter: &Filter,
    ) -> Option<PathBuf> {
        dialog(title, directory, filter)
            .set_file_name(file_name)
            .save_file()
    }

    pub fn open_file(title: &str, directory: Option<&Path>, filter: &Filter) -> Option<PathBuf> {
        dialog(title, directory, filter).pick_file()
    }
}

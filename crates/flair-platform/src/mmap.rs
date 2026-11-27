//! Read-only memory-mapped files (the file index is opened without copying:
//! the system loads only the pages a search touches, and can drop them when
//! Flair sleeps).

use std::io;
use std::path::Path;

/// A whole file mapped read-only.
pub struct MappedFile {
    map: memmap2::Mmap,
}

impl MappedFile {
    /// Maps `path`. Flair never writes into a mapped file: index files are
    /// written once under a new name, then only read until deleted.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        // SAFETY: the mapping is read-only and the file is never modified in
        // place by Flair (new indexes get a new file name); another program
        // truncating Flair's private data file is out of the safety contract,
        // as for any memory-mapped reader.
        let map = unsafe { memmap2::Mmap::map(&file) }?;
        Ok(Self { map })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.map
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_file() {
        let dir = std::env::temp_dir().join(format!("flair-mmap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("a.bin");
        std::fs::write(&file, b"hello").expect("write");
        let map = MappedFile::open(&file).expect("map");
        assert_eq!(map.bytes(), b"hello");
        drop(map);
        let _ = std::fs::remove_dir_all(dir);
    }
}

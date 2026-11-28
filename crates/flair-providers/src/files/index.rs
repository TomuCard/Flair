//! The compact file index: every name in one byte buffer, every entry as
//! `(parent, name start, name length, flags)`, never a `String` or a
//! `PathBuf` per file. Saved with `rkyv` and searched straight from the
//! memory-mapped file (validated, without copying): the system loads only the
//! pages a search touches.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rkyv::{Archive, Serialize};

/// Version of the file layout; another version is scanned again.
pub const FORMAT: u32 = 1;
/// Parent of the roots, whose name is their full path.
pub const NO_PARENT: u32 = u32::MAX;
/// Entry flag: a folder.
pub const DIR: u16 = 1;

#[derive(Archive, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub parent: u32,
    pub name_start: u32,
    pub name_len: u16,
    pub flags: u16,
}

#[derive(Archive, Serialize, Debug, Default)]
pub struct IndexData {
    pub format: u32,
    /// Fingerprint of the settings the index was built with (roots,
    /// exclusions): another one means a new scan is needed.
    pub settings: String,
    pub entries: Vec<Entry>,
    pub names: Vec<u8>,
}

/// Builds an index; children are always added after their parent.
#[derive(Debug, Default)]
pub struct Builder {
    pub entries: Vec<Entry>,
    pub names: Vec<u8>,
}

impl Builder {
    /// Adds an entry; `None` when the name is too long or the index full.
    pub fn push(&mut self, parent: u32, name: &str, dir: bool) -> Option<u32> {
        let id = u32::try_from(self.entries.len())
            .ok()
            .filter(|i| *i != NO_PARENT)?;
        let name_start = u32::try_from(self.names.len()).ok()?;
        let name_len = u16::try_from(name.len()).ok()?;
        self.names.extend_from_slice(name.as_bytes());
        self.entries.push(Entry {
            parent,
            name_start,
            name_len,
            flags: if dir { DIR } else { 0 },
        });
        Some(id)
    }

    pub fn finish(self, settings: String) -> IndexData {
        IndexData {
            format: FORMAT,
            settings,
            entries: self.entries,
            names: self.names,
        }
    }
}

/// Serialized index bytes.
pub fn to_bytes(data: &IndexData) -> Option<rkyv::util::AlignedVec> {
    rkyv::to_bytes::<rkyv::rancor::Error>(data).ok()
}

/// Reads (and validates) serialized index bytes.
pub fn view(bytes: &[u8]) -> Option<&ArchivedIndexData> {
    rkyv::access::<ArchivedIndexData, rkyv::rancor::Error>(bytes)
        .ok()
        .filter(|v| v.format.to_native() == FORMAT)
}

impl ArchivedIndexData {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn name(&self, id: u32) -> &str {
        self.entries
            .get(id as usize)
            .map_or("", |e| name_of(e, self.names.as_slice()))
    }

    pub fn is_dir(&self, id: u32) -> bool {
        self.entries
            .get(id as usize)
            .is_some_and(|e| e.flags.to_native() & DIR != 0)
    }

    pub fn parent(&self, id: u32) -> u32 {
        self.entries
            .get(id as usize)
            .map_or(NO_PARENT, |e| e.parent.to_native())
    }

    /// Full path, rebuilt from the parents (only for shown results).
    pub fn path(&self, id: u32) -> PathBuf {
        let mut names = Vec::new();
        let mut current = id;
        // Bounded: a damaged file cannot loop forever.
        while current != NO_PARENT && names.len() < 4096 {
            names.push(self.name(current));
            current = self.parent(current);
        }
        let mut path = PathBuf::new();
        for name in names.iter().rev() {
            path.push(name);
        }
        path
    }

    /// Number of folders above an entry (0 for a root).
    pub fn depth(&self, id: u32) -> usize {
        let mut depth = 0;
        let mut current = self.parent(id);
        while current != NO_PARENT && depth < 4096 {
            depth += 1;
            current = self.parent(current);
        }
        depth
    }

    /// Ids of the entries whose full path is one of `paths`.
    pub fn find(&self, paths: &[&Path]) -> Vec<(u32, PathBuf)> {
        let names: std::collections::HashSet<&str> = paths
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        let mut out = Vec::new();
        let all = self.names.as_slice();
        for (id, e) in self.entries.iter().enumerate() {
            let name = name_of(e, all);
            if names.contains(name) || e.parent.to_native() == NO_PARENT {
                #[allow(clippy::cast_possible_truncation)]
                let id = id as u32;
                let path = self.path(id);
                if paths.iter().any(|p| *p == path) {
                    out.push((id, path));
                }
            }
        }
        out
    }
}

fn name_of<'a>(e: &ArchivedEntry, names: &'a [u8]) -> &'a str {
    let start = e.name_start.to_native() as usize;
    let end = start + usize::from(e.name_len.to_native());
    names
        .get(start..end)
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("")
}

// ---------------------------------------------------------------- search

/// A query split into lowercase, accent-free words; a name matches when it
/// contains them all.
#[derive(Debug, Clone)]
pub struct Needle {
    words: Vec<String>,
    whole: String,
}

impl Needle {
    pub fn new(query: &str) -> Self {
        let words: Vec<String> = query
            .split_whitespace()
            .map(flair_core::lexicon::fold)
            .filter(|w| !w.is_empty())
            .collect();
        Self {
            whole: words.join(" "),
            words,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Quick score of a matching name, `None` if it does not match. Used to
    /// keep the best candidates before the real ranking.
    pub fn quick(&self, name: &str, dir: bool) -> Option<u32> {
        self.quick_with(name, dir, &mut String::new())
    }

    /// [`Needle::quick`] with a reused buffer for accented names.
    pub fn quick_with(&self, name: &str, dir: bool, folded: &mut String) -> Option<u32> {
        let text: &str = if name.is_ascii() {
            if !self
                .words
                .iter()
                .all(|w| contains_ascii_nocase(name.as_bytes(), w.as_bytes()))
            {
                return None;
            }
            name
        } else {
            flair_core::lexicon::fold_into(name, folded);
            if !self.words.iter().all(|w| folded.contains(w.as_str())) {
                return None;
            }
            folded
        };
        let first = self.words.first()?;
        let mut score: u32 = 1000;
        if text.len() == self.whole.len() && text.eq_ignore_ascii_case(&self.whole) {
            score += 4000;
        } else if starts_with_nocase(text, first) {
            score += 2000;
        } else if word_start(text, first) {
            score += 1000;
        }
        // Without extension: `rapport` finds `rapport.pdf` as an exact name.
        // (An extension has a letter: `python3.14` is not `python3`.)
        if let Some((stem, ext)) = text.rsplit_once('.')
            && ext.len() <= 6
            && ext.chars().any(char::is_alphabetic)
            && stem.eq_ignore_ascii_case(&self.whole)
        {
            score += 3000;
        }
        if dir {
            score += 200;
        }
        #[allow(clippy::cast_possible_truncation)]
        Some(score.saturating_sub(text.len().min(500) as u32))
    }
}

fn contains_ascii_nocase(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    let first = needle[0];
    hay.windows(needle.len())
        .any(|w| w[0].eq_ignore_ascii_case(&first) && w.eq_ignore_ascii_case(needle))
}

fn starts_with_nocase(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// `prefix` starts a word of `text` (after a space, `-`, `_`, `.`).
fn word_start(text: &str, prefix: &str) -> bool {
    text.char_indices()
        .filter(|(i, _)| *i > 0)
        .filter(|(i, _)| {
            text[..*i]
                .chars()
                .next_back()
                .is_some_and(|c| matches!(c, ' ' | '-' | '_' | '.' | '(' | '['))
        })
        .any(|(i, _)| starts_with_nocase(&text[i..], prefix))
}

/// The best `limit` matches `(quick score, id)`, best first. Large indexes
/// are searched on several threads. `None` when cancelled.
pub fn search(
    view: &ArchivedIndexData,
    needle: &Needle,
    limit: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Vec<(u32, u32)>> {
    let entries = view.entries.as_slice();
    let names = view.names.as_slice();
    let threads = if entries.len() > 200_000 {
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .clamp(1, 4)
    } else {
        1
    };
    let stop = AtomicBool::new(false);
    let chunk = entries.len().div_ceil(threads).max(1);
    let scan = |offset: usize, part: &[ArchivedEntry]| -> Vec<(u32, u32)> {
        let mut best: std::collections::BinaryHeap<std::cmp::Reverse<(u32, u32)>> =
            std::collections::BinaryHeap::with_capacity(limit + 1);
        let mut folded = String::new();
        for (k, e) in part.iter().enumerate() {
            if k % 16_384 == 0 && (stop.load(Ordering::Relaxed) || cancelled()) {
                stop.store(true, Ordering::Relaxed);
                return Vec::new();
            }
            let dir = e.flags.to_native() & DIR != 0;
            let Some(score) = needle.quick_with(name_of(e, names), dir, &mut folded) else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation)]
            let id = (offset + k) as u32;
            if best.len() < limit {
                best.push(std::cmp::Reverse((score, id)));
            } else if best.peek().is_some_and(|w| w.0.0 < score) {
                best.pop();
                best.push(std::cmp::Reverse((score, id)));
            }
        }
        best.into_iter().map(|r| r.0).collect()
    };
    let mut all: Vec<(u32, u32)> = if threads == 1 {
        scan(0, entries)
    } else {
        std::thread::scope(|s| {
            let handles: Vec<_> = entries
                .chunks(chunk)
                .enumerate()
                .map(|(i, part)| {
                    let scan = &scan;
                    s.spawn(move || scan(i * chunk, part))
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        })
    };
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    all.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    all.truncate(limit);
    Some(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> rkyv::util::AlignedVec {
        let mut b = Builder::default();
        let root = b.push(NO_PARENT, "/home/me", true).expect("root");
        let docs = b.push(root, "Documents", true).expect("dir");
        b.push(docs, "rapport.pdf", false);
        b.push(docs, "Rapport annuel 2025.docx", false);
        let photos = b.push(root, "Photos", true).expect("dir");
        b.push(photos, "été à la mer.jpg", false);
        b.push(root, "notes.txt", false);
        to_bytes(&b.finish("s".into())).expect("bytes")
    }

    #[test]
    fn paths_and_depths() {
        let bytes = sample();
        let v = view(&bytes).expect("valid");
        assert_eq!(v.len(), 7);
        assert_eq!(v.path(2), Path::new("/home/me/Documents/rapport.pdf"));
        assert_eq!(v.depth(2), 2);
        assert!(v.is_dir(1));
        assert_eq!(v.settings.as_str(), "s");
        let found = v.find(&[Path::new("/home/me/Photos"), Path::new("/nope")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 4);
    }

    #[test]
    fn search_ranks_exact_and_prefix_names_first() {
        let bytes = sample();
        let v = view(&bytes).expect("valid");
        let hits = search(v, &Needle::new("rapport"), 10, &|| false).expect("done");
        let names: Vec<&str> = hits.iter().map(|(_, id)| v.name(*id)).collect();
        assert_eq!(names, ["rapport.pdf", "Rapport annuel 2025.docx"]);
        assert!(hits[0].0 >= 4000, "exact name without extension");
        let n = Needle::new("python3");
        assert!(n.quick("python3", false).expect("match") >= 4000);
        assert!(n.quick("python3.14", false).expect("match") < 4000);
        // Several words, any order, case and accents ignored.
        let hits = search(v, &Needle::new("2025 RAPPORT"), 10, &|| false).expect("done");
        assert_eq!(hits.len(), 1);
        let hits = search(v, &Needle::new("ete mer"), 10, &|| false).expect("done");
        assert_eq!(v.name(hits[0].1), "été à la mer.jpg");
        assert!(
            search(v, &Needle::new("zzz"), 10, &|| false)
                .expect("done")
                .is_empty()
        );
        assert!(search(v, &Needle::new("a"), 10, &|| true).is_none());
    }

    #[test]
    fn damaged_bytes_are_refused() {
        assert!(view(b"not an index at all, really not").is_none());
        let bytes = sample();
        assert!(view(&bytes[..bytes.len() / 2]).is_none());
    }

    #[test]
    fn word_starts() {
        assert!(word_start("Rapport annuel", "ann"));
        assert!(word_start("my-file", "file"));
        assert!(!word_start("myfile", "file"));
    }
}

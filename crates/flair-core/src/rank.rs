//! Result ranking: fuzzy matching with `nucleo-matcher` plus Flair's bonuses
//! (start of the title, start of a word, word initials, result kind,
//! frecency). Every scoring function here is pure and unit tested.

use nucleo_matcher::chars;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Score `nucleo-matcher` gives each matched character (its `SCORE_MATCH`),
/// used to bring its scores to a scale independent of the query length.
const NUCLEO_SCORE_PER_CHAR: f32 = 16.0;

/// The query is the beginning of the whole text (`fire` → **Fire**fox).
pub const BONUS_TEXT_PREFIX: f32 = 2.0;
/// The query is the beginning of a word (`code` → Visual Studio **Code**).
pub const BONUS_WORD_PREFIX: f32 = 1.5;
/// The query is the beginning of the word initials (`vsc` → **V**isual
/// **S**tudio **C**ode).
pub const BONUS_INITIALS: f32 = 1.5;
/// Below this match quality (and without any word bonus), a match is too
/// scattered to be meant (`calc` in `Lo**c**al Security Poli**c**y`), while
/// abbreviations stay (`frfx` → Firefox ≈ 1.17).
pub const MIN_QUALITY: f32 = 1.1;
/// Matching an alternative name (English name, keyword, executable) instead
/// of the displayed title costs a little.
pub const PENALTY_ALTERNATIVE: f32 = 0.5;

/// A query prepared once, then matched against many texts.
pub struct Prepared {
    pattern: Pattern,
    /// Lowercase, accent-free query without spaces, for the bonuses.
    folded: Vec<char>,
    /// Same, split into words.
    words: Vec<Vec<char>>,
}

impl Prepared {
    pub fn new(query: &str) -> Self {
        // Accents are removed from the query too: `écran` finds `Ecran`
        // (with `Smart`, nucleo would require the accent when typed).
        let unaccented: String = query.chars().map(chars::normalize).collect();
        let pattern = Pattern::new(
            &unaccented,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let words: Vec<Vec<char>> = query
            .split_whitespace()
            .map(|w| w.chars().map(fold).collect())
            .collect();
        Self {
            pattern,
            folded: words.concat(),
            words,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.folded.is_empty()
    }
}

/// How well a text matches a query.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub score: f32,
    /// Matched char indices in the text, sorted, for highlighting.
    pub indices: Vec<u32>,
}

/// Reusable matcher (holds scratch buffers: one per thread or query).
pub struct Ranker {
    matcher: Matcher,
    buf: Vec<char>,
    indices: Vec<u32>,
}

impl Default for Ranker {
    fn default() -> Self {
        Self::new()
    }
}

impl Ranker {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Matches one text. `None` when some query word is not found in it.
    pub fn match_text(&mut self, query: &Prepared, text: &str) -> Option<Match> {
        if query.is_empty() {
            return None;
        }
        self.indices.clear();
        let raw = query.pattern.indices(
            Utf32Str::new(text, &mut self.buf),
            &mut self.matcher,
            &mut self.indices,
        )?;
        let mut indices = std::mem::take(&mut self.indices);
        indices.sort_unstable();
        indices.dedup();
        #[allow(clippy::cast_precision_loss)]
        let quality = raw as f32 / (NUCLEO_SCORE_PER_CHAR * query.folded.len() as f32);
        let bonus = word_bonus(query, text);
        if bonus == 0.0 && quality < MIN_QUALITY {
            return None;
        }
        Some(Match {
            score: quality + bonus,
            indices,
        })
    }

    /// Best match among a title (whose indices are kept for highlighting) and
    /// alternative names (no highlighting, small penalty).
    pub fn match_names<'a>(
        &mut self,
        query: &Prepared,
        title: &str,
        alternatives: impl IntoIterator<Item = &'a str>,
    ) -> Option<Match> {
        let mut best = self.match_text(query, title);
        for alt in alternatives {
            if let Some(m) = self.match_text(query, alt) {
                let score = m.score - PENALTY_ALTERNATIVE;
                if best.as_ref().is_none_or(|b| score > b.score) {
                    best = Some(Match {
                        score,
                        indices: Vec::new(),
                    });
                }
            }
        }
        best
    }
}

/// Lowercase and accent-free version of a char (`É` → `e`).
fn fold(c: char) -> char {
    chars::to_lower_case(chars::normalize(c))
}

/// Words of a text: split on spaces and punctuation, and on lowercase →
/// uppercase changes (`OneNote` → `One`, `Note`).
fn words(text: &str) -> Vec<Vec<char>> {
    let mut out: Vec<Vec<char>> = Vec::new();
    let mut current = Vec::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        prev_lower = c.is_lowercase() || c.is_numeric();
        current.push(fold(c));
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Bonus for the query being the beginning of the text, of a word, or of
/// the word initials. The best applicable bonus is returned.
pub fn word_bonus(query: &Prepared, text: &str) -> f32 {
    let words = words(text);
    let joined: Vec<char> = words.concat();
    if joined.starts_with(&query.folded) {
        return BONUS_TEXT_PREFIX;
    }
    // Every query word begins some word of the text (`stu co` → Visual
    // **Stu**dio **Co**de).
    let all_words_prefix = query
        .words
        .iter()
        .all(|q| words.iter().any(|w| w.starts_with(q)));
    if all_words_prefix {
        return BONUS_WORD_PREFIX;
    }
    if query.folded.len() >= 2 {
        let initials: Vec<char> = words.iter().filter_map(|w| w.first().copied()).collect();
        if initials.starts_with(&query.folded) {
            return BONUS_INITIALS;
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(query: &str, text: &str) -> Option<f32> {
        Ranker::new()
            .match_text(&Prepared::new(query), text)
            .map(|m| m.score)
    }

    #[test]
    fn initials_find_visual_studio_code() {
        let vsc = score("vsc", "Visual Studio Code").unwrap_or_default();
        let other = score("vsc", "Microsoft Visual C++ Redistributable").unwrap_or_default();
        assert!(vsc > other, "{vsc} vs {other}");
        let m = Ranker::new().match_text(&Prepared::new("vsc"), "Visual Studio Code");
        assert_eq!(m.map(|m| m.indices), Some(vec![0, 7, 14]));
    }

    #[test]
    fn prefix_beats_middle_match() {
        let fire = score("fire", "Firefox").unwrap_or_default();
        let mid = score("fire", "Bonfire Editor").unwrap_or_default();
        assert!(fire > mid, "{fire} vs {mid}");
    }

    #[test]
    fn accents_and_case_are_ignored() {
        assert!(score("parametres", "Paramètres").is_some());
        assert!(score("PARAM", "Paramètres").is_some());
        assert!(score("écran", "Ecran de veille").is_some());
    }

    #[test]
    fn scattered_matches_are_dropped_but_abbreviations_kept() {
        assert!(score("calc", "Local Security Policy").is_none());
        assert!(score("vsc", "NordVPN Diagnostics").is_none());
        assert!(score("frfx", "Firefox").is_some());
        assert!(score("ntpd", "Notepad").is_some());
        assert!(score("chrm", "Google Chrome").is_some());
    }

    #[test]
    fn every_word_must_match() {
        assert!(score("visual code", "Visual Studio Code").is_some());
        assert!(score("visual zzz", "Visual Studio Code").is_none());
        assert!(score("xyz", "Firefox").is_none());
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(score("", "Firefox").is_none());
        assert!(score("   ", "Firefox").is_none());
    }

    #[test]
    fn word_bonuses() {
        let q = |s| Prepared::new(s);
        assert_eq!(word_bonus(&q("fire"), "Firefox"), BONUS_TEXT_PREFIX);
        assert_eq!(
            word_bonus(&q("code"), "Visual Studio Code"),
            BONUS_WORD_PREFIX
        );
        assert_eq!(
            word_bonus(&q("stu co"), "Visual Studio Code"),
            BONUS_WORD_PREFIX
        );
        assert_eq!(word_bonus(&q("vsc"), "Visual Studio Code"), BONUS_INITIALS);
        assert_eq!(word_bonus(&q("on"), "OneNote"), BONUS_TEXT_PREFIX);
        assert_eq!(word_bonus(&q("note"), "OneNote"), BONUS_WORD_PREFIX);
        assert_eq!(word_bonus(&q("ofx"), "Firefox"), 0.0);
        // A single letter is not an acronym.
        assert_eq!(word_bonus(&q("s"), "Visual Studio Code"), BONUS_WORD_PREFIX);
    }

    #[test]
    fn alternative_names_match_without_highlight() {
        let mut r = Ranker::new();
        let q = Prepared::new("files");
        let m = r.match_names(&q, "Fichiers", ["Files", "Nautilus"]);
        let m = m.expect("Files should match");
        assert!(m.indices.is_empty());
        // The title wins when it matches as well as an alternative.
        let q = Prepared::new("fich");
        let m = r.match_names(&q, "Fichiers", ["Files"]).expect("match");
        assert_eq!(m.indices, vec![0, 1, 2, 3]);
    }
}

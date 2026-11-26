//! Themes: exactly 3 colors (background, foreground, accent) from which every
//! UI color — the *tokens* — is derived, with guaranteed WCAG contrasts.
//!
//! Mixing happens in Oklab, lightness adjustments in OKLCH. A theme is never
//! rejected: if a minimum contrast is not met, the faulty color is moved away
//! from the background, step by step, keeping its hue, and the theme is
//! flagged as `adjusted`.

pub mod builtin;
mod color;

pub use color::{Color, ColorParseError};

use palette::Oklch;
use serde::Deserialize;

/// Minimum contrast ratios (WCAG 2) guaranteed for every theme.
pub mod min_contrast {
    /// Main text on background (target).
    pub const TEXT: f32 = 7.0;
    /// Main text on the selected row.
    pub const TEXT_ON_SELECTION: f32 = 4.5;
    pub const TEXT_SECONDARY: f32 = 4.5;
    pub const TEXT_MUTED: f32 = 3.0;
    /// Accent on background and on the selected row.
    pub const ACCENT: f32 = 4.5;
    pub const TEXT_ON_ACCENT: f32 = 4.5;
}

/// Mixing ratios from the spec (section 7).
mod mix {
    pub const TEXT_SECONDARY: f32 = 0.35;
    pub const TEXT_MUTED: f32 = 0.50;
    pub const HOVER: f32 = 0.04;
    pub const SELECTION: f32 = 0.10;
    pub const BORDER: f32 = 0.10;
}

/// Background lightness (Oklab L) above which a theme is "light".
const LIGHT_THRESHOLD: f32 = 0.6;

/// Lightness step used by the contrast adjustment.
const STEP: f32 = 0.005;

/// A theme as written by the user: 3 colors (the accent is optional).
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub background: Color,
    pub foreground: Color,
    /// Without an accent, matched letters only stand out by their weight.
    pub accent: Option<Color>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("invalid theme file: {0}")]
    Syntax(String),
    #[error(transparent)]
    Color(#[from] ColorParseError),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    name: String,
    background: String,
    foreground: String,
    accent: Option<String>,
}

impl Theme {
    /// Parses a theme file (`themes/lavande.toml`).
    pub fn from_toml(text: &str) -> Result<Self, ThemeError> {
        let file: ThemeFile =
            toml::from_str(text).map_err(|e| ThemeError::Syntax(e.message().to_owned()))?;
        Ok(Self {
            name: file.name,
            background: Color::from_hex(&file.background)?,
            foreground: Color::from_hex(&file.foreground)?,
            accent: file.accent.as_deref().map(Color::from_hex).transpose()?,
        })
    }

    /// Light or dark, from the background lightness.
    pub fn is_light(&self) -> bool {
        self.background.lightness() > LIGHT_THRESHOLD
    }

    /// Derives all UI tokens. Deterministic: same colors, same tokens.
    pub fn tokens(&self) -> Tokens {
        Tokens::derive(self.background, self.foreground, self.accent)
    }
}

/// Every color used by the interface. No widget may use any other color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub background: Color,
    pub text: Color,
    /// Subtitles, action hints.
    pub text_secondary: Color,
    /// Placeholder, `Ctrl` numbers, scrollbar.
    pub text_muted: Color,
    pub hover: Color,
    /// Background of the selected row.
    pub selection: Color,
    pub border: Color,
    pub accent: Color,
    pub text_on_accent: Color,
    pub is_light: bool,
    /// True when a color had to be changed to reach a minimum contrast.
    pub adjusted: bool,
}

impl Tokens {
    pub fn derive(background: Color, foreground: Color, accent: Option<Color>) -> Self {
        let mut adjusted = false;

        // 1. A mid-tone background cannot give 7:1 text with any color: move it
        //    towards its closest extreme first (only case where the background changes).
        let background = track(&mut adjusted, ensure_reachable(background.quantized()));
        let away = Direction::away_from(background);

        // 2. Main text.
        let text = track(
            &mut adjusted,
            ensure_contrast(
                foreground.quantized(),
                &[(background, min_contrast::TEXT)],
                away,
            ),
        );

        // 3. Secondary and muted texts, mixed from the text towards the background.
        let text_secondary = track(
            &mut adjusted,
            ensure_contrast(
                text.mix(background, mix::TEXT_SECONDARY).quantized(),
                &[(background, min_contrast::TEXT_SECONDARY)],
                away,
            ),
        );
        let text_muted = track(
            &mut adjusted,
            ensure_contrast(
                text.mix(background, mix::TEXT_MUTED).quantized(),
                &[(background, min_contrast::TEXT_MUTED)],
                away,
            ),
        );
        let hover = background.mix(text, mix::HOVER).quantized();
        let border = background.mix(text, mix::BORDER).quantized();

        // 4. Accent (defaults to the text color): readable on the background and
        //    on the selection, which itself depends on the accent.
        let wanted_accent = accent.unwrap_or(text).quantized();
        let mut target = min_contrast::ACCENT;
        let (mut accent, mut accent_changed);
        let mut selection;
        loop {
            (accent, accent_changed) =
                ensure_contrast(wanted_accent, &[(background, target)], away);
            selection = background.mix(accent, mix::SELECTION).quantized();
            if accent.contrast(selection) >= min_contrast::ACCENT || target >= 21.0 {
                break;
            }
            target += 0.25;
        }
        adjusted |= accent_changed;

        // 5. Main text must stay readable on the selected row.
        let text = track(
            &mut adjusted,
            ensure_contrast(
                text,
                &[
                    (background, min_contrast::TEXT),
                    (selection, min_contrast::TEXT_ON_SELECTION),
                ],
                away,
            ),
        );

        // 6. Text on an accent background: whichever of background/text contrasts most.
        let text_on_accent = if accent.contrast(background) >= accent.contrast(text) {
            background
        } else {
            text
        };

        Self {
            background,
            text,
            text_secondary,
            text_muted,
            hover,
            selection,
            border,
            accent,
            text_on_accent,
            is_light: background.lightness() > LIGHT_THRESHOLD,
            adjusted,
        }
    }
}

/// Records whether an adjustment happened and returns the color.
fn track(adjusted: &mut bool, (color, changed): (Color, bool)) -> Color {
    *adjusted |= changed;
    color
}

/// Which way to move a color's lightness to increase its contrast with the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Darker,
    Lighter,
}

impl Direction {
    fn away_from(background: Color) -> Self {
        if background.contrast(Color::BLACK) >= background.contrast(Color::WHITE) {
            Self::Darker
        } else {
            Self::Lighter
        }
    }
}

/// Moves `color` lightness (OKLCH L) step by step in `direction` until every
/// `(against, min)` pair is satisfied, or the lightness bound is reached.
/// Returns the color and whether it changed.
fn ensure_contrast(color: Color, pairs: &[(Color, f32)], direction: Direction) -> (Color, bool) {
    let ok = |c: Color| pairs.iter().all(|&(bg, min)| c.contrast(bg) >= min);
    if ok(color) {
        return (color, false);
    }
    let lch = color.to_oklch();
    let mut l = lch.l;
    loop {
        l = match direction {
            Direction::Darker => (l - STEP).max(0.0),
            Direction::Lighter => (l + STEP).min(1.0),
        };
        let candidate = Color::from_oklch_in_gamut(Oklch::new(l, lch.chroma, lch.hue)).quantized();
        if ok(candidate) || l <= 0.0 || l >= 1.0 {
            return (candidate, true);
        }
    }
}

/// Background contrast reachable with black or white needed so that every
/// token minimum can be met (7:1 text plus margin for the selection).
const MIN_REACHABLE: f32 = 8.0;

/// Moves a mid-tone background towards its closest extreme until `MIN_REACHABLE`
/// is possible.
fn ensure_reachable(background: Color) -> (Color, bool) {
    if background.max_contrast() >= MIN_REACHABLE {
        return (background, false);
    }
    // Going darker makes white text possible, and vice versa.
    let direction = match Direction::away_from(background) {
        Direction::Darker => Direction::Lighter,
        Direction::Lighter => Direction::Darker,
    };
    let lch = background.to_oklch();
    let mut l = lch.l;
    loop {
        l = match direction {
            Direction::Darker => (l - STEP).max(0.0),
            Direction::Lighter => (l + STEP).min(1.0),
        };
        let candidate = Color::from_oklch_in_gamut(Oklch::new(l, lch.chroma, lch.hue)).quantized();
        if candidate.max_contrast() >= MIN_REACHABLE || l <= 0.0 || l >= 1.0 {
            return (candidate, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn check_minimums(t: &Tokens) {
        let c = |a: Color, b: Color| a.contrast(b);
        assert!(c(t.text, t.background) >= min_contrast::TEXT, "text {t:?}");
        assert!(
            c(t.text, t.selection) >= min_contrast::TEXT_ON_SELECTION,
            "text/sel {t:?}"
        );
        assert!(
            c(t.text_secondary, t.background) >= min_contrast::TEXT_SECONDARY,
            "secondary {t:?}"
        );
        assert!(
            c(t.text_muted, t.background) >= min_contrast::TEXT_MUTED,
            "muted {t:?}"
        );
        assert!(
            c(t.accent, t.background) >= min_contrast::ACCENT,
            "accent {t:?}"
        );
        assert!(
            c(t.accent, t.selection) >= min_contrast::ACCENT,
            "accent/sel {t:?}"
        );
        assert!(
            c(t.text_on_accent, t.accent) >= min_contrast::TEXT_ON_ACCENT,
            "on accent {t:?}"
        );
    }

    #[test]
    fn builtin_themes_meet_every_minimum_without_adjustment() {
        for theme in builtin::all() {
            let t = theme.tokens();
            assert!(!t.adjusted, "{} was adjusted: {t:?}", theme.name);
            check_minimums(&t);
            assert_eq!(t.background, theme.background);
            assert_eq!(t.text, theme.foreground);
            assert_eq!(Some(t.accent), theme.accent);
        }
        assert!(builtin::light().is_light());
        assert!(!builtin::dark().is_light());
    }

    #[test]
    fn builtin_accents_are_the_flair_purple() {
        // Same hue as the application color #AD50D4, lightness adjusted.
        let hue = |c: Color| c.to_oklch().hue.into_positive_degrees();
        let brand = hue(Color::rgb8(0xAD, 0x50, 0xD4));
        for theme in builtin::all() {
            let accent = theme.accent.expect("accent");
            assert!(
                (hue(accent) - brand).abs() < 3.0,
                "{}: {}",
                theme.name,
                accent
            );
        }
    }

    #[test]
    fn builtin_contrasts_match_the_spec() {
        // Spec: text ≥ 15.2, accent ≥ 5.8 on background and ≥ 5 on selection,
        // secondary ≥ 5.2, muted ≥ 3.3. The spec values are rounded: the dark
        // theme text is 15.19:1 exactly, hence 15.15 below.
        for theme in builtin::all() {
            let t = theme.tokens();
            assert!(t.text.contrast(t.background) >= 15.15, "{}", theme.name);
            assert!(t.accent.contrast(t.background) >= 5.8, "{}", theme.name);
            assert!(t.accent.contrast(t.selection) >= 5.0, "{}", theme.name);
            assert!(
                t.text_secondary.contrast(t.background) >= 5.2,
                "{}",
                theme.name
            );
            assert!(t.text_muted.contrast(t.background) >= 3.3, "{}", theme.name);
        }
    }

    #[test]
    fn unreadable_theme_is_adjusted_and_readable() {
        let theme = Theme {
            name: "gris".into(),
            background: Color::from_hex("#777777").expect("ok"),
            foreground: Color::from_hex("#888888").expect("ok"),
            accent: Some(Color::from_hex("#808080").expect("ok")),
        };
        let t = theme.tokens();
        assert!(t.adjusted);
        check_minimums(&t);
    }

    #[test]
    fn two_color_theme_uses_text_as_accent() {
        let theme =
            Theme::from_toml("name = \"Duo\"\nbackground = \"#101010\"\nforeground = \"EEE\"\n")
                .expect("valid");
        let t = theme.tokens();
        assert_eq!(t.accent, t.text);
        check_minimums(&t);
    }

    #[test]
    fn theme_file_errors() {
        assert!(
            Theme::from_toml("name = \"x\"\nbackground = \"#zzz\"\nforeground = \"#fff\"").is_err()
        );
        assert!(Theme::from_toml("background = \"#000\"\nforeground = \"#fff\"").is_err());
        let lavande = Theme::from_toml(
            "name = \"Lavande\"\nbackground = \"#1E1B2E\"\nforeground = \"#E6E1F5\"\naccent = \"#B79CFF\"",
        )
        .expect("valid");
        assert!(!lavande.is_light());
    }

    proptest! {
        #[test]
        fn random_themes_always_meet_minimums(
            bg in any::<[u8; 3]>(), fg in any::<[u8; 3]>(), accent in any::<Option<[u8; 3]>>()
        ) {
            let rgb = |[r, g, b]: [u8; 3]| Color::rgb8(r, g, b);
            let t = Tokens::derive(rgb(bg), rgb(fg), accent.map(rgb));
            check_minimums(&t);
        }

        #[test]
        fn derivation_is_deterministic(bg in any::<[u8; 3]>(), fg in any::<[u8; 3]>(), accent in any::<[u8; 3]>()) {
            let rgb = |[r, g, b]: [u8; 3]| Color::rgb8(r, g, b);
            let a = Tokens::derive(rgb(bg), rgb(fg), Some(rgb(accent)));
            let b = Tokens::derive(rgb(bg), rgb(fg), Some(rgb(accent)));
            prop_assert_eq!(a, b);
        }
    }
}

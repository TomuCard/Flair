//! sRGB colors, Oklab/OKLCH conversions and WCAG 2 contrast.

use std::fmt;

use palette::convert::FromColorUnclamped;
use palette::{FromColor, LinSrgb, Oklab, Oklch, Srgb};

/// An opaque sRGB color (gamma-encoded channels in `0.0..=1.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid color `{0}`: expected #RGB or #RRGGBB")]
pub struct ColorParseError(pub String);

impl Color {
    pub const BLACK: Self = Self::rgb8(0, 0, 0);
    pub const WHITE: Self = Self::rgb8(255, 255, 255);

    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
        }
    }

    /// Parses `#RGB`, `#RRGGBB`, with or without `#`.
    pub fn from_hex(text: &str) -> Result<Self, ColorParseError> {
        let err = || ColorParseError(text.to_owned());
        let hex = text.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).map_err(|_| err());
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        match hex.len() {
            3 => Ok(Self::rgb8(digit(0)? * 17, digit(1)? * 17, digit(2)? * 17)),
            6 => Ok(Self::rgb8(pair(0)?, pair(2)?, pair(4)?)),
            _ => Err(err()),
        }
    }

    /// Channels rounded to 8 bits.
    pub fn to_rgb8(self) -> [u8; 3] {
        // Values are clamped to 0..=255 before the cast.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b)]
    }

    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_rgb8();
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    fn to_linear(self) -> LinSrgb {
        Srgb::new(self.r, self.g, self.b).into_linear()
    }

    fn from_linear(lin: LinSrgb) -> Self {
        let s = Srgb::from_linear(LinSrgb::new(
            lin.red.clamp(0.0, 1.0),
            lin.green.clamp(0.0, 1.0),
            lin.blue.clamp(0.0, 1.0),
        ));
        Self {
            r: s.red,
            g: s.green,
            b: s.blue,
        }
    }

    pub fn to_oklab(self) -> Oklab {
        Oklab::from_color(self.to_linear())
    }

    pub fn from_oklab(lab: Oklab) -> Self {
        Self::from_linear(LinSrgb::from_color_unclamped(lab))
    }

    pub fn to_oklch(self) -> Oklch {
        Oklch::from_color(self.to_oklab())
    }

    /// Converts an OKLCH color to sRGB, reducing its chroma (keeping lightness
    /// and hue) until it fits in the sRGB gamut.
    pub fn from_oklch_in_gamut(lch: Oklch) -> Self {
        // `FromColor` would silently clamp: the unclamped conversion is needed
        // to detect out-of-gamut colors.
        let in_gamut = |c: &Oklch| {
            let lin = LinSrgb::from_color_unclamped(*c);
            let ok = |v: f32| (-1e-4..=1.0 + 1e-4).contains(&v);
            ok(lin.red) && ok(lin.green) && ok(lin.blue)
        };
        if in_gamut(&lch) {
            return Self::from_linear(LinSrgb::from_color_unclamped(lch));
        }
        // Binary search on chroma: deterministic and fast enough (computed once per theme).
        let (mut lo, mut hi) = (0.0_f32, lch.chroma);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if in_gamut(&Oklch::new(lch.l, mid, lch.hue)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Self::from_linear(LinSrgb::from_color_unclamped(Oklch::new(
            lch.l, lo, lch.hue,
        )))
    }

    /// Perceptual lightness (Oklab L, `0.0..=1.0`).
    pub fn lightness(self) -> f32 {
        self.to_oklab().l
    }

    /// Mixes `self` towards `other` in Oklab: `amount = 0` gives `self`,
    /// `amount = 1` gives `other`.
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let a = self.to_oklab();
        let b = other.to_oklab();
        let t = amount.clamp(0.0, 1.0);
        Self::from_oklab(Oklab::new(
            a.l + (b.l - a.l) * t,
            a.a + (b.a - a.a) * t,
            a.b + (b.b - a.b) * t,
        ))
    }

    /// WCAG 2 relative luminance.
    pub fn relative_luminance(self) -> f32 {
        // Uses the 8-bit value, like contrast checkers do.
        let [r, g, b] = self.to_rgb8();
        let lin = Srgb::new(r, g, b).into_format::<f32>().into_linear();
        0.2126 * lin.red + 0.7152 * lin.green + 0.0722 * lin.blue
    }

    /// WCAG 2 contrast ratio, from 1.0 to 21.0.
    pub fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Best contrast reachable against this color (with pure black or white).
    pub fn max_contrast(self) -> f32 {
        self.contrast(Self::BLACK).max(self.contrast(Self::WHITE))
    }

    /// Quantizes to 8 bits per channel, so tokens are exactly what is displayed.
    pub fn quantized(self) -> Self {
        let [r, g, b] = self.to_rgb8();
        Self::rgb8(r, g, b)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(Color::from_hex("#FAFAF9").expect("ok").to_hex(), "#FAFAF9");
        assert_eq!(Color::from_hex("fafaf9").expect("ok").to_hex(), "#FAFAF9");
        assert_eq!(Color::from_hex("#fff").expect("ok").to_hex(), "#FFFFFF");
        assert_eq!(Color::from_hex("1E1B2E").expect("ok").to_hex(), "#1E1B2E");
        for bad in ["", "#12", "#12345", "#GGGGGG", "#1234567", "é12"] {
            assert!(Color::from_hex(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn wcag_contrast_reference_values() {
        let c = Color::BLACK.contrast(Color::WHITE);
        assert!((c - 21.0).abs() < 0.01);
        assert!((Color::WHITE.contrast(Color::WHITE) - 1.0).abs() < 1e-6);
        // #777777 on white is the classic ~4.48:1.
        let gray = Color::from_hex("#777777").expect("ok");
        assert!((gray.contrast(Color::WHITE) - 4.48).abs() < 0.01);
    }

    #[test]
    fn mixing_in_oklab() {
        let mid = Color::BLACK.mix(Color::WHITE, 0.5);
        assert!((mid.lightness() - 0.5).abs() < 0.01);
        assert_eq!(Color::BLACK.mix(Color::WHITE, 0.0).to_hex(), "#000000");
        assert_eq!(Color::BLACK.mix(Color::WHITE, 1.0).to_hex(), "#FFFFFF");
    }

    #[test]
    fn oklch_roundtrip() {
        let c = Color::from_hex("#B03A0A").expect("ok");
        assert_eq!(Color::from_oklch_in_gamut(c.to_oklch()).to_hex(), "#B03A0A");
    }
}

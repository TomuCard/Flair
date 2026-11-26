//! Conversion of theme tokens into Iced styles. This is the only place where
//! widget colors are chosen, always from the tokens.

use flair_core::theme::{self, Tokens};
use iced::widget::{container, scrollable, svg, text_input};
use iced::{Background, Border, Color, Shadow, Theme, border};

use crate::design;

/// Tokens converted to Iced colors, with the window opacity applied.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub hover: Color,
    pub selection: Color,
    pub border: Color,
    pub accent: Color,
}

fn color(c: theme::Color, alpha: f32) -> Color {
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

impl Palette {
    /// `opacity` applies to the background (setting `theme.opacity`).
    pub fn new(tokens: &Tokens, opacity: f32) -> Self {
        let bg_alpha = opacity.clamp(0.0, 1.0);
        Self {
            background: color(tokens.background, bg_alpha),
            text: color(tokens.text, 1.0),
            text_secondary: color(tokens.text_secondary, 1.0),
            text_muted: color(tokens.text_muted, 1.0),
            hover: color(tokens.hover, bg_alpha),
            selection: color(tokens.selection, bg_alpha),
            border: color(tokens.border, 1.0),
            accent: color(tokens.accent, 1.0),
        }
    }

    /// Iced theme built from the tokens, used as a fallback by any widget
    /// that is not explicitly styled (there should be none).
    pub fn iced_theme(tokens: &Tokens) -> Theme {
        let accent = color(tokens.accent, 1.0);
        Theme::custom(
            "Flair",
            iced::theme::Palette {
                background: color(tokens.background, 1.0),
                text: color(tokens.text, 1.0),
                primary: accent,
                success: accent,
                warning: accent,
                danger: accent,
            },
        )
    }

    /// Main window frame: background, 1 px border, rounded corners.
    ///
    /// No shadow drawn by Flair: Iced's software renderer only redraws the
    /// bounds of a quad, not its shadow, so a shadow outside the frame was
    /// painted again and again over itself until it turned black. The shadow
    /// comes from the OS where available (DWM on Windows).
    pub fn window(self) -> container::Style {
        container::Style {
            background: Some(Background::Color(self.background)),
            border: Border {
                color: self.border,
                width: design::BORDER_WIDTH,
                radius: design::WINDOW_RADIUS.into(),
            },
            shadow: Shadow::default(),
            text_color: Some(self.text),
            snap: true,
        }
    }

    pub fn separator(self) -> container::Style {
        container::Style {
            background: Some(Background::Color(self.border)),
            ..container::Style::default()
        }
    }

    /// Result row: selection, hover, or nothing.
    pub fn row(self, selected: bool, hovered: bool) -> container::Style {
        let background = if selected {
            Some(self.selection)
        } else if hovered {
            Some(self.hover)
        } else {
            None
        };
        container::Style {
            background: background.map(Background::Color),
            border: border::rounded(design::ROW_RADIUS),
            ..container::Style::default()
        }
    }

    pub fn input(self) -> text_input::Style {
        text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: self.text_muted,
            placeholder: self.text_muted,
            value: self.text,
            selection: self.selection,
        }
    }

    pub fn tint(color: Color) -> svg::Style {
        svg::Style { color: Some(color) }
    }

    /// Thin scrollbar, visible only while the list is hovered or dragged.
    pub fn scrollable(self, status: scrollable::Status) -> scrollable::Style {
        let visible = matches!(
            status,
            scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
        );
        let scroller = if visible {
            self.text_muted
        } else {
            Color::TRANSPARENT
        };
        let rail = scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Background::Color(scroller),
                border: border::rounded(design::SCROLLBAR_WIDTH / 2.0),
            },
        };
        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: Background::Color(self.background),
                border: Border::default(),
                shadow: Shadow::default(),
                icon: self.text_muted,
            },
        }
    }
}

//! Rendering of the launcher (mockups of spec section 7).

use flair_core::result::BuiltinIcon;
use flair_core::{IconSource, ProviderKind, ResultItem};
use iced::font::Weight;
use iced::widget::text::{Span, Wrapping};
use iced::widget::{
    Id, column, container, mouse_area, rich_text, row, scrollable, space, span, svg, text,
    text_input,
};
use iced::{Element, Fill, Font, Length, Padding, alignment, window};

use super::{Flair, INPUT_ID, LIST_ID, Message};
use crate::assets;
use crate::design;
use crate::style::Palette;

/// Interface font (embedded Inter) and monospaced font for calculations.
pub const INTER: Font = Font::with_name("Inter");
const INTER_MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..INTER
};
const INTER_SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..INTER
};
const MONO: Font = Font::with_name("JetBrains Mono");

/// Height of the row showing `item`.
pub fn row_height(item: &ResultItem) -> f32 {
    design::row_height(item.subtitle.is_some())
}

/// The main calculator result is the one carrying the expression as subtitle.
fn is_main_calc(item: &ResultItem) -> bool {
    item.provider == ProviderKind::Calculator && item.subtitle.is_some()
}

impl Flair {
    pub fn view(&self, _window: window::Id) -> Element<'_, Message> {
        let p = self.palette();

        // The magnifier of the mockups is the Flair logo: muted when empty,
        // accent as soon as something is typed.
        let logo_color = if self.input.is_empty() {
            p.text_muted
        } else {
            p.accent
        };
        let logo = svg(assets::logo())
            .width(design::LOGO_SIZE)
            .height(design::LOGO_SIZE)
            .style(move |_, _| Palette::tint(logo_color));

        let input = text_input(&self.i18n.t("search-placeholder"), &self.input)
            .id(Id::new(INPUT_ID))
            .on_input(Message::InputChanged)
            .size(design::INPUT_TEXT_SIZE)
            .font(INTER)
            .padding(0)
            .style(move |_, _| p.input());

        let field = container(
            row![logo, input]
                .spacing(design::INPUT_GAP)
                .align_y(alignment::Vertical::Center),
        )
        .padding(
            Padding::ZERO
                .left(design::INPUT_PADDING_X)
                .right(design::INPUT_PADDING_X),
        )
        .height(design::INPUT_HEIGHT)
        .center_y(design::INPUT_HEIGHT);

        let rows = self.rows();
        let mut content = column![field];
        if !rows.is_empty() {
            let separator = container(space())
                .width(Fill)
                .height(design::SEPARATOR_HEIGHT)
                .style(move |_| p.separator());
            let list = column(
                rows.iter()
                    .enumerate()
                    .map(|(i, item)| self.row(i, item, p)),
            )
            .padding(design::LIST_PADDING);
            let list = scrollable(list)
                .id(Id::new(LIST_ID))
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(design::SCROLLBAR_WIDTH)
                        .scroller_width(design::SCROLLBAR_WIDTH),
                ))
                .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
                .style(move |_, status| p.scrollable(status))
                .height(Fill);
            content = content.push(separator).push(list);
        }

        // Explicit frame height: the window itself may be taller (Wayland, see
        // `platform_window::can_resize`); the rest of it stays transparent.
        let frame = container(content)
            .width(Fill)
            .height(self.frame_height())
            .style(move |_| p.window());
        container(frame)
            .width(Fill)
            .height(Fill)
            .align_top(Fill)
            .into()
    }

    fn row<'a>(&'a self, index: usize, item: &'a ResultItem, p: Palette) -> Element<'a, Message> {
        let selected = index == self.selected;
        let hovered = self.hovered == Some(index);
        let main_calc = is_main_calc(item);

        let icon_color = if main_calc {
            p.accent
        } else {
            p.text_secondary
        };
        let icon = match &item.icon {
            IconSource::Builtin(icon) => assets::builtin(*icon),
            // Real icons (apps, files) arrive with milestone 4.
            _ => assets::builtin(BuiltinIcon::App),
        };
        let icon = svg(icon)
            .width(design::ICON_SIZE)
            .height(design::ICON_SIZE)
            .style(move |_, _| Palette::tint(icon_color));

        let mut texts = column![title(item, p)];
        if let Some(subtitle) = &item.subtitle {
            texts = texts.push(
                text(subtitle)
                    .size(design::SUBTITLE_SIZE)
                    .font(INTER)
                    .color(p.text_secondary)
                    .wrapping(Wrapping::None),
            );
        }

        let mut line = row![icon, texts.width(Fill)]
            .spacing(design::ROW_GAP)
            .align_y(alignment::Vertical::Center);

        // `↵ Ouvrir` on the selected row only.
        if selected && let Some(action) = item.primary_action() {
            line = line.push(
                text(format!("↵ {}", self.i18n.t(action.label_id())))
                    .size(design::HINT_SIZE)
                    .font(INTER)
                    .color(p.text_secondary),
            );
        }
        // `Ctrl+n` numbers only while Ctrl is held.
        if self.ctrl_held && index < 9 {
            line = line.push(
                text((index + 1).to_string())
                    .size(design::HINT_SIZE)
                    .font(INTER_MEDIUM)
                    .color(p.text_muted),
            );
        }

        let height = row_height(item);
        let row = container(line)
            .padding(
                Padding::ZERO
                    .left(design::ROW_PADDING)
                    .right(design::ROW_PADDING),
            )
            .width(Fill)
            .height(height)
            .center_y(Length::Fixed(height))
            .style(move |_| p.row(selected, hovered));

        mouse_area(row)
            .on_press(Message::Activate(index))
            .on_enter(Message::Hover(Some(index)))
            .on_exit(Message::Hover(None))
            .into()
    }
}

/// Title with the matched characters in accent color and semi-bold.
fn title(item: &ResultItem, p: Palette) -> Element<'_, Message> {
    if item.provider == ProviderKind::Calculator {
        let size = if is_main_calc(item) {
            design::CALC_RESULT_SIZE
        } else {
            design::TITLE_SIZE
        };
        return text(&item.title)
            .font(MONO)
            .size(size)
            .color(p.text)
            .wrapping(Wrapping::None)
            .into();
    }

    let mut spans: Vec<Span<'_, (), Font>> = Vec::new();
    let mut current = String::new();
    let mut current_matched = false;
    for (i, c) in item.title.chars().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let matched = item.match_indices.contains(&(i as u32));
        if matched != current_matched && !current.is_empty() {
            spans.push(styled_span(
                std::mem::take(&mut current),
                current_matched,
                p,
            ));
        }
        current_matched = matched;
        current.push(c);
    }
    if !current.is_empty() {
        spans.push(styled_span(current, current_matched, p));
    }
    rich_text(spans)
        .size(design::TITLE_SIZE)
        .wrapping(Wrapping::None)
        .into()
}

fn styled_span(content: String, matched: bool, p: Palette) -> Span<'static, (), Font> {
    if matched {
        span(content).color(p.accent).font(INTER_SEMIBOLD)
    } else {
        span(content).color(p.text).font(INTER_MEDIUM)
    }
}

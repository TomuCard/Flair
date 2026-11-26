//! Every dimension of the interface (spec section 7), on a 4 px grid.
//! These values are never repeated anywhere else in the code.

/// Window width.
pub const WINDOW_WIDTH: f32 = 680.0;
/// Corner radius of the window.
pub const WINDOW_RADIUS: f32 = 12.0;
/// Corner radius of the selected row.
pub const ROW_RADIUS: f32 = 8.0;
/// Window border width (`border` token).
pub const BORDER_WIDTH: f32 = 1.0;

/// Search field.
pub const INPUT_HEIGHT: f32 = 56.0;
pub const INPUT_TEXT_SIZE: f32 = 20.0;
pub const INPUT_PADDING_X: f32 = 16.0;
/// Flair logo shown in the search field.
pub const LOGO_SIZE: f32 = 20.0;
/// Gap between the logo and the text.
pub const INPUT_GAP: f32 = 12.0;

/// Separator between the field and the results (only with results).
pub const SEPARATOR_HEIGHT: f32 = 1.0;

/// Result rows.
pub const ROW_HEIGHT: f32 = 48.0;
pub const ROW_HEIGHT_SINGLE: f32 = 40.0;
/// Inner horizontal padding of a row and outer padding of the list.
pub const ROW_PADDING: f32 = 8.0;
pub const LIST_PADDING: f32 = 8.0;
/// Gap between the icon and the texts, and between right-side hints.
pub const ROW_GAP: f32 = 12.0;
pub const ICON_SIZE: f32 = 24.0;
pub const TITLE_SIZE: f32 = 14.0;
pub const SUBTITLE_SIZE: f32 = 12.0;
pub const HINT_SIZE: f32 = 12.0;
/// Main calculator result, in monospaced digits.
pub const CALC_RESULT_SIZE: f32 = 22.0;

/// Scrollbar width.
pub const SCROLLBAR_WIDTH: f32 = 4.0;

/// Visible rows before scrolling.
pub const MAX_VISIBLE_ROWS: usize = 8;

/// Vertical position of the window: fraction of the screen height.
pub const WINDOW_TOP_FRACTION: f32 = 0.25;

/// Height of a result row.
pub fn row_height(has_subtitle: bool) -> f32 {
    if has_subtitle {
        ROW_HEIGHT
    } else {
        ROW_HEIGHT_SINGLE
    }
}

/// Height of the visible part of the list (at most [`MAX_VISIBLE_ROWS`] rows).
pub fn list_viewport_height(rows: impl IntoIterator<Item = f32>) -> f32 {
    rows.into_iter().take(MAX_VISIBLE_ROWS).sum::<f32>() + 2.0 * LIST_PADDING
}

/// Window height for the given row heights: the field alone when empty.
pub fn window_height(rows: impl IntoIterator<Item = f32>) -> f32 {
    let mut rows = rows.into_iter().peekable();
    let base = INPUT_HEIGHT + 2.0 * BORDER_WIDTH;
    if rows.peek().is_none() {
        base
    } else {
        base + SEPARATOR_HEIGHT + list_viewport_height(rows)
    }
}

/// Tallest possible window: the field and [`MAX_VISIBLE_ROWS`] full rows.
pub fn max_window_height() -> f32 {
    window_height([ROW_HEIGHT; MAX_VISIBLE_ROWS])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_window_is_the_field_alone() {
        assert_eq!(window_height([]), INPUT_HEIGHT + 2.0 * BORDER_WIDTH);
    }

    #[test]
    fn grows_up_to_eight_rows() {
        let three = window_height([ROW_HEIGHT; 3]);
        assert_eq!(three, 58.0 + 1.0 + 16.0 + 3.0 * 48.0);
        assert_eq!(
            window_height([ROW_HEIGHT; 8]),
            window_height([ROW_HEIGHT; 20])
        );
    }

    #[test]
    fn dimensions_follow_the_4px_grid() {
        for v in [
            WINDOW_WIDTH,
            WINDOW_RADIUS,
            ROW_RADIUS,
            INPUT_HEIGHT,
            INPUT_PADDING_X,
            LOGO_SIZE,
            INPUT_GAP,
            ROW_HEIGHT,
            ROW_HEIGHT_SINGLE,
            ROW_PADDING,
            LIST_PADDING,
            ROW_GAP,
            ICON_SIZE,
            SCROLLBAR_WIDTH,
        ] {
            assert_eq!(v % 4.0, 0.0, "{v} is not on the 4 px grid");
        }
    }
}

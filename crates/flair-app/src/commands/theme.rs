//! `> thème` / `> theme`: the themes, previewed live while browsing (`Enter`
//! keeps one), and new themes from 3 colors: `> thème 1E1B2E E6E1F5 B79CFF
//! lavande` previews it at once; `Enter` saves `themes/lavande.toml` and
//! applies it, in the light or dark slot depending on its background.

use flair_core::Action;
use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::theme::{Color, Theme};

use super::{Context, Row, SET_SEPARATOR, item};

/// `#1E1B2E`, `1E1B2E`, `#abc`: a color typed in the command.
fn color(word: &str) -> Option<Color> {
    let hex = word.strip_prefix('#').unwrap_or(word);
    let looks = matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit());
    looks.then(|| Color::from_hex(word).ok()).flatten()
}

/// The action of a new theme (read back by [`decode_new`]).
fn new_action(theme: &Theme) -> Action {
    let accent = theme.accent.map(Color::to_hex).unwrap_or_default();
    Action::Internal {
        command: format!(
            "theme:new:{}{SET_SEPARATOR}{}{SET_SEPARATOR}{}{SET_SEPARATOR}{accent}",
            theme.name,
            theme.background.to_hex(),
            theme.foreground.to_hex()
        ),
    }
}

/// A theme carried by a `theme:new:` action.
pub fn decode_new(command: &str) -> Option<Theme> {
    let rest = command.strip_prefix("theme:new:")?;
    let parts: Vec<&str> = rest.split(SET_SEPARATOR).collect();
    let [name, background, foreground, accent] = parts.as_slice() else {
        return None;
    };
    Some(Theme {
        name: (*name).to_owned(),
        background: Color::from_hex(background).ok()?,
        foreground: Color::from_hex(foreground).ok()?,
        accent: Color::from_hex(accent).ok(),
    })
}

/// The theme previewed by a row's action (`theme:use:` or `theme:new:`).
pub fn previewed(command: &str) -> Option<Theme> {
    if let Some(name) = command.strip_prefix("theme:use:") {
        return crate::themes::find(name);
    }
    decode_new(command)
}

fn kind(theme: &Theme, cx: &Context<'_>) -> String {
    cx.t(if theme.is_light() {
        "theme-light"
    } else {
        "theme-dark"
    })
}

pub fn rows(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed("theme").unwrap_or_else(|| "> theme".into());
    let words: Vec<&str> = rest.split_whitespace().collect();
    let colors: Vec<Color> = words.iter().filter_map(|w| color(w)).collect();
    if colors.len() >= 2 {
        let name_words: Vec<&str> = words
            .iter()
            .copied()
            .filter(|w| color(w).is_none())
            .collect();
        let name = if name_words.is_empty() {
            crate::themes::free_name("perso")
        } else {
            name_words.join(" ")
        };
        let theme = Theme {
            name: name.clone(),
            background: colors[0],
            foreground: colors[1],
            accent: colors.get(2).copied(),
        };
        let tokens = theme.tokens();
        let mut subtitle = format!(
            "{} {} {}",
            theme.background.to_hex(),
            theme.foreground.to_hex(),
            theme.accent.map(Color::to_hex).unwrap_or_default()
        );
        if tokens.adjusted {
            subtitle.push_str(&format!(" · {}", cx.t("contrast-adjusted")));
        }
        subtitle.push_str(&format!(" · {}", cx.t("theme-save")));
        return vec![Row {
            item: item(
                format!("theme:new:{name}"),
                cx.ta("theme-new", &[("name", &name), ("kind", &kind(&theme, cx))]),
                Some(subtitle),
                BuiltinIcon::Settings,
                Some(new_action(&theme)),
            ),
            completion: Some(format!("{typed} {rest}")),
        }];
    }
    let themes = crate::themes::all();
    let prepared = Prepared::new(rest);
    let mut ranker = Ranker::new();
    themes
        .into_iter()
        .filter(|t| rest.trim().is_empty() || ranker.match_text(&prepared, &t.name).is_some())
        .map(|t| {
            let mut subtitle = kind(&t, cx);
            if t.name == cx.config.theme.light || t.name == cx.config.theme.dark {
                subtitle.push_str(&format!(" · {}", cx.t("theme-current")));
            }
            if t.tokens().adjusted {
                subtitle.push_str(&format!(" · {}", cx.t("contrast-adjusted")));
            }
            Row {
                item: item(
                    format!("theme:{}", t.name),
                    t.name.clone(),
                    Some(subtitle),
                    BuiltinIcon::Settings,
                    Some(Action::Internal {
                        command: format!("theme:use:{}", t.name),
                    }),
                ),
                completion: Some(format!("{typed} {}", t.name)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::rows as command_rows;
    use super::super::tests::with;
    use super::*;
    use flair_core::i18n::Lang;

    #[test]
    fn lists_and_creates_themes() {
        with(&["fr", "en"], Lang::Fr, |cx| {
            let list = command_rows("thème", cx);
            assert!(list.iter().any(|r| r.item.title == "Flair sombre"));
            let new = command_rows("thème 1E1B2E E6E1F5 B79CFF lavande", cx);
            assert_eq!(new.len(), 1);
            assert!(new[0].item.title.contains("lavande") && new[0].item.title.contains("sombre"));
            let Some(Action::Internal { command }) = new[0].item.actions.first() else {
                panic!("action");
            };
            let theme = decode_new(command).expect("theme");
            assert_eq!(theme.name, "lavande");
            assert!(!theme.is_light());
            // Unreadable colors are adjusted, never refused.
            let grey = command_rows("theme 777777 888888 808080", cx);
            assert!(
                grey[0]
                    .item
                    .subtitle
                    .as_deref()
                    .is_some_and(|s| s.contains("contraste ajusté"))
            );
            assert!(grey[0].item.title.contains("perso-"));
        });
    }
}

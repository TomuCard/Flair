//! `> help` / `> aide`: every command by category, generated from the
//! catalog, the language files and the settings registry (it cannot lag
//! behind the code). `> help fich` filters; `Enter` shows the details of a
//! command, `Tab` writes it.

use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::{Action, ResultItem};

use super::{CATALOG, Category, Command, Context, Row, info, item};

/// `> transparence 0.9`: the command with its example.
fn example(c: &Command, cx: &Context<'_>) -> String {
    let typed = cx.typed(c.id).unwrap_or_else(|| format!("> {}", c.id));
    let args = cx.t(&format!("cmd-{}-example", c.id));
    if args.trim().is_empty() {
        typed
    } else {
        format!("{typed} {args}")
    }
}

fn category(c: &Command, cx: &Context<'_>) -> String {
    cx.t(c.category.label_id())
}

fn help_row(c: &Command, cx: &Context<'_>) -> Row {
    Row {
        item: item(
            format!("help:{}", c.id),
            example(c, cx),
            Some(format!(
                "{} · {}",
                category(c, cx),
                cx.t(&format!("cmd-{}-desc", c.id))
            )),
            BuiltinIcon::Command,
            Some(Action::Internal {
                command: format!("help:{}", c.id),
            }),
        ),
        completion: cx.typed(c.id).map(|t| format!("{t} ")),
    }
}

/// Commands of the active languages, in help order.
fn available<'a>(cx: &'a Context<'_>) -> impl Iterator<Item = &'static Command> + 'a {
    Category::ALL.into_iter().flat_map(move |cat| {
        CATALOG
            .iter()
            .filter(move |c| c.category == cat && cx.available(c))
    })
}

/// `> help` and `> help <word>`.
pub fn rows(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    if rest.trim().is_empty() {
        return available(cx).map(|c| help_row(c, cx)).collect();
    }
    search(rest, cx)
}

/// Commands and settings matching a word (fuzzy, both languages).
pub fn search(text: &str, cx: &Context<'_>) -> Vec<Row> {
    let prepared = Prepared::new(text);
    let mut ranker = Ranker::new();
    let mut scored: Vec<(f32, Row)> = available(cx)
        .filter_map(|c| {
            let mut texts: Vec<String> = cx
                .words
                .all_forms(c.id)
                .iter()
                .map(|f| (*f).to_owned())
                .collect();
            texts.push(cx.t(&format!("cmd-{}", c.id)));
            texts.push(cx.t(&format!("cmd-{}-desc", c.id)));
            let score = texts
                .iter()
                .filter_map(|t| ranker.match_text(&prepared, t).map(|m| m.score))
                .fold(None, |acc: Option<f32>, s| {
                    Some(acc.map_or(s, |a| a.max(s)))
                })?;
            Some((score, help_row(c, cx)))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out: Vec<Row> = scored.into_iter().map(|(_, r)| r).collect();
    // Settings too: `> help transparence` also finds `theme.opacity`.
    let settings = super::settings::config(text, cx);
    out.extend(
        settings
            .into_iter()
            .filter(|r| r.item.id.starts_with("setting:"))
            .take(5),
    );
    out
}

/// Details of a command: syntax, example, description.
pub fn detail(id: &str, cx: &Context<'_>) -> Vec<ResultItem> {
    let Some(c) = super::find(id) else {
        return Vec::new();
    };
    let forms: Vec<String> = cx
        .words
        .all_forms(id)
        .iter()
        .map(|f| format!("> {f}"))
        .collect();
    let mut rows = vec![info(
        cx.t(&format!("cmd-{id}")),
        Some(cx.t(&format!("cmd-{id}-desc"))),
    )];
    rows.push(info(
        cx.ta("help-syntax", &[("forms", &forms.join(" | "))]),
        Some(category(c, cx)),
    ));
    let mut example_row = info(cx.ta("help-example", &[("example", &example(c, cx))]), None);
    example_row.actions = vec![super::fill(&example(c, cx))];
    rows.push(example_row);
    rows
}

#[cfg(test)]
mod tests {
    use super::super::rows as command_rows;
    use super::super::tests::with;
    use super::*;
    use flair_core::i18n::Lang;

    #[test]
    fn lists_every_command_by_category() {
        with(&["fr", "en"], Lang::Fr, |cx| {
            let all = rows("", cx);
            let available = CATALOG.iter().filter(|c| cx.available(c)).count();
            assert_eq!(all.len(), available);
            assert!(
                all[0]
                    .item
                    .subtitle
                    .as_deref()
                    .is_some_and(|s| s.starts_with("Réglages"))
            );
            assert!(all.iter().any(|r| r.item.title == "> transparence 0.9"));
        });
        with(&["fr", "en"], Lang::En, |cx| {
            assert!(rows("", cx).iter().any(|r| r.item.title == "> opacity 0.9"));
        });
    }

    #[test]
    fn filters_and_details() {
        with(&["fr", "en"], Lang::Fr, |cx| {
            let files = rows("fich", cx);
            assert!(
                files[0].item.id == "help:files" || files[0].item.id == "help:reindex",
                "{:?}",
                files[0].item.id
            );
            let opacity = rows("transparence", cx);
            assert!(opacity.iter().any(|r| r.item.id == "help:opacity"));
            assert!(opacity.iter().any(|r| r.item.id == "setting:theme.opacity"));
            let d = detail("lock", cx);
            assert_eq!(d[0].title, "Verrouiller la session");
            assert!(d[1].title.contains("> verrouiller") && d[1].title.contains("> lock"));
            // Unknown words fall back to the help search.
            assert!(!command_rows("transparenc", cx).is_empty());
        });
    }
}

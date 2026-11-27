//! `> config` and the shortcuts to settings (`> transparence 0.9`,
//! `> fichiers exclure .venv`, `> langue fr désactiver`…), all built on the
//! settings registry: same validation as the file, same descriptions.

use flair_core::rank::{Prepared, Ranker};
use flair_core::result::BuiltinIcon;
use flair_core::settings::Config;
use flair_core::settings::registry::{self, REGISTRY, SettingDescriptor, SettingKind};
use flair_core::{Action, lexicon::fold};

use super::{Command, Context, Row, SET_SEPARATOR, fill, info, item};

// ---------------------------------------------------------------- values

/// Current value of a setting.
pub fn current(config: &Config, key: &str) -> Option<toml::Value> {
    let all = toml::Value::try_from(config).ok()?;
    registry::lookup(&all, key).cloned()
}

/// The configuration with one value changed (live previews).
pub fn with_value(config: &Config, key: &str, value: &toml::Value) -> Option<Config> {
    let mut all = toml::Value::try_from(config).ok()?;
    let parts: Vec<&str> = key.split('.').collect();
    let (last, sections) = parts.split_last()?;
    let mut target = &mut all;
    for part in sections {
        target = target.get_mut(*part)?;
    }
    target
        .as_table_mut()?
        .insert((*last).to_owned(), value.clone());
    all.try_into().ok()
}

/// Settings previewed while their row is selected (they only change the look).
pub const PREVIEWED: &[&str] = &["theme.opacity", "theme.mode", "theme.light", "theme.dark"];

/// The action applying `value` to `key`.
pub fn set_action(key: &str, value: &toml::Value) -> Action {
    Action::Internal {
        command: format!("set:{key}{SET_SEPARATOR}{}", literal(value)),
    }
}

/// TOML text of a value (inline: arrays of tables as inline tables).
fn literal(value: &toml::Value) -> String {
    match value {
        toml::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(literal).collect();
            format!("[{}]", inner.join(", "))
        }
        toml::Value::Table(table) => {
            let inner: Vec<String> = table
                .iter()
                .map(|(k, v)| format!("{k} = {}", literal(v)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        other => other.to_string(),
    }
}

/// Reads a `set:` action back.
pub fn decode_set(command: &str) -> Option<(String, toml::Value)> {
    let rest = command.strip_prefix("set:")?;
    let (key, text) = rest.split_once(SET_SEPARATOR)?;
    let mut table: toml::Table = toml::from_str(&format!("v = {text}")).ok()?;
    Some((key.to_owned(), table.remove("v")?))
}

/// A value as shown (`activé`, `0.9`, `fr, en`).
pub fn display(value: &toml::Value, cx: &Context<'_>) -> String {
    match value {
        toml::Value::Boolean(true) => cx.t("value-on"),
        toml::Value::Boolean(false) => cx.t("value-off"),
        toml::Value::String(s) => s.clone(),
        toml::Value::Array(items) if items.is_empty() => cx.t("value-empty"),
        toml::Value::Array(items) => items
            .iter()
            .map(|v| match v {
                toml::Value::Table(t) => t
                    .get("keyword")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("?")
                    .to_owned(),
                other => display(other, cx),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

/// The text to write back for a value (`> config theme.opacity 0.9`).
fn raw(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn name(desc: &SettingDescriptor, cx: &Context<'_>) -> &'static str {
    desc.name.get(cx.i18n.lang())
}

/// `theme.opacity = 1 · défaut : 1 · 0.85 à 1`.
fn subtitle(desc: &SettingDescriptor, cx: &Context<'_>) -> String {
    let value = current(cx.config, desc.key);
    let mut parts = vec![format!(
        "{} = {}",
        desc.key,
        value.as_ref().map_or_else(String::new, |v| display(v, cx))
    )];
    if let Some(default) = desc.default_value()
        && Some(&default) != value.as_ref()
    {
        parts.push(cx.ta("setting-default", &[("value", &display(&default, cx))]));
    }
    match desc.kind {
        SettingKind::Choice(choices) => parts.push(choices.join(" | ")),
        SettingKind::Integer { min, max } => {
            parts.push(cx.ta(
                "setting-range",
                &[("min", &min.to_string()), ("max", &max.to_string())],
            ));
        }
        SettingKind::Float { min, max } => {
            parts.push(cx.ta(
                "setting-range",
                &[("min", &min.to_string()), ("max", &max.to_string())],
            ));
        }
        _ => {}
    }
    if desc.requires_restart {
        parts.push(cx.t("setting-restart"));
    }
    parts.join(" · ")
}

fn error_row(message: String) -> Row {
    Row {
        item: info(message, None),
        completion: None,
    }
}

// ---------------------------------------------------------------- finding settings

/// Folded names a setting is found by: key, leaf, names and aliases in both
/// languages.
fn names_of(desc: &SettingDescriptor) -> Vec<String> {
    let mut all = vec![desc.key.to_owned(), desc.leaf().to_owned()];
    all.push(desc.name.fr.to_owned());
    all.push(desc.name.en.to_owned());
    all.extend(desc.aliases_fr.iter().map(|a| (*a).to_owned()));
    all.extend(desc.aliases_en.iter().map(|a| (*a).to_owned()));
    all.iter().map(|n| fold(n)).collect()
}

/// The setting named at the start of `text` and what follows:
/// `transparence 0.9` → (`theme.opacity`, `0.9`).
pub fn find_setting(text: &str) -> Option<(&'static SettingDescriptor, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for n in (1..=words.len().min(4)).rev() {
        let typed = fold(&words[..n].join(" "));
        for desc in REGISTRY {
            if names_of(desc).contains(&typed) {
                return Some((desc, words[n..].join(" ")));
            }
        }
    }
    None
}

/// Settings matching a fuzzy text, best first.
fn search(
    text: &str,
    keep: impl Fn(&SettingDescriptor) -> bool,
) -> Vec<&'static SettingDescriptor> {
    let found: Vec<&SettingDescriptor> = REGISTRY.iter().filter(|d| keep(d)).collect();
    if text.trim().is_empty() {
        return found;
    }
    let prepared = Prepared::new(text);
    let mut ranker = Ranker::new();
    let mut scored: Vec<(f32, &SettingDescriptor)> = found
        .into_iter()
        .filter_map(|d| {
            let best = names_of(d)
                .iter()
                .filter_map(|n| ranker.match_text(&prepared, n).map(|m| m.score))
                .fold(None, |acc: Option<f32>, s| {
                    Some(acc.map_or(s, |a| a.max(s)))
                })?;
            Some((best, d))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().map(|(_, d)| d).collect()
}

/// `> config` row of a setting: `Enter` toggles a boolean, otherwise writes
/// the command to change it.
fn setting_row(desc: &SettingDescriptor, cx: &Context<'_>) -> Row {
    let config_cmd = cx.typed("config").unwrap_or_else(|| "> config".into());
    let prefix = format!("{config_cmd} {}", desc.key);
    let value = current(cx.config, desc.key);
    let action = match (desc.kind, &value) {
        (SettingKind::Bool, Some(toml::Value::Boolean(b))) => {
            set_action(desc.key, &toml::Value::Boolean(!b))
        }
        (SettingKind::WebEngines, _) => fill(&format!(
            "{} ",
            cx.typed("web").unwrap_or_else(|| "> web".into())
        )),
        (SettingKind::Choice(_) | SettingKind::List { .. }, _) | (_, None) => {
            fill(&format!("{prefix} "))
        }
        (_, Some(v)) => fill(&format!("{prefix} {}", raw(v))),
    };
    Row {
        item: item(
            format!("setting:{}", desc.key),
            name(desc, cx).to_owned(),
            Some(subtitle(desc, cx)),
            BuiltinIcon::Settings,
            Some(action),
        ),
        completion: Some(format!("{prefix} ")),
    }
}

fn setting_rows(found: Vec<&SettingDescriptor>, cx: &Context<'_>) -> Vec<Row> {
    found.into_iter().map(|d| setting_row(d, cx)).collect()
}

// ---------------------------------------------------------------- `> config`

/// `> config`, `> config transp`, `> config theme.opacity 0.9`,
/// `> config ouvrir`, `> config recharger`.
pub fn config(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let config_cmd = cx.typed("config").unwrap_or_else(|| "> config".into());
    if let Some((arg, _)) = cx.words.argument(rest) {
        let (command, title) = match arg {
            "open" => ("config:open", "cmd-config-open"),
            "reload" => ("config:reload", "cmd-config-reload"),
            _ => ("", ""),
        };
        if !command.is_empty() {
            return vec![Row {
                item: super::row(command, cx.t(title), None, BuiltinIcon::Settings),
                completion: Some(format!(
                    "{config_cmd} {}",
                    cx.words.argument_shown(arg).unwrap_or_default()
                )),
            }];
        }
    }
    if let Some((desc, value)) = find_setting(rest) {
        let prefix = format!("{config_cmd} {}", desc.key);
        let mut rows = value_rows(desc, &value, &prefix, cx);
        if value.is_empty() {
            rows.insert(0, setting_row(desc, cx));
        }
        return rows;
    }
    setting_rows(search(rest, |_| true), cx)
}

/// A shortcut command to one setting (`> transparence 0.9`).
pub fn setting_command(c: &Command, key: &str, rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let Some(desc) = registry::find(key) else {
        return Vec::new();
    };
    let prefix = cx.typed(c.id).unwrap_or_default();
    let mut rows = value_rows(desc, rest, &prefix, cx);
    if rest.trim().is_empty() {
        rows.insert(0, setting_row(desc, cx));
    }
    rows
}

/// Rows for a value being typed for `desc` (`prefix` is the command typed
/// before the value, for `Tab`).
fn value_rows(desc: &SettingDescriptor, text: &str, prefix: &str, cx: &Context<'_>) -> Vec<Row> {
    let text = text.trim();
    let value = current(cx.config, desc.key);
    if cx.words.is_argument(text, "default") {
        return match desc.default_value() {
            Some(default) => vec![set_row(desc, default, &format!("{prefix} {text}"), cx)],
            None => Vec::new(),
        };
    }
    match desc.kind {
        SettingKind::Bool if text.is_empty() => [true, false]
            .into_iter()
            .map(|b| {
                let word = cx
                    .words
                    .argument_shown(if b { "on" } else { "off" })
                    .unwrap_or_default();
                set_row(
                    desc,
                    toml::Value::Boolean(b),
                    &format!("{prefix} {word}"),
                    cx,
                )
            })
            .collect(),
        SettingKind::Choice(choices) => {
            let typed = fold(text);
            let matching: Vec<&&str> = choices
                .iter()
                .filter(|c| fold(c).starts_with(&typed))
                .collect();
            if matching.is_empty() {
                return vec![error_row(cx.i18n.value_error(
                    &registry::ValueError::NotAChoice {
                        allowed: choices.to_vec(),
                    },
                ))];
            }
            matching
                .into_iter()
                .map(|c| {
                    set_row(
                        desc,
                        toml::Value::String((*c).to_owned()),
                        &format!("{prefix} {c}"),
                        cx,
                    )
                })
                .collect()
        }
        SettingKind::List { .. } => list_rows(desc, text, prefix, cx),
        SettingKind::WebEngines => vec![Row {
            item: item(
                "fill:web".into(),
                name(desc, cx).to_owned(),
                value.as_ref().map(|v| display(v, cx)),
                BuiltinIcon::Web,
                Some(fill(&format!("{} ", cx.typed("web").unwrap_or_default()))),
            ),
            completion: cx.typed("web").map(|w| format!("{w} ")),
        }],
        _ if text.is_empty() => Vec::new(),
        _ => match parse_value(desc, text, cx) {
            Ok(v) => vec![set_row(desc, v, &format!("{prefix} {text}"), cx)],
            Err(message) => vec![error_row(message)],
        },
    }
}

/// A typed value checked with the registry rules.
fn parse_value(
    desc: &SettingDescriptor,
    text: &str,
    cx: &Context<'_>,
) -> Result<toml::Value, String> {
    let value = match desc.kind {
        SettingKind::Bool => {
            if cx.words.is_argument(text, "on") {
                toml::Value::Boolean(true)
            } else if cx.words.is_argument(text, "off") {
                toml::Value::Boolean(false)
            } else {
                return Err(cx.i18n.value_error(&registry::ValueError::NotABool));
            }
        }
        SettingKind::Integer { .. } => text
            .parse::<i64>()
            .map(toml::Value::Integer)
            .map_err(|_| cx.i18n.value_error(&registry::ValueError::NotAnInteger))?,
        SettingKind::Float { .. } => text
            .replace(',', ".")
            .parse::<f64>()
            .map(toml::Value::Float)
            .map_err(|_| cx.i18n.value_error(&registry::ValueError::NotANumber))?,
        _ => toml::Value::String(text.to_owned()),
    };
    desc.validate(&value).map_err(|e| cx.i18n.value_error(&e))?;
    Ok(value)
}

/// `Enter` applies `value`; the row shows the change.
fn set_row(
    desc: &SettingDescriptor,
    value: toml::Value,
    completion: &str,
    cx: &Context<'_>,
) -> Row {
    let old = current(cx.config, desc.key);
    let is_current = old.as_ref() == Some(&value);
    let title = format!("{} : {}", name(desc, cx), display(&value, cx));
    let mut subtitle = if is_current {
        cx.t("setting-current")
    } else {
        cx.ta(
            "setting-apply",
            &[(
                "old",
                &old.as_ref().map_or_else(String::new, |v| display(v, cx)),
            )],
        )
    };
    if desc.requires_restart {
        subtitle = format!("{subtitle} · {}", cx.t("setting-restart"));
    }
    Row {
        item: item(
            format!("set:{}:{}", desc.key, literal(&value)),
            title,
            Some(subtitle),
            BuiltinIcon::Settings,
            Some(set_action(desc.key, &value)),
        ),
        completion: Some(completion.trim_end().to_owned()),
    }
}

/// Lists: `ajouter X`, `retirer X`; alone, the items (`Enter` removes one).
fn list_rows(desc: &SettingDescriptor, text: &str, prefix: &str, cx: &Context<'_>) -> Vec<Row> {
    let items: Vec<String> = current(cx.config, desc.key)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let as_value = |list: &[String]| {
        toml::Value::Array(
            list.iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        )
    };
    let check = |list: Vec<String>, completion: String| -> Row {
        let value = as_value(&list);
        match desc.validate(&value) {
            Ok(()) => set_row(desc, value, &completion, cx),
            Err(e) => error_row(cx.i18n.value_error(&e)),
        }
    };
    let add_word = cx.words.argument_shown("add").unwrap_or("add");
    let remove_word = cx.words.argument_shown("remove").unwrap_or("remove");
    match cx.words.argument(text) {
        Some(("add", x)) if !x.is_empty() => {
            if items.contains(&x) {
                return vec![error_row(cx.ta("setting-list-has", &[("value", &x)]))];
            }
            let mut list = items.clone();
            list.push(x.clone());
            vec![check(list, format!("{prefix} {add_word} {x}"))]
        }
        Some(("remove", x)) if !x.is_empty() => {
            if !items.contains(&x) {
                return vec![error_row(cx.ta("setting-list-missing", &[("value", &x)]))];
            }
            let list: Vec<String> = items.iter().filter(|i| **i != x).cloned().collect();
            vec![check(list, format!("{prefix} {remove_word} {x}"))]
        }
        _ => {
            let mut rows: Vec<Row> = items
                .iter()
                .map(|i| {
                    let list: Vec<String> = items.iter().filter(|x| *x != i).cloned().collect();
                    let mut r = check(list, format!("{prefix} {remove_word} {i}"));
                    r.item.title = i.clone();
                    r.item.subtitle = Some(cx.t("setting-list-remove"));
                    r
                })
                .collect();
            rows.push(Row {
                item: item(
                    format!("fill:{}:add", desc.key),
                    cx.t("setting-list-add"),
                    Some(format!("{prefix} {add_word} …")),
                    BuiltinIcon::Settings,
                    Some(fill(&format!("{prefix} {add_word} "))),
                ),
                completion: Some(format!("{prefix} {add_word} ")),
            });
            rows
        }
    }
}

// ---------------------------------------------------------------- shortcuts

/// Rows of one setting given a value text (`on`, `rad`, `15`).
fn shortcut(key: &str, value: &str, prefix: &str, cx: &Context<'_>) -> Vec<Row> {
    let Some(desc) = registry::find(key) else {
        return Vec::new();
    };
    let mut rows = value_rows(desc, value, prefix, cx);
    if value.trim().is_empty() {
        rows.insert(0, setting_row(desc, cx));
    }
    rows
}

fn under(section: &str) -> impl Fn(&SettingDescriptor) -> bool + '_ {
    move |d| d.section() == Some(section)
}

/// `> fichiers`, `> fichiers ajouter D:\Projets`, `> fichiers exclure .venv`,
/// `> fichiers cachés on`, `> fichiers retirer D:\Projets`.
pub fn files(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed("files").unwrap_or_default();
    let config_cmd = cx.typed("config").unwrap_or_default();
    match cx.words.argument(rest) {
        Some(("add", x)) => {
            let add = cx.words.argument_shown("add").unwrap_or_default();
            list_rows_for(
                "files.roots",
                &format!("{add} {x}"),
                &format!("{config_cmd} files.roots"),
                cx,
            )
        }
        Some(("exclude", x)) => {
            let add = cx.words.argument_shown("add").unwrap_or_default();
            list_rows_for(
                "files.exclude",
                &format!("{add} {x}"),
                &format!("{config_cmd} files.exclude"),
                cx,
            )
        }
        Some(("hidden", x)) => shortcut(
            "files.include_hidden",
            &x,
            &format!(
                "{typed} {}",
                cx.words.argument_shown("hidden").unwrap_or_default()
            ),
            cx,
        ),
        Some(("remove", x)) => {
            let roots = current(cx.config, "files.roots")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            let key = if roots.iter().any(|r| r.as_str() == Some(x.as_str())) {
                "files.roots"
            } else {
                "files.exclude"
            };
            let remove = cx.words.argument_shown("remove").unwrap_or_default();
            list_rows_for(
                key,
                &format!("{remove} {x}"),
                &format!("{config_cmd} {key}"),
                cx,
            )
        }
        Some(("on" | "off", _)) => shortcut("files.enabled", rest, &typed, cx),
        _ => setting_rows(search(rest, under("files")), cx),
    }
}

fn list_rows_for(key: &str, text: &str, prefix: &str, cx: &Context<'_>) -> Vec<Row> {
    registry::find(key).map_or_else(Vec::new, |d| list_rows(d, text, prefix, cx))
}

/// `> calc angle rad`, `> calc chiffres 15`, `> calc devises on`,
/// `> calc octets binaire`.
pub fn calc(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed("calc").unwrap_or_default();
    let arg_word = |id: &str| cx.words.argument_shown(id).unwrap_or_default().to_owned();
    match cx.words.argument(rest) {
        Some(("angle", x)) => shortcut(
            "calc.angle_unit",
            &x,
            &format!("{typed} {}", arg_word("angle")),
            cx,
        ),
        Some(("digits", x)) => shortcut(
            "calc.significant_digits",
            &x,
            &format!("{typed} {}", arg_word("digits")),
            cx,
        ),
        Some(("currency", x)) => shortcut(
            "calc.currency",
            &x,
            &format!("{typed} {}", arg_word("currency")),
            cx,
        ),
        Some(("bytes", x)) => {
            let prefix = format!("{typed} {}", arg_word("bytes"));
            let Some(desc) = registry::find("calc.binary_prefixes") else {
                return Vec::new();
            };
            let pick = |binary: bool| {
                let word = arg_word(if binary { "binary" } else { "decimal" });
                set_row(
                    desc,
                    toml::Value::Boolean(binary),
                    &format!("{prefix} {word}"),
                    cx,
                )
            };
            match cx.words.argument(&x) {
                Some(("binary", _)) => vec![pick(true)],
                Some(("decimal", _)) => vec![pick(false)],
                _ => vec![pick(true), pick(false)],
            }
        }
        _ => setting_rows(search(rest, under("calc")), cx),
    }
}

/// `> batterie pause on`, `> batterie off`.
pub fn battery(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed("battery").unwrap_or_default();
    let value = match cx.words.argument(rest) {
        Some(("pause", x)) => x,
        _ => rest.to_owned(),
    };
    shortcut("resources.pause_on_battery", &value, &typed, cx)
}

// ---------------------------------------------------------------- `> web`

fn engines(config: &Config) -> Vec<(String, String, String)> {
    config
        .web
        .iter()
        .map(|e| (e.keyword.clone(), e.name.clone(), e.url.clone()))
        .collect()
}

fn engines_value(list: &[(String, String, String)]) -> toml::Value {
    toml::Value::Array(
        list.iter()
            .map(|(k, n, u)| {
                let mut t = toml::Table::new();
                t.insert("keyword".into(), toml::Value::String(k.clone()));
                t.insert("name".into(), toml::Value::String(n.clone()));
                t.insert("url".into(), toml::Value::String(u.clone()));
                toml::Value::Table(t)
            })
            .collect(),
    )
}

/// `> web` lists the engines; `> web ajouter yt YouTube https://…{query}`;
/// `> web retirer yt`.
pub fn web(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed("web").unwrap_or_default();
    let Some(desc) = registry::find("web") else {
        return Vec::new();
    };
    let list = engines(cx.config);
    match cx.words.argument(rest) {
        Some(("add", x)) => {
            let words: Vec<&str> = x.split_whitespace().collect();
            let add = cx.words.argument_shown("add").unwrap_or_default();
            if words.len() < 3 {
                return vec![error_row(cx.t("web-add-usage"))];
            }
            let keyword = words[0].to_owned();
            let url = words[words.len() - 1].to_owned();
            let name = words[1..words.len() - 1].join(" ");
            let mut new: Vec<_> = list.into_iter().filter(|(k, _, _)| *k != keyword).collect();
            new.push((keyword, name, url));
            let value = engines_value(&new);
            match desc.validate(&value) {
                Ok(()) => vec![set_row(desc, value, &format!("{typed} {add} {x}"), cx)],
                Err(e) => vec![error_row(cx.i18n.value_error(&e))],
            }
        }
        Some(("remove", x)) => {
            let remove = cx.words.argument_shown("remove").unwrap_or_default();
            if !list.iter().any(|(k, _, _)| *k == x) {
                return vec![error_row(cx.ta("setting-list-missing", &[("value", &x)]))];
            }
            let new: Vec<_> = list.into_iter().filter(|(k, _, _)| *k != x).collect();
            vec![set_row(
                desc,
                engines_value(&new),
                &format!("{typed} {remove} {x}"),
                cx,
            )]
        }
        _ => {
            let engines: Vec<flair_providers::web::Engine> = list
                .iter()
                .map(|(k, n, u)| flair_providers::web::Engine {
                    keyword: k.clone(),
                    name: n.clone(),
                    url: u.clone(),
                })
                .collect();
            let mut rows: Vec<Row> = flair_providers::web::matching(&engines, rest)
                .into_iter()
                .map(|e| Row {
                    item: item(
                        format!("web:{}", e.keyword),
                        format!("{} ({})", e.name, e.keyword),
                        Some(e.url.clone()),
                        BuiltinIcon::Web,
                        // `Enter`: search with it.
                        Some(fill(&format!("{} ", e.keyword))),
                    ),
                    completion: Some(format!("{} ", e.keyword)),
                })
                .collect();
            let add = cx.words.argument_shown("add").unwrap_or_default();
            rows.push(Row {
                item: item(
                    "fill:web:add".into(),
                    cx.t("web-add"),
                    Some(cx.t("web-add-usage")),
                    BuiltinIcon::Web,
                    Some(fill(&format!("{typed} {add} "))),
                ),
                completion: Some(format!("{typed} {add} ")),
            });
            rows
        }
    }
}

// ---------------------------------------------------------------- `> langue`

/// `> langue` lists the input languages (`Enter` enables or disables one);
/// `> langue fr désactiver`, `> langue en préférer`, `> langue interface en`.
pub fn language(rest: &str, cx: &Context<'_>) -> Vec<Row> {
    let typed = cx.typed("language").unwrap_or_else(|| "> language".into());
    let active = &cx.config.input_languages;
    let Some(desc) = registry::find("input_languages") else {
        return Vec::new();
    };
    let list_value = |list: &[String]| {
        toml::Value::Array(
            list.iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        )
    };
    if let Some(("interface", x)) = cx.words.argument(rest) {
        return shortcut(
            "ui_language",
            &x,
            &format!(
                "{typed} {}",
                cx.words.argument_shown("interface").unwrap_or_default()
            ),
            cx,
        );
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    let chosen = words.first().and_then(|w| {
        let w = fold(w);
        cx.languages
            .iter()
            .find(|l| l.code == w || fold(&l.name).starts_with(&w))
    });
    let toggle_row = |code: &str, enable: bool, prefer: bool| -> Row {
        let mut list: Vec<String> = active.iter().filter(|c| *c != code).cloned().collect();
        if prefer {
            list.insert(0, code.to_owned());
        } else if enable {
            list.push(code.to_owned());
        }
        let arg = if prefer {
            "prefer"
        } else if enable {
            "on"
        } else {
            "off"
        };
        let completion = format!(
            "{typed} {code} {}",
            cx.words.argument_shown(arg).unwrap_or_default()
        );
        if list.is_empty() {
            return error_row(cx.t("language-last"));
        }
        let mut r = set_row(desc, list_value(&list), &completion, cx);
        let name = cx
            .languages
            .iter()
            .find(|l| l.code == code)
            .map_or(code, |l| l.name.as_str());
        r.item.title = cx.ta(
            if prefer {
                "language-prefer"
            } else if enable {
                "language-enable"
            } else {
                "language-disable"
            },
            &[("language", name)],
        );
        r
    };
    if let Some(lang) = chosen {
        let rest = words[1..].join(" ");
        let is_active = active.contains(&lang.code);
        return match cx.words.argument(&rest).map(|(a, _)| a) {
            Some("on") if is_active => vec![error_row(
                cx.ta("language-already", &[("language", &lang.name)]),
            )],
            Some("on") => vec![toggle_row(&lang.code, true, false)],
            Some("off") if !is_active => vec![error_row(
                cx.ta("language-already-off", &[("language", &lang.name)]),
            )],
            Some("off") => vec![toggle_row(&lang.code, false, false)],
            Some("prefer") => vec![toggle_row(&lang.code, true, true)],
            _ => {
                let mut rows = vec![toggle_row(&lang.code, !is_active, false)];
                if is_active && active.first() != Some(&lang.code) {
                    rows.push(toggle_row(&lang.code, true, true));
                }
                rows
            }
        };
    }
    // The list: active languages in order of preference, then the others.
    let mut ordered: Vec<&super::Language> = cx.languages.iter().collect();
    ordered.sort_by_key(|l| {
        (
            active
                .iter()
                .position(|c| *c == l.code)
                .unwrap_or(usize::MAX),
            l.code.clone(),
        )
    });
    let mut rows: Vec<Row> = ordered
        .into_iter()
        .map(|l| {
            let position = active.iter().position(|c| *c == l.code);
            let mut r = toggle_row(&l.code, position.is_none(), false);
            r.item.title = format!("{} ({})", l.name, l.code);
            r.item.subtitle = Some(match position {
                Some(_) if active.len() == 1 => cx.t("language-last"),
                Some(p) => cx.ta("language-active", &[("position", &(p + 1).to_string())]),
                None => cx.t("language-inactive"),
            });
            if position.is_some() && active.len() == 1 {
                r.item.actions.clear();
            }
            r
        })
        .collect();
    if let Some(ui) = registry::find("ui_language") {
        let mut r = setting_row(ui, cx);
        r.item.actions = vec![fill(&format!(
            "{typed} {} ",
            cx.words.argument_shown("interface").unwrap_or_default()
        ))];
        rows.push(r);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::super::rows;
    use super::super::tests::with;
    use super::*;
    use flair_core::i18n::Lang;

    fn sets(text: &str) -> Vec<(String, toml::Value)> {
        with(&["fr", "en"], Lang::Fr, |cx| {
            rows(text, cx)
                .into_iter()
                .filter_map(|r| match r.item.actions.first() {
                    Some(Action::Internal { command }) => decode_set(command),
                    _ => None,
                })
                .collect()
        })
    }

    fn titles(text: &str) -> Vec<String> {
        with(&["fr", "en"], Lang::Fr, |cx| {
            rows(text, cx).into_iter().map(|r| r.item.title).collect()
        })
    }

    #[test]
    fn set_actions_round_trip() {
        for value in [
            toml::Value::Float(0.9),
            toml::Value::String("a \"b\"".into()),
            engines_value(&[("g".into(), "Google".into(), "https://g/?q={query}".into())]),
        ] {
            let Action::Internal { command } = set_action("k.x", &value) else {
                panic!("internal");
            };
            assert_eq!(decode_set(&command), Some(("k.x".into(), value)));
        }
    }

    #[test]
    fn config_finds_settings_by_alias_in_both_languages() {
        assert_eq!(
            sets("config transparence 0.9"),
            [("theme.opacity".into(), toml::Value::Float(0.9))]
        );
        assert_eq!(
            sets("config opacity 0,9"),
            [("theme.opacity".into(), toml::Value::Float(0.9))]
        );
        assert_eq!(
            sets("réglages max_results 10"),
            [("max_results".into(), toml::Value::Integer(10))]
        );
        assert_eq!(
            sets("config calc.angle_unit rad"),
            [("calc.angle_unit".into(), "rad".into())]
        );
        assert_eq!(
            sets("config autostart oui"),
            [("autostart".into(), toml::Value::Boolean(true))]
        );
        assert_eq!(
            sets("config autostart off"),
            [("autostart".into(), toml::Value::Boolean(false))]
        );
        assert_eq!(
            sets("config max_results défaut"),
            [("max_results".into(), toml::Value::Integer(8))]
        );
        // `> config transp` finds theme.opacity by fuzzy search.
        assert_eq!(titles("config transp")[0], "Opacité");
    }

    #[test]
    fn invalid_values_say_why_and_write_nothing() {
        assert!(sets("config max_results 99").is_empty());
        assert_eq!(titles("config max_results 99"), ["doit être entre 1 et 20"]);
        assert!(titles("config transparence 2")[0].contains("0.85"));
        assert!(sets("config theme.mode bleu").is_empty());
    }

    #[test]
    fn choices_and_booleans_list_their_values() {
        let modes = sets("config theme.mode ");
        assert_eq!(modes.len(), 3);
        assert_eq!(
            sets("config theme.mode da"),
            [("theme.mode".into(), "dark".into())]
        );
        assert_eq!(sets("config autostart").len(), 3, "row toggling + on + off");
    }

    #[test]
    fn shortcuts() {
        assert_eq!(
            sets("transparence 0.95"),
            [("theme.opacity".into(), toml::Value::Float(0.95))]
        );
        assert_eq!(
            sets("opacity 0.95"),
            [("theme.opacity".into(), toml::Value::Float(0.95))]
        );
        assert_eq!(
            sets("rendu cpu"),
            [("resources.renderer".into(), "cpu".into())]
        );
        assert_eq!(
            sets("calc angle rad"),
            [("calc.angle_unit".into(), "rad".into())]
        );
        assert_eq!(
            sets("calc chiffres 15"),
            [("calc.significant_digits".into(), toml::Value::Integer(15))]
        );
        assert_eq!(
            sets("calc devises on"),
            [("calc.currency".into(), toml::Value::Boolean(true))]
        );
        assert_eq!(
            sets("calc octets binaire"),
            [("calc.binary_prefixes".into(), toml::Value::Boolean(true))]
        );
        assert_eq!(
            sets("batterie pause off"),
            [(
                "resources.pause_on_battery".into(),
                toml::Value::Boolean(false)
            )]
        );
        let added = sets("fichiers exclure .venv");
        assert_eq!(added[0].0, "files.exclude");
        assert!(
            added[0]
                .1
                .as_array()
                .expect("list")
                .contains(&".venv".into())
        );
        let roots = sets("files add /data");
        assert_eq!(
            roots[0].1,
            toml::Value::Array(vec!["~".into(), "/data".into()])
        );
        assert_eq!(
            sets("fichiers cachés oui"),
            [("files.include_hidden".into(), toml::Value::Boolean(true))]
        );
        assert!(
            titles("fichiers exclure .git")[0].contains(".git"),
            "already there"
        );
    }

    #[test]
    fn languages_are_enabled_disabled_and_preferred() {
        let fr_off = sets("langue fr désactiver");
        assert_eq!(
            fr_off,
            [(
                "input_languages".into(),
                toml::Value::Array(vec!["en".into()])
            )]
        );
        let en_first = sets("language en prefer");
        assert_eq!(
            en_first,
            [(
                "input_languages".into(),
                toml::Value::Array(vec!["en".into(), "fr".into()])
            )]
        );
        with(&["en"], Lang::Fr, |cx| {
            let r = rows("langue en désactiver", cx);
            assert!(r[0].item.actions.is_empty(), "last language kept");
            assert_eq!(r[0].item.title, cx.t("language-last"));
            // French words are off, but `> langue fr activer` still works
            // with the common words… and `enable` in English.
            let r = rows("langue fr enable", cx);
            assert!(
                matches!(r[0].item.actions.first(), Some(Action::Internal { command }) if command.starts_with("set:input_languages"))
            );
        });
        assert!(titles("langue").iter().any(|t| t.starts_with("Français")));
    }

    #[test]
    fn web_engines_are_added_and_removed() {
        let added =
            sets("web ajouter yt YouTube https://www.youtube.com/results?search_query={query}");
        assert_eq!(added[0].0, "web");
        assert!(
            added[0]
                .1
                .as_array()
                .expect("list")
                .iter()
                .any(|e| e.get("keyword").and_then(toml::Value::as_str) == Some("yt"))
        );
        let removed = sets("web retirer g");
        assert!(
            !removed[0]
                .1
                .as_array()
                .expect("list")
                .iter()
                .any(|e| e.get("keyword").and_then(toml::Value::as_str) == Some("g"))
        );
        assert!(sets("web ajouter x Bad https://no-placeholder").is_empty());
        assert!(titles("web").iter().any(|t| t.contains("Google")));
    }
}

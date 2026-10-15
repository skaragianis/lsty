//! The scalar options: how each one reads a TOML value.

use super::{Config, DetailPosition, Hidden, LineNumbers, Value, quote};
use crate::{dates, todotxt::Priority};

/// Why a value was not taken: one reason per problem.
type Problems = Vec<String>;

/// Reads one option into the config; the problems are worded without the
/// option's name.
type Setter = fn(&mut Config, &Value) -> Result<(), Problems>;

/// The reader of the scalar option `name`, if there is one.
fn setter(name: &str) -> Option<Setter> {
    let setter: Setter = match name {
        "theme" => |c, v| text(v).map(|s| s.clone_into(&mut c.theme)),
        "leader" => |c, v| text(v).map(|s| s.clone_into(&mut c.leader)),
        "include_creation" => |c, v| flag(v).map(|b| c.include_creation = b),
        "auto_archive" => |c, v| flag(v).map(|b| c.auto_archive = b),
        "keep_priority" => |c, v| flag(v).map(|b| c.keep_priority = b),
        "keep_adding" => |c, v| flag(v).map(|b| c.keep_adding = b),
        "inherit_project" => |c, v| flag(v).map(|b| c.inherit_project = b),
        "inherit_context" => |c, v| flag(v).map(|b| c.inherit_context = b),
        "mouse" => |c, v| flag(v).map(|b| c.mouse = b),
        "default_priority" => default_priority,
        "priority_range" => |c, v| letter(v).map(|l| c.priority_range = l),
        "upcoming_days" => |c, v| number(v, 1).map(|n| c.upcoming_days = n),
        "hidden_items" => hidden_items,
        "show_sidebar" => |c, v| flag(v).map(|b| c.show_sidebar = b),
        "sidebar_min_width" => {
            |c, v| number(v, 0).map(|n| c.sidebar_min_width = n)
        }
        "show_detail" => |c, v| flag(v).map(|b| c.show_detail = b),
        "wrap" => |c, v| flag(v).map(|b| c.wrap = b),
        "show_ids" => |c, v| flag(v).map(|b| c.show_ids = b),
        "detail_min_width" => {
            |c, v| number(v, 0).map(|n| c.detail_min_width = n)
        }
        "detail_position" => |c, v| {
            choice(v, DetailPosition::ALL, DetailPosition::as_str)
                .map(|p| c.detail_position = p)
        },
        "line_numbers" => |c, v| {
            choice(v, LineNumbers::ALL, LineNumbers::as_str)
                .map(|m| c.line_numbers = m)
        },
        "lock_timeout" => |c, v| duration(v).map(|d| c.lock_timeout = d),
        "claim_stale_after" => {
            |c, v| duration(v).map(|d| c.claim_stale_after = d)
        }
        "start_filter" => |c, v| optional(v).map(|s| c.start_filter = s),
        "paste_command" => |c, v| optional(v).map(|s| c.paste_command = s),
        "copy_command" => |c, v| optional(v).map(|s| c.copy_command = s),
        "default_file" => |c, v| optional(v).map(|s| c.default_file = s),
        _ => return None,
    };
    Some(setter)
}

/// Reads the scalar option `name` into `cfg`. `None` when `name` is not
/// a scalar option; otherwise the problems, worded without the name.
pub(super) fn apply(
    cfg: &mut Config,
    name: &str,
    value: &Value,
) -> Option<Problems> {
    let set = setter(name)?;
    Some(set(cfg, value).err().unwrap_or_default())
}

/// Tells whether `name` is an option of `config.toml`.
pub fn is_option(name: &str) -> bool {
    setter(name).is_some() || super::load::is_table_option(name)
}

/// Returns `cfg` with one option changed as `config.toml` would read it.
/// When the value is not accepted, the reasons come back instead, each
/// starting with the option's name, except for an unknown option, whose
/// reason is that it is unknown.
pub fn set(
    cfg: &Config,
    name: &str,
    value: &Value,
) -> Result<Config, Vec<String>> {
    let mut changed = cfg.clone();
    match apply(&mut changed, name, value) {
        None => Err(vec![format!("unknown option {}", quote(name))]),
        Some(problems) if problems.is_empty() => Ok(changed),
        Some(problems) => Err(problems
            .into_iter()
            .map(|p| format!("{name}: {p}"))
            .collect()),
    }
}

fn problem<T>(text: impl Into<String>) -> Result<T, Problems> {
    Err(vec![text.into()])
}

fn text(value: &Value) -> Result<&str, Problems> {
    let Value::Str(s) = value else {
        return problem("expected string");
    };
    Ok(s)
}

/// A string where the empty string counts as unset.
fn optional(value: &Value) -> Result<Option<String>, Problems> {
    text(value).map(|s| Some(s.to_owned()).filter(|s| !s.is_empty()))
}

fn flag(value: &Value) -> Result<bool, Problems> {
    let Value::Bool(b) = value else {
        return problem("expected bool");
    };
    Ok(*b)
}

fn number(value: &Value, minimum: u64) -> Result<u64, Problems> {
    let Value::Int(n) = value else {
        return problem("expected integer");
    };
    match u64::try_from(*n) {
        Ok(n) if n >= minimum => Ok(n),
        _ => problem(format!("{n} must be {minimum} or more")),
    }
}

/// The one of `all` whose spelling `value` is.
fn choice<T: Copy, const N: usize>(
    value: &Value,
    all: [T; N],
    name: fn(T) -> &'static str,
) -> Result<T, Problems> {
    let s = text(value)?;
    let Some(found) = all.into_iter().find(|&c| name(c) == s) else {
        return problem(format!(
            "{} is not one of {}",
            quote(s),
            all.map(name).join(", ")
        ));
    };
    Ok(found)
}

/// One ASCII letter, uppercased.
fn letter(value: &Value) -> Result<Priority, Problems> {
    let s = text(value)?;
    Priority::parse(s).map_or_else(
        || problem(format!("{} must be one letter A-Z", quote(s))),
        Ok,
    )
}

fn default_priority(cfg: &mut Config, value: &Value) -> Result<(), Problems> {
    if matches!(value, Value::Str(s) if s.is_empty()) {
        cfg.default_priority = None;
        return Ok(());
    }
    letter(value).map(|l| cfg.default_priority = Some(l))
}

fn duration(value: &Value) -> Result<std::time::Duration, Problems> {
    let s = text(value)?;
    dates::parse_duration(s).or_else(|_| {
        problem(format!(
            "{} is not a duration such as 500ms, 2s, 4h or 2d",
            quote(s)
        ))
    })
}

/// The strings of a list, or `None` when the value is anything else.
pub(super) fn string_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::List(values) => Some(values.clone()),
        Value::Bool(_) | Value::Int(_) | Value::Str(_) | Value::Other => None,
    }
}

fn hidden_items(cfg: &mut Config, value: &Value) -> Result<(), Problems> {
    let Some(values) = string_list(value) else {
        return problem("expected list of strings");
    };
    let mut problems = Vec::new();
    let mut kept: Vec<Hidden> = Vec::new();
    for word in &values {
        match Hidden::ALL.into_iter().find(|h| h.as_str() == word) {
            None => problems.push(format!("unknown value {}", quote(word))),
            Some(kind) if !kept.contains(&kind) => kept.push(kind),
            Some(_) => {}
        }
    }
    cfg.hidden_items = kept;
    problems.is_empty().then_some(()).ok_or(problems)
}

use std::path::Path;

use toml::de::{DeTable, DeValue};

use super::{
    Config, Filter, Paths, Value, Warning,
    options::{self, string_list},
    quote,
};
use crate::{errs, errs::shown, fsio};

/// Reads the config file named by `paths`. It never fails: each problem
/// becomes a [`Warning`] and the affected option keeps its default.
///
/// A relative config path is opened from `cwd`, and messages keep the path
/// as given.
///
/// With `no_config` nothing is read and the defaults come back, marked.
pub fn load(
    paths: &Paths,
    cwd: &Path,
    no_config: bool,
) -> (Config, Vec<Warning>) {
    let mut cfg = Config::default();
    if no_config {
        cfg.no_config = true;
        return (cfg, Vec::new());
    }
    let data = match fsio::read_in(cwd, &paths.config_file) {
        Ok(Some(data)) => data,
        Ok(None) => {
            let missing = paths.explicit.then(|| {
                let shown = errs::lossy_path(&paths.config_file);
                Warning::config(format!("{shown} not found"))
            });
            return (cfg, missing.into_iter().collect());
        }
        Err(e) => return (cfg, vec![Warning::config(e.message().to_owned())]),
    };
    match parse(&data) {
        Ok(entries) => {
            let warnings = apply(&mut cfg, entries);
            (cfg, warnings)
        }
        Err(message) => (cfg, vec![Warning::config(message)]),
    }
}

/// One thing the file sets, with where it starts.
struct Entry {
    start: usize,
    what: What,
}

enum What {
    /// A top-level key that is not `hints` or `filters`.
    Option(String, Value),
    /// A top-level `hints` or `filters` that is not a table.
    NotTable(String),
    /// An entry of `[hints]`.
    Hint(String, Value),
    /// An entry of `[filters]`.
    Filter(String, Value),
}

/// The entries of the file in file order, or the message of the syntax
/// problem.
fn parse(data: &[u8]) -> Result<Vec<Entry>, String> {
    let table = parse_table(data)?;
    let mut entries = Vec::new();
    for (key, value) in table.get_ref() {
        top_entries(&mut entries, key, value);
    }
    entries.sort_by_key(|e| e.start);
    Ok(entries)
}

/// The TOML table of a file, or the `line N: …` message of its first
/// byte that is not UTF-8, its syntax error or its first number out of
/// range.
pub(crate) fn parse_table(
    data: &[u8],
) -> Result<toml::Spanned<DeTable<'_>>, String> {
    let text = utf8_text(data)?;
    let table = DeTable::parse(text).map_err(|e| syntax_message(data, &e))?;
    match range_error(text, data, table.get_ref()) {
        Some(message) => Err(message),
        None => Ok(table),
    }
}

fn utf8_text(data: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(data).map_err(|e| {
        let at = e.valid_up_to();
        let line = line_at(data, at);
        let byte = data[at];
        format!("line {line}: invalid UTF-8 byte: 0x{byte:02x}")
    })
}

/// The `line N: …` message of a TOML syntax error in `data`.
fn syntax_message(data: &[u8], e: &toml::de::Error) -> String {
    let at = e.span().map_or(0, |s| s.start);
    format!("line {}: {}", error_line(data, at), e.message())
}

/// A parsed TOML table: keys and values with their spans.
pub(crate) type Table<'a> = toml::map::Map<Key<'a>, Val<'a>>;

pub(crate) type Key<'a> = toml::Spanned<std::borrow::Cow<'a, str>>;
pub(crate) type Val<'a> = toml::Spanned<DeValue<'a>>;

/// Adds what the top-level `key` sets.
fn top_entries(out: &mut Vec<Entry>, key: &Key<'_>, value: &Val<'_>) {
    let name = key.get_ref().to_string();
    let start = key.span().start;
    if !is_table_option(&name) {
        let what = What::Option(name, convert(value));
        out.push(Entry { start, what });
        return;
    }
    let Some(table) = value.get_ref().as_table() else {
        out.push(Entry {
            start,
            what: What::NotTable(name),
        });
        return;
    };
    for (k, v) in table {
        let entry = k.get_ref().to_string();
        let what = if name == "hints" {
            What::Hint(entry, convert(v))
        } else {
            What::Filter(entry, convert(v))
        };
        out.push(Entry {
            start: k.span().start,
            what,
        });
    }
}

pub(super) fn is_table_option(name: &str) -> bool {
    name == "hints" || name == "filters"
}

/// The value as an option sees it. Tables are not kept.
fn convert(value: &Val<'_>) -> Value {
    match value.get_ref() {
        DeValue::String(s) => Value::Str(s.to_string()),
        DeValue::Boolean(b) => Value::Bool(*b),
        DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix())
            .map_or(Value::Other, Value::Int),
        DeValue::Array(_) => {
            toml_strings(value.get_ref()).map_or(Value::Other, Value::List)
        }
        DeValue::Table(_) | DeValue::Float(_) | DeValue::Datetime(_) => {
            Value::Other
        }
    }
}

/// The strings of an array, or `None` for anything else.
pub fn toml_strings(value: &DeValue<'_>) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|task| task.get_ref().as_str().map(str::to_owned))
        .collect()
}

/// The `line N: … is out of range` message for the first integer that
/// does not fit an `i64` or float that does not fit an `f64` anywhere in
/// `table`, values of unknown keys and nested tables included.
fn range_error(text: &str, data: &[u8], table: &Table<'_>) -> Option<String> {
    let mut first = None;
    for value in table.values() {
        first_out_of_range(value, &mut first);
    }
    let (at, end, kind) = first?;
    let number = &text[at..end];
    Some(format!(
        "line {}: {number} is out of range for {kind}",
        line_at(data, at)
    ))
}

/// Keeps in `first` the start, end and type of the earliest number of
/// `value` that is out of range.
fn first_out_of_range(
    value: &Val<'_>,
    first: &mut Option<(usize, usize, &'static str)>,
) {
    let span = value.span();
    let kind = match value.get_ref() {
        DeValue::Integer(i)
            if i64::from_str_radix(i.as_str(), i.radix()).is_err() =>
        {
            "int64"
        }
        DeValue::Float(f) if is_infinite_literal(f.as_str()) => "float64",
        DeValue::Array(tasks) => {
            for task in tasks {
                first_out_of_range(task, first);
            }
            return;
        }
        DeValue::Table(table) => {
            for v in table.values() {
                first_out_of_range(v, first);
            }
            return;
        }
        DeValue::String(_)
        | DeValue::Integer(_)
        | DeValue::Float(_)
        | DeValue::Boolean(_)
        | DeValue::Datetime(_) => return,
    };
    if first.is_none_or(|(at, _, _)| span.start < at) {
        *first = Some((span.start, span.end, kind));
    }
}

/// Whether a float literal is a finite number written too large.
fn is_infinite_literal(literal: &str) -> bool {
    literal.parse::<f64>().is_ok_and(f64::is_infinite)
        && !literal.trim_start_matches(['+', '-']).starts_with("inf")
}

/// The 1-based line of the byte at `offset`.
fn line_at(data: &[u8], offset: usize) -> usize {
    let upto = &data[..offset.min(data.len())];
    upto.split(|&b| b == b'\n').count()
}

/// The line of a syntax error at `offset`. An error at the very end of
/// text that ends in a newline belongs to the line before it.
fn error_line(data: &[u8], offset: usize) -> usize {
    if offset >= data.len() && data.ends_with(b"\n") {
        line_at(data, data.len() - 1)
    } else {
        line_at(data, offset)
    }
}

/// Reads the entries in file order.
fn apply(cfg: &mut Config, entries: Vec<Entry>) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for Entry { what, .. } in entries {
        match what {
            What::Option(name, value) => {
                add_option(cfg, &name, &value, &mut warnings);
            }
            What::NotTable(name) => {
                let message = format!("{name}: expected table");
                warnings.push(Warning::config(message));
            }
            What::Hint(mode, value) => {
                warnings
                    .extend(add_hint(cfg, &mode, &value).map(Warning::config));
            }
            What::Filter(name, value) => {
                warnings.extend(
                    add_filter(cfg, &name, &value).map(Warning::config),
                );
            }
        }
    }
    warnings
}

fn add_option(
    cfg: &mut Config,
    name: &str,
    value: &Value,
    warnings: &mut Vec<Warning>,
) {
    let Some(problems) = options::apply(cfg, name, value) else {
        let message = format!("unknown option {}", quote(name));
        warnings.push(Warning::config(message));
        return;
    };
    warnings.extend(
        problems
            .into_iter()
            .map(|p| Warning::config(format!("{name}: {p}"))),
    );
}

fn add_hint(cfg: &mut Config, mode: &str, value: &Value) -> Option<String> {
    let Some(list) = string_list(value) else {
        return Some(format!(
            "hints.{}: expected list of strings",
            shown(mode)
        ));
    };
    cfg.hints.insert(mode.to_owned(), list);
    None
}

fn add_filter(cfg: &mut Config, name: &str, value: &Value) -> Option<String> {
    let Value::Str(query) = value else {
        return Some(format!("filters.{}: expected string", shown(name)));
    };
    cfg.filters.push(Filter {
        name: name.to_owned(),
        query: query.clone(),
    });
    None
}

use std::fs::File;
use std::io::Read;
use std::path::Path;

use toml::de::{DeTable, DeValue};

use super::options::{self, string_list};
use super::{Config, Filter, Paths, Value, Warning, quote};
use crate::errs;

/// Reads the config file named by `paths`. It never fails: each problem
/// becomes a [`Warning`] and the affected option keeps its default.
///
/// With `no_config` nothing is read and the defaults come back, marked.
pub fn load(paths: &Paths, no_config: bool) -> (Config, Vec<Warning>) {
    let mut cfg = Config::default();
    if no_config {
        cfg.no_config = true;
        return (cfg, Vec::new());
    }
    let data = match read(&paths.config_file) {
        Ok(data) => data,
        Err(e) => return (cfg, read_warnings(paths, &e)),
    };
    match parse(&data) {
        Ok(entries) => {
            let warnings = apply(&mut cfg, entries);
            (cfg, warnings)
        }
        Err(message) => (cfg, vec![Warning::new(message)]),
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
    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(e) => {
            let at = e.valid_up_to();
            let line = line_at(data, at);
            let byte = data.get(at).copied().unwrap_or_default();
            return Err(format!(
                "line {line}: invalid UTF-8 byte: 0x{byte:02x}"
            ));
        }
    };
    let table = DeTable::parse(text).map_err(|e| {
        let at = e.span().map_or(0, |s| s.start);
        format!("line {}: {}", error_line(data, at), e.message())
    })?;
    let mut entries = Vec::new();
    let mut too_large = None;
    for (key, value) in table.get_ref() {
        top_entries(&mut entries, &mut too_large, key, value);
    }
    if let Some(at) = too_large {
        return Err(out_of_range(text, data, at));
    }
    entries.sort_by_key(|e| e.start);
    Ok(entries)
}

type Key<'a> = toml::Spanned<std::borrow::Cow<'a, str>>;
type Val<'a> = toml::Spanned<DeValue<'a>>;

/// Adds what the top-level `key` sets. `too_large` keeps the smallest
/// start of an integer too large to read.
fn top_entries(
    out: &mut Vec<Entry>,
    too_large: &mut Option<usize>,
    key: &Key<'_>,
    value: &Val<'_>,
) {
    let name = key.get_ref().to_string();
    let start = key.span().start;
    let converted = convert(value, too_large);
    let table = match value.get_ref() {
        DeValue::Table(table) if is_table_option(&name) => Some(table),
        DeValue::String(_)
        | DeValue::Integer(_)
        | DeValue::Float(_)
        | DeValue::Boolean(_)
        | DeValue::Datetime(_)
        | DeValue::Array(_)
        | DeValue::Table(_) => None,
    };
    let Some(table) = table else {
        let what = if is_table_option(&name) {
            What::NotTable(name)
        } else {
            What::Option(name, converted)
        };
        out.push(Entry { start, what });
        return;
    };
    // The table counts where its first key is.
    let what = What::Option(name.clone(), Value::Other);
    out.push(Entry { start, what });
    for (k, v) in table {
        let entry = k.get_ref().to_string();
        let what = if name == "hints" {
            What::Hint(entry, convert(v, too_large))
        } else {
            What::Filter(entry, convert(v, too_large))
        };
        out.push(Entry {
            start: k.span().start,
            what,
        });
    }
}

fn is_table_option(name: &str) -> bool {
    name == "hints" || name == "filters"
}

/// The value as an option sees it. Tables are walked for integers that
/// are too large but are not kept; `too_large` keeps the smallest start
/// of one.
fn convert(value: &Val<'_>, too_large: &mut Option<usize>) -> Value {
    match value.get_ref() {
        DeValue::String(s) => Value::Str(s.to_string()),
        DeValue::Boolean(b) => Value::Bool(*b),
        DeValue::Integer(i) => {
            let parsed = i64::from_str_radix(i.as_str(), i.radix());
            parsed.map_or_else(
                |_| {
                    let at = value.span().start;
                    *too_large = Some(too_large.map_or(at, |t| t.min(at)));
                    Value::Other
                },
                Value::Int,
            )
        }
        DeValue::Array(items) => {
            let items: Vec<Value> =
                items.iter().map(|i| convert(i, too_large)).collect();
            strings(items).map_or(Value::Other, Value::List)
        }
        DeValue::Table(table) => {
            for v in table.values() {
                convert(v, too_large);
            }
            Value::Other
        }
        DeValue::Float(_) | DeValue::Datetime(_) => Value::Other,
    }
}

/// The strings of `items`, or `None` if one is not a string.
fn strings(items: Vec<Value>) -> Option<Vec<String>> {
    items
        .into_iter()
        .map(|i| match i {
            Value::Str(s) => Some(s),
            Value::Bool(_) | Value::Int(_) | Value::List(_) | Value::Other => {
                None
            }
        })
        .collect()
}

fn out_of_range(text: &str, data: &[u8], at: usize) -> String {
    let rest = text.get(at..).unwrap_or_default();
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || "+-_".contains(c)))
        .unwrap_or(rest.len());
    let number = rest.get(..end).unwrap_or_default();
    format!(
        "line {}: {number} is out of range for int64",
        line_at(data, at)
    )
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

/// The file's bytes, or the failure and whether the file is missing.
fn read(path: &Path) -> Result<Vec<u8>, ReadError> {
    let mut file =
        File::open(path).map_err(|e| ReadError::new("open", path, e))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| ReadError::new("read", path, e))?;
    Ok(data)
}

struct ReadError {
    missing: bool,
    message: String,
}

impl ReadError {
    fn new(op: &str, path: &Path, err: std::io::Error) -> Self {
        let missing = err.kind() == std::io::ErrorKind::NotFound;
        let message = errs::io(op, path, err).message().to_owned();
        Self { missing, message }
    }
}

fn read_warnings(paths: &Paths, err: &ReadError) -> Vec<Warning> {
    let message = match (err.missing, paths.explicit) {
        (true, false) => return Vec::new(),
        (true, true) => {
            format!("{} not found", paths.config_file.display())
        }
        (false, _) => err.message.clone(),
    };
    vec![Warning::new(message)]
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
                warnings.push(Warning::new(message));
            }
            What::Hint(mode, value) => {
                warnings
                    .extend(add_hint(cfg, &mode, &value).map(Warning::new));
            }
            What::Filter(name, value) => {
                warnings
                    .extend(add_filter(cfg, &name, &value).map(Warning::new));
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
    if name == "hints" || name == "filters" {
        return;
    }
    let Some(problems) = options::apply(cfg, name, value) else {
        let message = format!("unknown option {}", quote(name));
        warnings.push(Warning::new(message));
        return;
    };
    warnings.extend(
        problems
            .into_iter()
            .map(|p| Warning::new(format!("{name}: {p}"))),
    );
}

fn add_hint(cfg: &mut Config, mode: &str, value: &Value) -> Option<String> {
    let Some(list) = string_list(value) else {
        return Some(format!("hints.{mode}: expected list of strings"));
    };
    cfg.hints.insert(mode.to_owned(), list);
    None
}

fn add_filter(cfg: &mut Config, name: &str, value: &Value) -> Option<String> {
    let Value::Str(query) = value else {
        return Some(format!("filters.{name}: expected string"));
    };
    cfg.filters.push(Filter {
        name: name.to_owned(),
        query: query.clone(),
    });
    None
}

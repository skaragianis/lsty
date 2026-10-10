//! `keys.toml` merged over the defaults.

use std::path::Path;

use toml::de::DeValue;

use super::{
    actions::{Action, Mode, action_in},
    defaults::defaults,
    matcher::{Binding, Matcher, starts_with},
    notation::{Stroke, parse_keys},
};
use crate::{
    config::{self, Table, Val, Warning, parse_table},
    errs::{self, quote},
    fsio,
};

const KEYS_SOURCE: &str = "keys.toml";

/// A key as written and as strokes.
#[derive(Debug)]
struct Written {
    text: String,
    strokes: Vec<Stroke>,
}

/// An action and the keys that want it.
#[derive(Debug)]
struct Claim {
    action: Action,
    keys: Vec<Written>,
}

/// The claims of every mode, in the order of `Mode::ALL`.
type Tables = [Vec<Claim>; Mode::ALL.len()];

/// The key bindings of every mode after merging `keys.toml`.
#[derive(Clone, Debug)]
pub struct Keymap {
    bindings: [Vec<Binding>; Mode::ALL.len()],
}

fn warn(message: String) -> Warning {
    Warning::new(KEYS_SOURCE, message)
}

/// Reads `keys.toml` over the defaults. `leader` is the `leader` option of
/// `config.toml`. A relative `path` is opened from `cwd`. It never fails:
/// each problem becomes a warning.
pub fn load(
    path: &Path,
    cwd: &Path,
    no_config: bool,
    leader: &str,
) -> (Keymap, Vec<Warning>) {
    let mut warnings = Vec::new();
    let stroke = parse_leader(leader, &mut warnings);
    let mut tables = default_claims();
    if !no_config {
        read_file(cwd, path, stroke, &mut tables, &mut warnings);
    }
    resolve_all(tables, warnings)
}

fn parse_leader(leader: &str, warnings: &mut Vec<Warning>) -> Stroke {
    if let Ok([one]) = parse_keys(leader, None).as_deref() {
        return *one;
    }
    let message = format!("leader: {} is not one key", quote(leader));
    warnings.push(Warning::config(message));
    Stroke::char(' ')
}

fn default_claims() -> Tables {
    Mode::ALL.map(|mode| {
        defaults(mode)
            .iter()
            .map(|&(action, written)| Claim {
                action,
                keys: written
                    .iter()
                    .filter_map(|&text| {
                        let strokes = parse_keys(text, None).ok()?;
                        Some(Written {
                            text: text.to_owned(),
                            strokes,
                        })
                    })
                    .collect(),
            })
            .collect()
    })
}

fn read_file(
    cwd: &Path,
    path: &Path,
    leader: Stroke,
    tables: &mut Tables,
    warnings: &mut Vec<Warning>,
) {
    let data = match fsio::read_in(cwd, path) {
        Ok(Some(data)) => data,
        Ok(None) => return,
        Err(e) => return warnings.push(warn(e.message().to_owned())),
    };
    match parse_table(&data) {
        Ok(table) => merge_file(table.get_ref(), leader, tables, warnings),
        Err(message) => warnings.push(warn(message)),
    }
}

/// One thing the file says, with where it starts.
struct Item {
    start: usize,
    what: What,
}

enum What {
    /// A table that is no mode.
    UnknownMode(String),
    /// A mode that is not a table.
    NotTable(Mode),
    Entry(Entry),
}

/// An entry of a mode's table.
struct Entry {
    mode: Mode,
    action: String,
    /// `None` when the value is no list of strings.
    list: Option<Vec<String>>,
}

fn merge_file(
    table: &Table<'_>,
    leader: Stroke,
    tables: &mut Tables,
    warnings: &mut Vec<Warning>,
) {
    let mut items = Vec::new();
    for (key, value) in table {
        collect(&mut items, key, value);
    }
    items.sort_by_key(|i| i.start);
    for item in items {
        match item.what {
            What::UnknownMode(name) => {
                warnings.push(warn(format!(
                    "unknown mode {}",
                    quote(format!("[{name}]"))
                )));
            }
            What::NotTable(mode) => {
                let name = mode.as_str();
                warnings.push(warn(format!("[{name}]: expected a table")));
            }
            What::Entry(entry) => {
                merge_entry(&entry, leader, tables, warnings);
            }
        }
    }
}

/// Adds what the top-level `key` says.
fn collect(items: &mut Vec<Item>, key: &config::Key<'_>, value: &Val<'_>) {
    let name = key.get_ref().to_string();
    let start = key.span().start;
    let Some(mode) = Mode::from_name(&name) else {
        items.push(Item {
            start,
            what: What::UnknownMode(name),
        });
        return;
    };
    let DeValue::Table(table) = value.get_ref() else {
        items.push(Item {
            start,
            what: What::NotTable(mode),
        });
        return;
    };
    for (k, v) in table {
        items.push(Item {
            start: k.span().start,
            what: What::Entry(Entry {
                mode,
                action: k.get_ref().to_string(),
                list: string_list(v.get_ref()),
            }),
        });
    }
}

fn string_list(value: &DeValue<'_>) -> Option<Vec<String>> {
    let DeValue::Array(items) = value else {
        return None;
    };
    items
        .iter()
        .map(|item| match item.get_ref() {
            DeValue::String(s) => Some(s.to_string()),
            DeValue::Integer(_)
            | DeValue::Float(_)
            | DeValue::Boolean(_)
            | DeValue::Datetime(_)
            | DeValue::Array(_)
            | DeValue::Table(_) => None,
        })
        .collect()
}

fn merge_entry(
    entry: &Entry,
    leader: Stroke,
    tables: &mut Tables,
    warnings: &mut Vec<Warning>,
) {
    let mode = entry.mode;
    let name = mode.as_str();
    let Some(action) = action_in(mode, &entry.action) else {
        let message =
            format!("[{name}] unknown action {}", quote(&entry.action));
        return warnings.push(warn(message));
    };
    let Some(written) = &entry.list else {
        let message = format!("[{name}] {action}: expected a list of keys");
        return warnings.push(warn(message));
    };
    let keys = parse_entry(mode, action, written, leader, warnings);
    if keys.is_empty() && !written.is_empty() {
        return;
    }
    let claims = &mut tables[mode.index()];
    claims.retain(|c| c.action != action);
    claims.push(Claim { action, keys });
}

fn parse_entry(
    mode: Mode,
    action: Action,
    written: &[String],
    leader: Stroke,
    warnings: &mut Vec<Warning>,
) -> Vec<Written> {
    let mut keys = Vec::new();
    for text in written {
        match parse_key(mode, text, leader) {
            Ok(strokes) => keys.push(Written {
                text: text.clone(),
                strokes,
            }),
            Err(e) => warnings
                .push(warn(format!("[{}] {action}: {e}", mode.as_str()))),
        }
    }
    keys
}

fn parse_key(
    mode: Mode,
    text: &str,
    leader: Stroke,
) -> errs::Result<Vec<Stroke>> {
    let strokes = parse_keys(text, Some(leader))?;
    if mode.reserves_digits() && starts_with_count(&strokes) {
        let message =
            format!("invalid key {}: starts with a digit", quote(text));
        return Err(errs::Error::new(errs::Code::InvalidValue, message));
    }
    Ok(strokes)
}

fn starts_with_count(strokes: &[Stroke]) -> bool {
    strokes
        .first()
        .and_then(|s| s.plain_digit())
        .is_some_and(|d| d != 0)
}

fn resolve_all(
    tables: Tables,
    mut warnings: Vec<Warning>,
) -> (Keymap, Vec<Warning>) {
    let mut keymap = Keymap {
        bindings: Mode::ALL.map(|_| Vec::new()),
    };
    for (mode, mut claims) in Mode::ALL.into_iter().zip(tables) {
        resolve_conflicts(mode, &mut claims, &mut warnings);
        keymap.bindings[mode.index()] =
            drop_hidden(mode, &flatten(&claims), &mut warnings);
    }
    (keymap, warnings)
}

/// Takes a key from every earlier claim of it: the later claim wins.
fn resolve_conflicts(
    mode: Mode,
    claims: &mut [Claim],
    warnings: &mut Vec<Warning>,
) {
    for i in 0..claims.len() {
        let (before, rest) = claims.split_at_mut(i);
        let Some(winner) = rest.first() else {
            continue;
        };
        for key in &winner.keys {
            for owner in before.iter_mut() {
                warnings.extend(take_key(mode, owner, winner.action, key));
            }
        }
    }
}

fn take_key(
    mode: Mode,
    owner: &mut Claim,
    winner: Action,
    key: &Written,
) -> Option<Warning> {
    let held = owner.keys.iter().any(|o| o.strokes == key.strokes);
    if !held {
        return None;
    }
    owner.keys.retain(|o| o.strokes != key.strokes);
    Some(warn(format!(
        "[{}] {} is bound to {} and {winner}; {winner} wins",
        mode.as_str(),
        quote(&key.text),
        owner.action,
    )))
}

fn flatten(claims: &[Claim]) -> Vec<Binding> {
    let mut bound: Vec<Binding> = Vec::new();
    for claim in claims {
        for key in &claim.keys {
            if bound.iter().all(|b| b.strokes != key.strokes) {
                bound.push(Binding {
                    action: claim.action,
                    written: key.text.clone(),
                    strokes: key.strokes.clone(),
                });
            }
        }
    }
    bound
}

/// Drops each binding that is the start of a longer one.
fn drop_hidden(
    mode: Mode,
    bound: &[Binding],
    warnings: &mut Vec<Warning>,
) -> Vec<Binding> {
    let mut kept = Vec::new();
    for b in bound {
        let hider = bound.iter().find(|o| starts_with(&o.strokes, &b.strokes));
        match hider {
            None => kept.push(b.clone()),
            Some(o) => warnings.push(warn(format!(
                "[{}] {} is hidden by {}",
                mode.as_str(),
                quote(&b.written),
                quote(&o.written),
            ))),
        }
    }
    kept
}

impl Keymap {
    /// The keys of `action` in `mode` as written, in order.
    pub fn keys(&self, mode: Mode, action: Action) -> Vec<&str> {
        self.table(mode)
            .iter()
            .filter(|b| b.action == action)
            .map(|b| b.written.as_str())
            .collect()
    }

    pub(super) fn table(&self, mode: Mode) -> &[Binding] {
        &self.bindings[mode.index()]
    }

    /// A matcher for `mode`.
    pub fn matcher(&self, mode: Mode) -> Matcher {
        Matcher::new(self.table(mode).to_vec(), mode.has_counts())
    }

    /// A matcher for the bindings of `top` over those of `base`; it takes
    /// no counts.
    pub fn layered(&self, top: Mode, base: Mode) -> Matcher {
        let mut bindings = self.table(top).to_vec();
        bindings.extend_from_slice(self.table(base));
        Matcher::new(bindings, false)
    }
}

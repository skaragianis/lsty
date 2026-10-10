//! `keys.toml` merged over the defaults.

use std::path::Path;

use toml::de::{DeTable, DeValue};

use super::{
    actions::{Action, Mode, action_in},
    defaults::defaults,
    matcher::{Binding, Matcher, starts_with},
    notation::{Key, Stroke, parse_keys},
};
use crate::{
    config::{Table, Warning, range_error, syntax_message, utf8_text},
    errs::{self, quote},
    fsio,
};

const KEYS_SOURCE: &str = "keys.toml";

/// A key as written and as strokes.
#[derive(Clone, Debug)]
struct Written {
    text: String,
    strokes: Vec<Stroke>,
}

/// An action and the keys that want it.
#[derive(Clone, Debug)]
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
    let (stroke, mut warnings) = parse_leader(leader);
    let mut tables = default_claims();
    if !no_config {
        warnings.extend(read_file(cwd, path, stroke, &mut tables));
    }
    resolve_all(&tables, warnings)
}

fn parse_leader(leader: &str) -> (Stroke, Vec<Warning>) {
    let space = Stroke::char(' ');
    if let Ok([one]) = parse_keys(leader, None).as_deref() {
        return (*one, Vec::new());
    }
    let message = format!("leader: {} is not one key", quote(leader));
    (space, vec![Warning::config(message)])
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
) -> Vec<Warning> {
    let data = match fsio::read_in(cwd, path) {
        Ok(Some(data)) => data,
        Ok(None) => return Vec::new(),
        Err(e) => return vec![warn(e.message().to_owned())],
    };
    let text = match utf8_text(&data) {
        Ok(text) => text,
        Err(message) => return vec![warn(message)],
    };
    let table = match DeTable::parse(text) {
        Ok(table) => table,
        Err(e) => return vec![warn(syntax_message(&data, &e))],
    };
    if let Some(message) = range_error(text, &data, table.get_ref()) {
        return vec![warn(message)];
    }
    merge_file(table.get_ref(), leader, tables)
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
    /// An entry of a mode's table; `None` when the value is no list of
    /// strings.
    Entry(Mode, String, Option<Vec<String>>),
}

fn merge_file(
    table: &Table<'_>,
    leader: Stroke,
    tables: &mut Tables,
) -> Vec<Warning> {
    let mut items = Vec::new();
    for (key, value) in table {
        collect(&mut items, key, value);
    }
    items.sort_by_key(|i| i.start);
    let mut warnings = Vec::new();
    for item in items {
        match item.what {
            What::UnknownMode(name) => {
                warnings.push(warn(format!("unknown mode \"[{name}]\"")));
            }
            What::NotTable(mode) => {
                let name = mode.as_str();
                warnings.push(warn(format!("[{name}]: expected a table")));
            }
            What::Entry(mode, action, list) => warnings.extend(merge_entry(
                mode,
                &action,
                list.as_deref(),
                leader,
                tables,
            )),
        }
    }
    warnings
}

type SpannedKey<'a> = toml::Spanned<std::borrow::Cow<'a, str>>;

/// Adds what the top-level `key` says.
fn collect(
    items: &mut Vec<Item>,
    key: &SpannedKey<'_>,
    value: &toml::Spanned<DeValue<'_>>,
) {
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
            what: What::Entry(
                mode,
                k.get_ref().to_string(),
                string_list(v.get_ref()),
            ),
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
    mode: Mode,
    action: &str,
    list: Option<&[String]>,
    leader: Stroke,
    tables: &mut Tables,
) -> Vec<Warning> {
    let name = mode.as_str();
    let Some(action) = action_in(mode, action) else {
        return vec![warn(format!(
            "[{name}] unknown action {}",
            quote(action)
        ))];
    };
    let Some(written) = list else {
        return vec![warn(format!(
            "[{name}] {action}: expected a list of keys"
        ))];
    };
    let (keys, warnings) = parse_entry(mode, action, written, leader);
    if keys.is_empty() && !written.is_empty() {
        return warnings;
    }
    let claims = &mut tables[mode.index()];
    claims.retain(|c| c.action != action);
    claims.push(Claim { action, keys });
    warnings
}

fn parse_entry(
    mode: Mode,
    action: Action,
    written: &[String],
    leader: Stroke,
) -> (Vec<Written>, Vec<Warning>) {
    let mut keys = Vec::new();
    let mut warnings = Vec::new();
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
    (keys, warnings)
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
    strokes.first().is_some_and(|s| {
        s.mods.is_empty()
            && matches!(s.key, Key::Char(c) if ('1'..='9').contains(&c))
    })
}

fn resolve_all(
    tables: &Tables,
    mut warnings: Vec<Warning>,
) -> (Keymap, Vec<Warning>) {
    let mut keymap = Keymap {
        bindings: Mode::ALL.map(|_| Vec::new()),
    };
    for mode in Mode::ALL {
        let mut claims = tables[mode.index()].clone();
        warnings.extend(resolve_conflicts(mode, &mut claims));
        let (kept, hidden) = drop_hidden(mode, &flatten(&claims));
        keymap.bindings[mode.index()] = kept;
        warnings.extend(hidden);
    }
    (keymap, warnings)
}

/// Takes a key from every earlier claim of it: the later claim wins.
fn resolve_conflicts(mode: Mode, claims: &mut [Claim]) -> Vec<Warning> {
    let mut warnings = Vec::new();
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
    warnings
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
fn drop_hidden(mode: Mode, bound: &[Binding]) -> (Vec<Binding>, Vec<Warning>) {
    let mut kept = Vec::new();
    let mut warnings = Vec::new();
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
    (kept, warnings)
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

//! The property the fuzz target checks, behind the `fuzzing` feature.
#![expect(clippy::panic, reason = "a property fails by panicking")]

use toml::de::DeValue;

use super::{
    actions::{Action, Mode, action_in},
    defaults::defaults,
    load::{Keymap, merge, parse_key},
    matcher::{Binding, Matcher, Outcome, starts_with},
    notation::{Mods, Named, Stroke},
};
use crate::{
    config::{
        DEFAULT_LEADER, Key, Table, Val, Warning, parse_table, toml_strings,
    },
    errs::quote,
};

/// An entry of the file that replaces an action's keys: where it starts
/// and its keys that parse.
struct Rebind {
    mode: Mode,
    action: Action,
    start: usize,
    keys: Vec<Vec<Stroke>>,
    /// Each of `keys` as written.
    written: Vec<String>,
}

/// Any bytes as `keys.toml` load without a panic; each problem is a
/// warning that names it, and the keymap keeps the rules of the file:
/// an action the file does not rebind holds only its default keys, and
/// each stays bound to it or to the user's action that took it, or is
/// hidden by a longer key of the user's; a rebound action has only the
/// keys it was given, and the last entry of a key wins it; the detail
/// panel inherits the normal keys of the actions it does not claim
/// unless they clash with its own; no key of a mode is the same as or
/// the start of another, nor does an insert key start a completion key;
/// and every matcher reaches each binding it holds.
///
/// # Panics
///
/// When the property fails.
pub fn keys_property(data: &[u8]) {
    let (map, warnings) =
        merge(Ok(Some(data.to_vec())), &DEFAULT_LEADER.to_string());
    for w in &warnings {
        let names = w.message.starts_with("line ")
            || w.message.starts_with("unknown mode \"[")
            || Mode::ALL
                .iter()
                .any(|m| w.message.starts_with(&format!("[{}]", m.as_str())));
        assert!(
            w.source == "keys.toml" && names,
            "a warning names nothing: {w:?}"
        );
    }
    let rebinds = parse_table(data).map_or_else(
        |_| {
            assert_eq!(warnings.len(), 1, "a file that does not parse");
            Vec::new()
        },
        |table| rebinds(table.get_ref(), &warnings),
    );
    let pool = pool(&map);
    for mode in Mode::ALL {
        let table = map.table(mode);
        let ours: Vec<&Rebind> =
            rebinds.iter().filter(|r| r.mode == mode).collect();
        no_clash(mode, table);
        defaults_kept(mode, table, &ours);
        let below = if mode == Mode::Completion {
            map.table(Mode::Insert)
        } else {
            &[]
        };
        rebinds_kept(mode, table, below, &ours, &warnings);
        inherited(&map, mode, &ours);
        let mut m = map.matcher(mode);
        for b in table {
            reaches(&mut m, &b.strokes, b.action);
        }
        feed_any(&mut m, data, &pool);
    }
    let mut layered = map.layered(Mode::Completion, Mode::Insert);
    for c in map.table(Mode::Completion) {
        reaches(&mut layered, &c.strokes, c.action);
    }
    feed_any(&mut layered, data, &pool);
}

/// The entries of the file that rebind an action: a list of keys that is
/// empty or has a key that parses. Each problem on the way must be among
/// the `warnings`.
fn rebinds(table: &Table<'_>, warnings: &[Warning]) -> Vec<Rebind> {
    let mut out = Vec::new();
    for (name, value) in table {
        let Some(mode) = Mode::from_name(name.get_ref()) else {
            let name = quote(format!("[{}]", name.get_ref()));
            warned(warnings, &format!("unknown mode {name}"));
            continue;
        };
        let DeValue::Table(entries) = value.get_ref() else {
            warned(
                warnings,
                &format!("[{}]: expected a table", mode.as_str()),
            );
            continue;
        };
        out.extend(
            entries
                .iter()
                .filter_map(|(k, v)| rebind(mode, k, v, warnings)),
        );
    }
    out
}

fn rebind(
    mode: Mode,
    key: &Key<'_>,
    value: &Val<'_>,
    warnings: &[Warning],
) -> Option<Rebind> {
    let m = mode.as_str();
    let Some(action) = action_in(mode, key.get_ref()) else {
        let name = quote(key.get_ref().as_ref());
        warned(warnings, &format!("[{m}] unknown action {name}"));
        return None;
    };
    let Some(list) = toml_strings(value.get_ref()) else {
        warned(
            warnings,
            &format!("[{m}] {action}: expected a list of keys"),
        );
        return None;
    };
    let (keys, written): (Vec<_>, Vec<_>) = list
        .iter()
        .filter_map(|text| match parse_key(mode, text, leader()) {
            Ok(strokes) => Some((strokes, text.clone())),
            Err(e) => {
                assert!(e.message().contains(&quote(text)), "{e:?}");
                warned(warnings, &format!("[{m}] {action}: {e}"));
                None
            }
        })
        .collect();
    (list.is_empty() || !keys.is_empty()).then_some(Rebind {
        mode,
        action,
        start: key.span().start,
        keys,
        written,
    })
}

fn leader() -> Stroke {
    Stroke::char(DEFAULT_LEADER)
}

fn warned(warnings: &[Warning], message: &str) {
    assert!(
        warnings.iter().any(|w| w.message == message),
        "no warning {message:?} in {warnings:?}"
    );
}

/// No key of `table` is the same as or the start of another.
fn no_clash(mode: Mode, table: &[Binding]) {
    for (i, a) in table.iter().enumerate() {
        for b in &table[i + 1..] {
            assert!(
                !a.strokes.starts_with(&b.strokes)
                    && !b.strokes.starts_with(&a.strokes),
                "[{}] {a:?} and {b:?} clash",
                mode.as_str()
            );
        }
    }
}

/// An action the file does not rebind has only its default keys, and
/// each is still its, or the file gave it, or a longer key it starts, to
/// an action of this mode. Defaults never hide defaults, so no other key
/// can take it.
fn defaults_kept(mode: Mode, table: &[Binding], ours: &[&Rebind]) {
    for &(action, written) in defaults(mode) {
        if ours.iter().any(|r| r.action == action) {
            continue;
        }
        let keys: Vec<_> = written
            .iter()
            .map(|text| {
                parse_key(mode, text, leader())
                    .unwrap_or_else(|e| panic!("default {text:?}: {e:?}"))
            })
            .collect();
        for b in table.iter().filter(|b| b.action == action) {
            assert!(
                keys.contains(&b.strokes),
                "[{}] {action} gained {:?}",
                mode.as_str(),
                b.written
            );
        }
        for (text, s) in written.iter().zip(keys) {
            assert!(
                table.iter().any(|b| b.strokes.starts_with(&s)
                    && (b.strokes == s && b.action == action
                        || ours.iter().any(|r| {
                            r.action == b.action && r.keys.contains(&b.strokes)
                        }))),
                "[{}] {action} lost its default {text:?}",
                mode.as_str()
            );
        }
    }
}

/// A rebound action has only keys the file gave it, and keeps each that
/// no later entry gives another and no longer key of an action this mode
/// claims hides, nor a key of `below`, the layer under it, starts: that
/// one is warned of.
fn rebinds_kept(
    mode: Mode,
    table: &[Binding],
    below: &[Binding],
    ours: &[&Rebind],
    warnings: &[Warning],
) {
    for r in ours {
        for b in table.iter().filter(|b| b.action == r.action) {
            assert!(
                r.keys.contains(&b.strokes),
                "[{}] {} kept {:?}, which the file did not give it",
                mode.as_str(),
                r.action,
                b.written
            );
        }
        for s in &r.keys {
            let later =
                ours.iter().any(|o| o.start > r.start && o.keys.contains(s));
            let kept = table.iter().any(|b| {
                (b.action == r.action && b.strokes == *s)
                    || starts_with(&b.strokes, s)
                        && claims(mode, b.action, ours)
            });
            let under = below.iter().find(|i| starts_with(s, &i.strokes));
            if let Some(i) = under.filter(|_| !later) {
                // The first spelling of the key in the list is the one kept.
                let first = r.keys.iter().position(|k| k == s).unwrap_or(0);
                warned(
                    warnings,
                    &format!(
                        "[{}] {} is hidden by {}",
                        mode.as_str(),
                        quote(&r.written[first]),
                        quote(&i.written)
                    ),
                );
            }
            assert!(
                later || kept || under.is_some(),
                "[{}] {} lost a key the file gave it last: {s:?}",
                mode.as_str(),
                r.action
            );
        }
    }
}

/// A binding of an action `mode` does not claim is one the detail panel
/// inherits: the normal binding of such an action, which it holds unless
/// it clashes with one of its own.
fn inherited(map: &Keymap, mode: Mode, ours: &[&Rebind]) {
    let claims = |action| claims(mode, action, ours);
    let normal = map.table(Mode::Normal);
    let table = map.table(mode);
    for b in table.iter().filter(|b| !claims(b.action)) {
        assert!(
            mode == Mode::Detail && normal.contains(b),
            "[{}] {b:?} is not its own or inherited",
            mode.as_str()
        );
    }
    if mode != Mode::Detail {
        return;
    }
    for n in normal.iter().filter(|n| !claims(n.action)) {
        assert!(
            table.iter().any(|b| b.strokes.starts_with(&n.strokes)
                || n.strokes.starts_with(&b.strokes)),
            "[detail] does not inherit {n:?}"
        );
    }
}

/// Whether `mode` has `action` in its defaults or in an entry of the file;
/// `ours` are the entries of `mode`.
fn claims(mode: Mode, action: Action, ours: &[&Rebind]) -> bool {
    defaults(mode).iter().any(|&(a, _)| a == action)
        || ours.iter().any(|r| r.action == action)
}

/// Feeding `strokes` to `m` from the start ends in `action`.
fn reaches(m: &mut Matcher, strokes: &[Stroke], action: Action) {
    m.reset();
    let mut last = Outcome::NoMatch;
    for &s in strokes {
        last = m.feed(s).outcome;
    }
    assert_eq!(
        last,
        Outcome::Matched { action, count: 0 },
        "{strokes:?} does not reach {action}"
    );
}

/// The strokes of every binding, `<Esc>` and `<C-c>`.
fn pool(map: &Keymap) -> Vec<Stroke> {
    let mut pool = vec![
        Stroke::named(Named::Esc),
        Stroke::char('c').with_mods(Mods::CTRL),
    ];
    for mode in Mode::ALL {
        for b in map.table(mode) {
            pool.extend(&b.strokes);
        }
    }
    pool
}

/// Feeds `m` a stroke for each byte of `data`: a printable one is its
/// character, as typed; any other picks one of `pool`.
fn feed_any(m: &mut Matcher, data: &[u8], pool: &[Stroke]) {
    m.reset();
    for &byte in data {
        m.feed(if byte.is_ascii_graphic() {
            Stroke::char(char::from(byte))
        } else {
            pool[usize::from(byte) % pool.len()]
        });
    }
}

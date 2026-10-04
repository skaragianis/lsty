//! The hint row: the keys of a mode and what they do.

use lsty_core::config::Warning;
use lsty_core::errs::quote;
use lsty_core::keys::{Action, Keymap, Mode};
use lsty_core::theme::Slot;
use std::collections::BTreeMap;

use crate::text::Span;

const CONFIG: &str = "config.toml";

/// The actions of a mode's hints when `[hints]` says nothing.
pub(crate) fn defaults(mode: Mode) -> &'static [Action] {
    match mode {
        Mode::Normal => &[
            Action::ToggleComplete,
            Action::CycleState,
            Action::InsertBelow,
            Action::EditAppend,
            Action::Delete,
            Action::Undo,
            Action::Search,
            Action::Help,
        ],
        Mode::Visual => &[
            Action::ToggleComplete,
            Action::CycleState,
            Action::Delete,
            Action::Yank,
            Action::ExitVisual,
        ],
        Mode::Insert => &[Action::Commit, Action::Cancel],
        Mode::Completion => &[Action::Accept, Action::Next, Action::Close],
        Mode::Command => {
            &[Action::Execute, Action::CompleteNext, Action::Cancel]
        }
        Mode::Panel => &[
            Action::SelectAndJump,
            Action::ToggleEntry,
            Action::Fold,
            Action::Unfold,
            Action::FocusNext,
        ],
        Mode::Help => &[Action::Search, Action::Close],
        Mode::Picker => &[Action::Accept, Action::Close],
    }
}

fn warning(message: String) -> Warning {
    Warning {
        source: CONFIG.to_owned(),
        message,
    }
}

/// The problems of the `[hints]` table: a mode or an action that does
/// not exist.
pub(crate) fn warnings(hints: &BTreeMap<String, Vec<String>>) -> Vec<Warning> {
    let mut out = Vec::new();
    for (mode, names) in hints {
        if Mode::from_name(mode).is_none() {
            out.push(warning(format!("hints.{mode}: unknown mode")));
            continue;
        }
        for name in names.iter().filter(|n| Action::from_name(n).is_none()) {
            out.push(warning(format!(
                "hints.{mode}: unknown action {}",
                quote(name)
            )));
        }
    }
    out
}

/// The hints of `mode`, in order: the first key of each action and its
/// label. An action without a key has no hint.
pub(crate) fn hints<'a>(
    mode: Mode,
    configured: &BTreeMap<String, Vec<String>>,
    keymap: &'a Keymap,
) -> Vec<[Span<'a>; 3]> {
    let actions: Vec<Action> = match configured.get(mode.as_str()) {
        Some(names) => {
            names.iter().filter_map(|n| Action::from_name(n)).collect()
        }
        None => defaults(mode).to_vec(),
    };
    actions
        .into_iter()
        .filter_map(|action| {
            let written = keymap.keys(mode, action).into_iter().next()?;
            Some([
                Span::new(key_label(written), Some(Slot::HintKey)),
                Span::new(" ", None),
                Span::new(label(mode, action), Some(Slot::HintDesc)),
            ])
        })
        .collect()
}

fn label(mode: Mode, action: Action) -> &'static str {
    if mode == Mode::Picker && action == Action::Accept {
        "open"
    } else {
        action.hint()
    }
}

/// The pieces of a binding as written: `<...>` groups and single
/// characters.
fn pieces(written: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = written;
    while let Some(c) = rest.chars().next() {
        let size = group_len(rest).unwrap_or_else(|| c.len_utf8());
        let (piece, tail) = rest.split_at(size);
        out.push(piece);
        rest = tail;
    }
    out
}

/// The length of the `<...>` group that `s` starts with.
fn group_len(s: &str) -> Option<usize> {
    let inner = s.strip_prefix('<')?;
    let close = inner.find(['<', '>'])?;
    (close > 0 && inner.get(close..)?.starts_with('>')).then_some(close + 2)
}

/// A binding in the readable form of the hint row: `<C-r>` is `Ctrl-r`,
/// and keys are told apart by a space when one of them has a name.
pub(crate) fn key_label(written: &str) -> String {
    let mut out = String::new();
    let mut prev = 0;
    for piece in pieces(written) {
        let label = piece_label(piece);
        let chars = label.chars().count();
        if prev > 0 && (prev > 1 || chars > 1) {
            out.push(' ');
        }
        out.push_str(&label);
        prev = label.trim().chars().count();
    }
    out
}

const NAMES: [(&str, &str); 18] = [
    ("cr", "Enter"),
    ("enter", "Enter"),
    ("return", "Enter"),
    ("esc", "Esc"),
    ("tab", "Tab"),
    ("space", "Space"),
    ("up", "↑"),
    ("down", "↓"),
    ("left", "←"),
    ("right", "→"),
    ("bs", "Backspace"),
    ("del", "Delete"),
    ("lt", "<"),
    ("pageup", "PageUp"),
    ("pagedown", "PageDown"),
    ("home", "Home"),
    ("end", "End"),
    ("leader", "Leader"),
];

fn modifier(c: char) -> Option<&'static str> {
    match c.to_ascii_lowercase() {
        'c' => Some("Ctrl"),
        'a' => Some("Alt"),
        's' => Some("Shift"),
        _ => None,
    }
}

fn piece_label(piece: &str) -> String {
    let Some(inner) = piece
        .strip_prefix('<')
        .and_then(|p| p.strip_suffix('>'))
        .filter(|i| !i.is_empty() && piece.len() >= 3)
    else {
        return piece.to_owned();
    };
    let mut parts: Vec<&str> = Vec::new();
    let mut inner = inner;
    while inner.len() > 2 && inner.get(1..2) == Some("-") {
        let Some(label) = inner.chars().next().and_then(modifier) else {
            break;
        };
        parts.push(label);
        inner = inner.get(2..).unwrap_or_default();
    }
    let lower = inner.to_lowercase();
    let named = NAMES.iter().find(|(k, _)| *k == lower).map(|&(_, v)| v);
    parts.push(named.unwrap_or(inner));
    parts.join("-")
}

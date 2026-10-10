//! The hint row: the keys of a mode and what they do.

use std::collections::BTreeMap;

use lsty_core::config::Warning;
use lsty_core::errs::quote;
use lsty_core::keys::{
    Action, Key, Keymap, Mode, Mods, Named, Piece, Stroke, pieces,
};
use lsty_core::theme::Slot;

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
        Mode::Picker => &[Action::Accept, Action::Search, Action::Close],
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
/// label, with `accept` as the label of a picker's choice. An action
/// without a key has no hint.
pub(crate) fn hints<'a>(
    mode: Mode,
    configured: &BTreeMap<String, Vec<String>>,
    keymap: &'a Keymap,
    hidden: &[Action],
    accept: &'static str,
) -> Vec<[Span<'a>; 3]> {
    let actions: Vec<Action> = match configured.get(mode.as_str()) {
        Some(names) => {
            names.iter().filter_map(|n| Action::from_name(n)).collect()
        }
        None => defaults(mode).to_vec(),
    };
    actions
        .into_iter()
        .filter(|action| !hidden.contains(action))
        .filter_map(|action| {
            let written = keymap.keys(mode, action).into_iter().next()?;
            Some([
                Span::new(key_label(written), Some(Slot::HintKey)),
                Span::new(" ", None),
                Span::new(label(mode, action, accept), Some(Slot::HintDesc)),
            ])
        })
        .collect()
}

fn label(mode: Mode, action: Action, accept: &'static str) -> &'static str {
    if mode == Mode::Picker && action == Action::Accept {
        accept
    } else {
        action.hint()
    }
}

/// A binding in the readable form of the hint row: `<C-r>` is `Ctrl-r`,
/// and keys are told apart by a space when one of them has a name.
pub(crate) fn key_label(written: &str) -> String {
    let mut out = String::new();
    let mut prev = 0;
    for piece in pieces(written) {
        let label = piece_label(&piece);
        let chars = label.chars().count();
        if prev > 0 && (prev > 1 || chars > 1) {
            out.push(' ');
        }
        out.push_str(&label);
        prev = label.chars().count();
    }
    out
}

fn piece_label(piece: &Piece) -> String {
    let Piece::Stroke(Stroke { key, mods }) = piece else {
        return "Leader".to_owned();
    };
    let held = [
        (Mods::CTRL, "Ctrl"),
        (Mods::ALT, "Alt"),
        (Mods::SHIFT, "Shift"),
    ];
    let mut parts: Vec<String> = held
        .iter()
        .filter(|(m, _)| mods.has(*m))
        .map(|&(_, name)| name.to_owned())
        .collect();
    parts.push(match key {
        Key::Char(' ') => "Space".to_owned(),
        Key::Char(c) => c.to_string(),
        Key::Named(n) => named_label(*n).to_owned(),
    });
    parts.join("-")
}

fn named_label(named: Named) -> &'static str {
    match named {
        Named::Cr => "Enter",
        Named::Bs => "Backspace",
        Named::Del => "Delete",
        Named::Up => "↑",
        Named::Down => "↓",
        Named::Left => "←",
        Named::Right => "→",
        Named::Esc
        | Named::Tab
        | Named::Home
        | Named::End
        | Named::PageUp
        | Named::PageDown
        | Named::F(_) => named.as_str(),
    }
}

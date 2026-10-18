//! The hint row: the keys of a mode and what they do.

use std::collections::BTreeMap;

use lsty_core::{
    config::Warning,
    errs::{quote, shown},
    keys::{Action, Keymap, Mode},
    theme::Slot,
};

use crate::text::Span;

/// The actions of a mode's hints when `[hints]` says nothing.
pub(crate) fn defaults(mode: Mode) -> &'static [Action] {
    match mode {
        Mode::Normal => &[
            Action::ToggleComplete,
            Action::CycleState,
            Action::Stop,
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
            Action::Stop,
            Action::Delete,
            Action::Yank,
            Action::ExitVisual,
        ],
        Mode::Insert => &[
            Action::Commit,
            Action::Cancel,
            Action::DeleteWordBack,
            Action::DeleteToStart,
        ],
        Mode::Completion => &[Action::Accept, Action::Next, Action::Close],
        Mode::Command => {
            &[Action::Execute, Action::CompleteNext, Action::Cancel]
        }
        Mode::Panel => &[
            Action::ToggleEntry,
            Action::SelectAndJump,
            Action::Fold,
            Action::Unfold,
            Action::FocusNext,
        ],
        Mode::Detail => &[
            Action::OpenLink,
            Action::NextLink,
            Action::ToggleComplete,
            Action::Delete,
            Action::FocusList,
        ],
        Mode::Help => &[Action::Search, Action::Close],
        Mode::Picker => &[Action::Accept, Action::Search, Action::Close],
    }
}

/// The problems of the `[hints]` table: a mode or an action that does
/// not exist.
pub(crate) fn warnings(hints: &BTreeMap<String, Vec<String>>) -> Vec<Warning> {
    let mut out = Vec::new();
    for (mode, names) in hints {
        if Mode::from_name(mode).is_none() {
            out.push(Warning::config(format!(
                "hints.{}: unknown mode",
                shown(mode)
            )));
            continue;
        }
        for name in names.iter().filter(|n| Action::from_name(n).is_none()) {
            out.push(Warning::config(format!(
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
                Span::new(written, Some(Slot::HintKey)),
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

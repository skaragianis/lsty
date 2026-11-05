//! The hint row: the keys of a mode and what they do.

use std::collections::BTreeMap;

use lsty_core::{
    config::Warning,
    errs::{quote, shown},
    keys::{Action, Keymap, Mode},
    theme::Slot,
};

use crate::{
    press::{Answer, Command, Refusal},
    text::{Span, width},
};

/// The actions of a mode's hints when `[hints]` says nothing.
pub(crate) fn defaults(mode: Mode) -> &'static [Action] {
    match mode {
        Mode::Normal => &[
            Action::ClearSearch,
            Action::ToggleComplete,
            Action::ToggleStart,
            Action::Wontdo,
            Action::InsertBelow,
            Action::EditAppend,
            Action::Delete,
            Action::Undo,
            Action::Search,
            Action::Help,
        ],
        Mode::Visual => &[
            Action::ToggleComplete,
            Action::ExitVisual,
            Action::ToggleStart,
            Action::Wontdo,
            Action::Delete,
            Action::Yank,
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
            Action::FocusNext,
            Action::Fold,
            Action::Unfold,
        ],
        Mode::Detail => &[
            Action::ToggleComplete,
            Action::OpenLink,
            Action::NextLink,
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

/// Whether a hint is dim: `answer` says its key would do nothing.
pub(crate) const fn dim(answer: Answer) -> bool {
    matches!(answer, Answer::Refused(_) | Answer::CoreRefuses(_))
}

/// The hints of `mode`, in order: the first key of each action, what `label`
/// calls it and whether it is dim. An action without a key has no hint.
pub(crate) fn hints<'a>(
    mode: Mode,
    configured: &BTreeMap<String, Vec<String>>,
    keymap: &'a Keymap,
    hidden: &[Action],
    label: impl Fn(Action) -> Option<(&'static str, bool)>,
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
            let (word, dim) = label(action)?;
            let slot = |lit| if dim { Slot::HintDim } else { lit };
            Some([
                Span::new(written, Some(slot(Slot::HintKey))),
                Span::new(" ", None),
                Span::new(word, Some(slot(Slot::HintDesc))),
            ])
        })
        .collect()
}

/// While the matcher has keys or a count to drop, `<Esc>` and `<C-c>` drop
/// them whatever they are bound to, so their hints say so.
///
/// With a count (`room` holds the cells left), one hint says it, first. It
/// gives way when the next hint would not fit behind it, and a line with no
/// hints stays without. With only pending keys, each hint changes its word
/// where it stands.
pub(crate) fn cancelling<'a>(
    shown: &mut Vec<[Span<'a>; 3]>,
    room: Option<usize>,
) {
    let drops = |[key, ..]: &[Span<'a>; 3]| {
        ["<Esc>", "<C-c>"]
            .iter()
            .any(|name| key.text.eq_ignore_ascii_case(name))
    };
    let says = |key: &Span<'a>| {
        [
            Span::new(key.text.clone(), Some(Slot::HintKey)),
            Span::new(" ", None),
            Span::new("cancel", Some(Slot::HintDesc)),
        ]
    };
    let Some(room) = room else {
        for hint in shown.iter_mut().filter(|hint| drops(hint)) {
            let [key, ..] = &*hint;
            *hint = says(key);
        }
        return;
    };
    if shown.is_empty() {
        return;
    }
    let esc = [Span::new("<Esc>", None)];
    let key = shown.iter().find(|hint| drops(hint));
    let hint = says(key.map_or(&esc[0], |[key, ..]| key));
    shown.retain(|hint| !drops(hint));
    let next = shown.first().map_or(0, |next| 2 + width(next));
    if 2 + width(&hint) + next <= room {
        shown.insert(0, hint);
    }
}

/// What the hint of `action` says in `mode`: `accept` for a picker's
/// choice and the Enter of the text line, and for the task keys the command
/// `answer` resolves to. `None` when the key does nothing there: `t` on a
/// done task.
pub(crate) fn label(
    mode: Mode,
    action: Action,
    accept: &'static str,
    answer: Answer,
) -> Option<&'static str> {
    Some(match (mode, action, answer) {
        (Mode::Picker, Action::Accept, _)
        | (Mode::Command, Action::Execute, _) => accept,
        (
            _,
            Action::ToggleStart,
            Answer::Refused(Refusal::AllDone | Refusal::Closed),
        ) => return None,
        (_, Action::Wontdo, Answer::Resolved(Command::Reopen)) => "will",
        (_, _, Answer::Resolved(Command::Reopen))
        | (_, Action::ToggleComplete, Answer::Refused(Refusal::Closed)) => {
            "reopen"
        }
        (_, _, Answer::Resolved(Command::Stop)) => "stop",
        _ => action.hint(),
    })
}

//! The texts the writes of the list show, besides those of the
//! operations.

use lsty_core::{errs, list::State, todotxt::Line};

/// How the message in the status line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// What happened, or a warning.
    Notice,
    /// An error: nothing was done.
    Alert,
}

/// What the status line says when every task of a prompt is gone.
pub(crate) const GONE: &str = "the task is gone";

/// What the status line says when the task a draft gives an id to is
/// gone.
pub(crate) const PREREQUISITE_GONE: &str = "the task it waits for is gone";

/// What it ends with when that task is gone.
pub(crate) const DELETE_DEP: &str = " — delete its dep: to go on";

/// The states in the order a message names them.
const ORDER: [State; 3] = [State::Todo, State::Doing, State::Done];

/// What the status line says when the lock is held by someone else.
pub(crate) fn busy(name: &str) -> String {
    format!(
        "{name} is busy (locked by another writer) \
         — nothing changed, try again"
    )
}

/// The refusal of tasks that are not all in one state, or `None` when
/// they are.
pub(crate) fn mixed_states(states: &[State]) -> Option<String> {
    let parts: Vec<String> = ORDER
        .into_iter()
        .filter_map(|state| {
            let n = states.iter().filter(|s| **s == state).count();
            (n > 0).then(|| format!("{n} {}", state.as_str()))
        })
        .collect();
    (parts.len() > 1).then(|| {
        format!("mixed states ({}) — nothing changed", parts.join(", "))
    })
}

/// The refusal of a selection of which some tasks are archived.
pub(crate) const ARCHIVED_TASKS: &str =
    "the selection has archived tasks — nothing changed";

/// How a message names a task: its text, or its line as written when
/// it has no text, only tags.
pub(crate) fn name_of(line: &Line) -> String {
    let text = errs::lossy(&line.text());
    if text.is_empty() {
        errs::lossy(line.raw())
    } else {
        text
    }
}

/// The refusal of the one archived task named `name`.
pub(crate) fn archived_task(name: &str) -> String {
    format!("{name} is archived — x restores it")
}

/// The refusal of a new task that waits for the archived task named
/// `name`, which has no id.
pub(crate) fn archived_no_id(name: &str) -> String {
    format!("{name} is archived and has no id — x restores it first")
}

/// What the status line says after the human's own action made a task
/// stop matching the view.
pub(crate) const STICKY: &str =
    "hidden by current filters in 2 seconds or at the next key";

/// What the status line says when there is nothing to paste.
pub(crate) const NOTHING_TO_PASTE: &str = "nothing to paste";

/// What the status line adds when the clipboard could not be read and
/// the last yank was pasted.
pub(crate) const UNREADABLE: &str =
    "clipboard not readable \u{2014} pasted the last yank";

/// What the status line says after a yank of `n` tasks.
pub(crate) fn yanked(n: usize) -> String {
    format!("yanked {n} task{}", if n == 1 { "" } else { "s" })
}

/// The refusal to move a task in a view that is not in file order.
pub(crate) fn sorted(field: &str) -> String {
    format!("sorted by {field} — press s for manual order")
}

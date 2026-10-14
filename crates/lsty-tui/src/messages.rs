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

/// What the status line says when every item of a prompt is gone.
pub(crate) const GONE: &str = "the item is gone";

/// What the status line says when the todo a draft gives an id to is
/// gone.
pub(crate) const PREREQUISITE_GONE: &str = "the todo it waits for is gone";

/// What it ends with when that todo is gone or changed.
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

/// The refusal of items that are not all in one state, or `None` when
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

/// The refusal of a selection of which some items are archived.
pub(crate) const ARCHIVED_ITEMS: &str =
    "the selection has archived items — nothing changed";

/// How a message names an item: its text, or its line as written when
/// it has no text, only labels and keys.
pub(crate) fn name_of(line: &Line) -> String {
    let text = errs::lossy(&line.text());
    if text.is_empty() {
        errs::lossy(line.raw())
    } else {
        text
    }
}

/// The refusal of the one archived item named `name`.
pub(crate) fn archived_item(name: &str) -> String {
    format!("{name} is archived — x restores it")
}

/// The refusal of a new item that waits for the archived item named
/// `name`, which has no id.
pub(crate) fn archived_no_id(name: &str) -> String {
    format!("{name} is archived and has no id — x restores it first")
}

/// What the status line says after the human's own action made an item
/// stop matching the view.
pub(crate) const STICKY: &str =
    "hidden by current filters in 2 seconds or at the next key";

/// What the status line says when there is nothing to paste.
pub(crate) const NOTHING_TO_PASTE: &str = "nothing to paste";

/// What the status line adds when the clipboard could not be read and
/// the last yank was pasted.
pub(crate) const UNREADABLE: &str =
    "clipboard not readable \u{2014} pasted the last yank";

/// What the status line says after a yank of `n` items.
pub(crate) fn yanked(n: usize) -> String {
    format!("yanked {n} item{}", if n == 1 { "" } else { "s" })
}

/// The refusal to move an item in a view that is not in file order.
pub(crate) fn sorted(field: &str) -> String {
    format!("sorted by {field} — press s for manual order")
}

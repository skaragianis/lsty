//! The texts the writes of the list show, besides those of the
//! operations.

use lsty_core::list::State;

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

/// The refusal of items of which some are archived; `text` is the text
/// of the item when there is just one.
pub(crate) fn archived(text: Option<&str>) -> String {
    text.map_or_else(
        || "the selection has archived items — nothing changed".to_owned(),
        |text| format!("{text} is archived — x restores it"),
    )
}

/// What the status line says after the human's own action made an item
/// stop matching the view.
pub(crate) const STICKY: &str = "hidden by current filters once you move";

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

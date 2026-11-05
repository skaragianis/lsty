//! The texts the writes of the list show, besides those of the
//! operations.

use lsty_core::{errs, ops, todotxt::Line};

/// How the message in the status line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// What happened, or a warning.
    Notice,
    /// An error: nothing was done.
    Alert,
}

/// What the status line says when the task of a line being edited is gone.
pub(crate) const GONE: &str = "the task is gone";

/// What a field says when a commit on a line that changed is refused.
pub(crate) const REFUSED: &str =
    "the list changed — look again; <Esc> drops your edit";

/// The same, when what refuses is a `dep:` in the text.
pub(crate) const DEP_REFUSED: &str =
    "the list changed — <Esc> drops it, or delete the dep:";

/// The message of `e`, followed by its hint when it has one.
pub(crate) fn hinted(e: &errs::Error) -> String {
    e.hint().map_or_else(
        || e.message().to_owned(),
        |hint| format!("{} — {hint}", e.message()),
    )
}

/// What the status line says when the lock is held by someone else.
pub(crate) fn busy(name: &str) -> String {
    format!(
        "{name} is busy (locked by another writer) \
         — nothing changed, try again"
    )
}

/// The refusal of `t` over todo and done tasks.
pub(crate) const SOME_DONE: &str = "some are done — nothing changed";

/// The refusal of a press over several tasks of which some are archived.
pub(crate) const SOME_ARCHIVED: &str = "some are archived — nothing changed";

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
    format!("{name} is archived — x reopens it")
}

/// The refusal of `t` and `T` on the one done task named `name`, won't do or
/// not.
pub(crate) fn closed_task(name: &str) -> String {
    format!("{name} is done — x reopens it")
}

pub(crate) const CLOSED_TASKS: &str = "all are done — x reopens them";

/// What a second `t` says when it puts back what the first one stopped.
pub(crate) const PUT_BACK: &str = "put back as it was";

/// The refusal of a new task that waits for the archived task named
/// `name`, which has no id.
pub(crate) fn archived_no_id(name: &str) -> String {
    format!("{name} is archived and has no id — x reopens it first")
}

/// What the status line says after the human's own action made a task
/// stop matching the view.
pub(crate) const STICKY: &str =
    "hidden by current filters in 2 seconds or at the next key";

/// The same, when `keep_adding` has opened the next field under the task.
pub(crate) const STICKY_IN_FIELD: &str = "hidden when this field closes";

/// What the status line says about a task without links.
pub(crate) const NO_LINK: &str = "no link on this task";

/// What the status line says with no yank and nothing in the clipboard.
pub(crate) const NO_PASTE: &str = "nothing to paste";

/// What the status line adds when the clipboard could not be read and
/// the last yank was pasted.
pub(crate) const UNREADABLE: &str =
    "clipboard not readable \u{2014} pasted the last yank";

/// What the status line says after a yank of `n` tasks.
pub(crate) fn yanked(n: usize) -> String {
    format!("yanked {}", ops::counted(n, "task"))
}

/// The refusal to move a task in a view that is not in file order.
pub(crate) fn sorted(field: &str) -> String {
    format!("sorted by {field} — press s for manual order")
}

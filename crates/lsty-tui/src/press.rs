//! What a key would do over the tasks it is about: one rule for every key
//! that writes, asked by the key itself and by its hint. The core keeps its
//! own checks, for the CLI and for files changed on disk.

use lsty_core::{
    errs::Source,
    keys::Action,
    list::{Snapshot, State},
};

use crate::{model::Model, rows::Row};

/// The `lsty` command a key resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Do,
    Wontdo,
    Start,
    Stop,
    Reopen,
    Restore,
    Delete,
    /// `cycle_priority` and its way back: `pri`, each task from its own.
    Step {
        forward: bool,
    },
}

impl Command {
    /// How the short form of a press that passed tasks over says what it
    /// did and what it passed over, for the commands that can.
    pub(crate) const fn words(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Do => Some(("done", "already done")),
            Self::Wontdo => Some(("won't do", "already done")),
            Self::Start => Some(("started", "already underway")),
            Self::Stop => Some(("stopped", "not underway")),
            Self::Reopen
            | Self::Restore
            | Self::Delete
            | Self::Step { .. } => None,
        }
    }
}

/// Why a press writes nothing, when the key says so itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// No task is open, and some are archived.
    Closed,
    /// Some of the tasks are archived, and some open.
    Archived,
    /// `t` on tasks that are all done.
    AllDone,
    /// `t` on todo and done tasks.
    SomeDone,
    /// `T` on done tasks of which some are not won't do.
    NotWontdo,
}

/// What a press would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// Nothing to decide: no task, a key that does not write tasks, or a
    /// prompt that may open.
    Applies,
    /// The command runs over the tasks it changes, at least one, and passes
    /// over the rest.
    Resolved(Command),
    /// The press is refused, by the key.
    Refused(Refusal),
    /// The command runs over all the tasks and says why it writes nothing:
    /// it names the blockers or the task in its message.
    CoreRefuses(Command),
}

/// What the rule reads of the tasks of a press.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Summary {
    total: usize,
    archived: usize,
    todo: usize,
    doing: usize,
    done: usize,
    blocked_todo: usize,
    blocked_doing: usize,
    /// A done task that is not won't do, read only while no task is open.
    plain: bool,
}

impl Summary {
    /// One pass over the tasks of `rows`, reading fields only.
    pub(crate) fn of(snap: &Snapshot, rows: &[Row]) -> Self {
        let mut s = Self::default();
        for task in rows.iter().filter_map(|row| snap.task(row.id)) {
            s.total += 1;
            let blocked = usize::from(task.blocked());
            match task.state() {
                _ if task.source() == Source::Done => s.archived += 1,
                State::Todo => {
                    s.todo += 1;
                    s.blocked_todo += blocked;
                }
                State::Doing => {
                    s.doing += 1;
                    s.blocked_doing += blocked;
                }
                State::Done => {
                    s.done += 1;
                    s.plain = s.plain
                        || (s.todo + s.doing == 0 && !snap.is_wontdo(task));
                }
            }
        }
        s
    }
}

impl Model {
    /// What the tasks of a press with `count` are.
    pub(crate) fn summary(&self, count: usize) -> Summary {
        Summary::of(&self.snap, self.rows_of(count))
    }
}

/// `x`: any open task, `lsty do`, which refuses a blocked one; all done,
/// `lsty reopen`.
const fn complete(s: &Summary) -> Answer {
    if s.todo + s.doing == 0 {
        Answer::Resolved(Command::Reopen)
    } else if s.blocked_todo + s.blocked_doing > 0 {
        Answer::CoreRefuses(Command::Do)
    } else {
        Answer::Resolved(Command::Do)
    }
}

/// `t`: any todo task, `lsty start`, which refuses a done or blocked one
/// and takes over an agent's claim; else any doing, `lsty stop`.
const fn toggle_start(s: &Summary) -> Answer {
    if s.todo == 0 && s.doing == 0 {
        Answer::Refused(Refusal::AllDone)
    } else if s.todo == 0 {
        Answer::Resolved(Command::Stop)
    } else if s.done > 0 {
        Answer::Refused(Refusal::SomeDone)
    } else if s.blocked_todo > 0 {
        Answer::CoreRefuses(Command::Start)
    } else {
        Answer::Resolved(Command::Start)
    }
}

/// `stop`, bound by a user: `lsty stop`, which says so when none is underway.
const fn stop(s: &Summary) -> Answer {
    if s.doing == 0 {
        Answer::CoreRefuses(Command::Stop)
    } else {
        Answer::Resolved(Command::Stop)
    }
}

/// `T`: any open task, `lsty do --wontdo`; all won't do, `lsty reopen`.
const fn wontdo(s: &Summary) -> Answer {
    if s.todo + s.doing > 0 {
        Answer::Resolved(Command::Wontdo)
    } else if s.plain {
        Answer::Refused(Refusal::NotWontdo)
    } else {
        Answer::Resolved(Command::Reopen)
    }
}

/// `+` and `-`: a step is never already there, and `lsty pri` refuses a
/// done task.
const fn step(s: &Summary, forward: bool) -> Answer {
    let command = Command::Step { forward };
    if s.done > 0 {
        Answer::CoreRefuses(command)
    } else {
        Answer::Resolved(command)
    }
}

/// What a press of `action` over the tasks of `s` would do. Every action is
/// named, so a new one does not compile until it is decided. An archived
/// task refuses every key that writes, except `x` on archived tasks alone.
#[expect(clippy::too_many_lines, reason = "every action is listed")]
pub(crate) fn decide(action: Action, s: &Summary) -> Answer {
    let rule: fn(&Summary) -> Answer = match action {
        Action::ToggleComplete => complete,
        Action::ToggleStart => toggle_start,
        Action::Stop => stop,
        Action::Wontdo => wontdo,
        Action::Delete => |_| Answer::Resolved(Command::Delete),
        Action::CyclePriority => |s| step(s, true),
        Action::CyclePriorityBack => |s| step(s, false),
        Action::SetDue | Action::AddTag | Action::RemoveTag => {
            |_| Answer::Applies
        }
        // These do not write tasks; for now they always apply.
        Action::CursorDown
        | Action::CursorUp
        | Action::GoTop
        | Action::GoBottom
        | Action::HalfPageDown
        | Action::HalfPageUp
        | Action::PageDown
        | Action::PageUp
        | Action::ScrollDown
        | Action::ScrollUp
        | Action::CentreView
        | Action::TopView
        | Action::BottomView
        | Action::GoViewTop
        | Action::GoViewMiddle
        | Action::GoViewBottom
        | Action::EditDescription
        | Action::EditAppend
        | Action::EditLineStart
        | Action::ChangeDescription
        | Action::InsertBelow
        | Action::InsertAbove
        | Action::InsertDependent
        | Action::InsertPrerequisite
        | Action::Yank
        | Action::PasteBelow
        | Action::PasteAbove
        | Action::MoveTaskDown
        | Action::MoveTaskUp
        | Action::OpenLink
        | Action::Undo
        | Action::Redo
        | Action::Repeat
        | Action::Search
        | Action::ClearSearch
        | Action::ClearFilters
        | Action::CycleSort
        | Action::ToggleSidebar
        | Action::ToggleDetail
        | Action::ToggleWrap
        | Action::EnterVisual
        | Action::ExitVisual
        | Action::FocusLeft
        | Action::FocusDown
        | Action::FocusUp
        | Action::FocusRight
        | Action::FocusNext
        | Action::FocusPrev
        | Action::FocusList
        | Action::FocusDetail
        | Action::FocusProjects
        | Action::FocusContexts
        | Action::FocusFilters
        | Action::CommandLine
        | Action::Help
        | Action::Quit
        | Action::Commit
        | Action::Cancel
        | Action::CursorRowUp
        | Action::CursorRowDown
        | Action::CursorLeft
        | Action::CursorRight
        | Action::WordBack
        | Action::WordForward
        | Action::LineStart
        | Action::LineEnd
        | Action::DeleteChar
        | Action::DeleteWordBack
        | Action::DeleteWordForward
        | Action::DeleteToStart
        | Action::DeleteToEnd
        | Action::Accept
        | Action::Close
        | Action::NextLink
        | Action::PrevLink
        | Action::Next
        | Action::Prev
        | Action::Execute
        | Action::CompleteNext
        | Action::CompletePrev
        | Action::HistoryPrev
        | Action::HistoryNext
        | Action::ToggleEntry
        | Action::SelectAndJump
        | Action::Fold
        | Action::Unfold => return Answer::Applies,
    };
    if s.total == 0 {
        Answer::Applies
    } else if s.archived == 0 {
        rule(s)
    } else if s.todo + s.doing > 0 {
        Answer::Refused(Refusal::Archived)
    } else if s.archived == s.total && matches!(action, Action::ToggleComplete)
    {
        Answer::Resolved(Command::Restore)
    } else {
        Answer::Refused(Refusal::Closed)
    }
}

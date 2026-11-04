//! What a key would do over the tasks it is about: one rule for every key
//! that writes tasks or opens a field on them, and for those with nothing to
//! do in some state, asked by the key itself and by its hint. The core keeps
//! its own checks, for the CLI and for files changed on disk.

use lsty_core::{
    errs::Source,
    keys::{Action, Matcher},
    list::{Snapshot, State},
};

use crate::{focus::Focus, history::Again, messages, model::Model, rows::Row};

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
    /// Some of the tasks are archived, and some open; or any, for `J` and `K`.
    Archived,
    /// `t` on tasks that are all done.
    AllDone,
    /// `t` on todo and done tasks.
    SomeDone,
    /// `T` on done tasks of which some are not won't do.
    NotWontdo,
    /// The key has nothing to work on, and says nothing.
    Quiet,
    /// `J` and `K` in a view that is not in file order.
    Sorted,
    /// `go` on an archived task without an id.
    NoId,
    /// The key has nothing to work on, and says this.
    Says(&'static str),
}

/// What a press would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The key does its work, or says why not: a key that does not write
    /// tasks, or a prompt that may open.
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
    /// The row of the first task.
    lo: usize,
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
    /// One pass over the tasks of `rows`, the first at row `lo`, reading
    /// fields only.
    pub(crate) fn of(snap: &Snapshot, lo: usize, rows: &[Row]) -> Self {
        let mut s = Self {
            lo,
            ..Self::default()
        };
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
    /// What a press with `count` reads.
    pub(crate) fn summary(&self, count: usize) -> Summary {
        let snap = &self.snap;
        match self.selected_rows() {
            Some((lo, hi)) => Summary::of(snap, lo, self.rows_between(lo, hi)),
            None => Summary::of(snap, self.cursor, self.picked_rows(count)),
        }
    }

    /// The count typed so far; 0 when none, or where a digit is no count.
    pub(crate) fn typed_count(&self) -> usize {
        self.matchers
            .get(&self.key_mode())
            .map_or(0, Matcher::count)
    }

    /// What the hints ask of the rule, with the count typed so far. A second
    /// summary only with a count, for the keys that do not read it.
    pub(crate) fn answers(&self) -> impl Fn(Action) -> Answer + '_ {
        let count = self.typed_count();
        let typed = self.summary(count);
        let one = (count > 1).then(|| self.summary(1));
        move |action| {
            let cursor = one.as_ref().filter(|_| !action.reads_count());
            decide(action, self, cursor.unwrap_or(&typed))
        }
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

/// `t` in the window after it stopped these tasks: it puts their claims back,
/// which a blocked task does not refuse.
fn puts_back(m: &Model, s: &Summary) -> bool {
    s.blocked_todo > 0
        && m.stopped_over(m.rows_between(s.lo, s.lo + s.total - 1))
            == Some(true)
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

/// `J` and `K`: a task to move, the file's order, nothing archived, and a
/// todo row to move past, at row `past`.
fn moving(m: &Model, s: &Summary, past: Option<usize>) -> Answer {
    let past = past.and_then(|at| m.rows.get(at));
    if s.total == 0 {
        Answer::Refused(Refusal::Quiet)
    } else if m.sorted() {
        Answer::Refused(Refusal::Sorted)
    } else if s.archived > 0 {
        Answer::Refused(Refusal::Archived)
    } else {
        let todo_row = past.is_some_and(|row| row.source != Source::Done);
        lit_if(todo_row, Refusal::Quiet)
    }
}

/// A key that does its work, or says `why` not.
const fn lit_if(lit: bool, why: Refusal) -> Answer {
    if lit {
        Answer::Applies
    } else {
        Answer::Refused(why)
    }
}

/// `.`: as the key it repeats over the tasks it stored or the count typed over
/// them, whose word it does not take. `J` and `K` take a distance, not tasks.
/// Nothing to repeat says nothing.
fn repeat(m: &Model) -> Answer {
    let Some(again) = &m.again else {
        return Answer::Refused(Refusal::Quiet);
    };
    let count = match again {
        Again::Task { action, .. } if !action.reads_count() => 1,
        Again::Task { count, .. }
        | Again::Block { count, .. }
        | Again::Prompt { count, .. } => {
            let typed = m.typed_count();
            if typed > 0 { typed } else { *count }
        }
    };
    match decide(again.action(), m, &m.summary(count)) {
        Answer::Resolved(_) => Answer::Applies,
        answer @ (Answer::Applies
        | Answer::Refused(_)
        | Answer::CoreRefuses(_)) => answer,
    }
}

/// What a press of `action` over the tasks of `s` would do. Every action is
/// named, so a new one does not compile until it is decided. An archived
/// task refuses every key that writes or opens a field on it, except `x` on
/// archived tasks alone and `go` on one with an id.
#[expect(clippy::too_many_lines, reason = "every action is listed")]
pub(crate) fn decide(action: Action, m: &Model, s: &Summary) -> Answer {
    let rule: fn(&Summary) -> Answer = match action {
        // Nothing to move over, select or show beside the list.
        action
            if s.total == 0
                && m.focus == Focus::List
                && (action.is_motion()
                    || matches!(
                        action,
                        Action::EnterVisual
                            | Action::FocusDetail
                            | Action::FocusDown
                            | Action::FocusUp
                            | Action::FocusRight
                            | Action::FocusList
                    )) =>
        {
            return Answer::Refused(Refusal::Quiet);
        }
        Action::ToggleStart if puts_back(m, s) => {
            return Answer::Resolved(Command::Start);
        }
        Action::ToggleComplete => complete,
        Action::ToggleStart => toggle_start,
        Action::Stop => stop,
        Action::Wontdo => wontdo,
        Action::Delete => |_| Answer::Resolved(Command::Delete),
        Action::CyclePriority => |s| step(s, true),
        Action::CyclePriorityBack => |s| step(s, false),
        Action::SetDue
        | Action::AddTag
        | Action::RemoveTag
        | Action::EditDescription
        | Action::EditAppend
        | Action::EditLineStart
        | Action::ChangeDescription
        | Action::InsertPrerequisite => |_| Answer::Applies,
        Action::InsertDependent => {
            let task = m.rows.get(m.cursor).and_then(|r| m.snap.task(r.id));
            let no_id = task.is_some_and(|task| {
                task.source() == Source::Done && m.snap.id_of(task).is_none()
            });
            return lit_if(!no_id, Refusal::NoId);
        }
        Action::Yank => return lit_if(s.total > 0, Refusal::Quiet),
        Action::MoveTaskDown => return moving(m, s, Some(s.lo + s.total)),
        Action::MoveTaskUp => return moving(m, s, s.lo.checked_sub(1)),
        Action::OpenLink if s.total == 0 => {
            return Answer::Refused(Refusal::Quiet);
        }
        Action::OpenLink => {
            let links = m.cursor_links().len();
            // The panel opens the link it has selected.
            let wanted = if m.focus == Focus::Detail {
                1
            } else {
                m.typed_count()
            };
            let lit = links >= wanted.max(1);
            return lit_if(lit, Refusal::Says(messages::NO_LINK));
        }
        Action::Undo => {
            let lit = m.kept.is_some() || !m.hist.undo.is_empty();
            return lit_if(lit, Refusal::Says("nothing to undo"));
        }
        Action::Redo => {
            let lit = !m.hist.redo.is_empty();
            return lit_if(lit, Refusal::Says("nothing to redo"));
        }
        Action::Repeat => return repeat(m),
        // These always apply.
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
        | Action::InsertBelow
        | Action::InsertAbove
        // What is pasted is known only by running the clipboard's program.
        | Action::PasteBelow
        | Action::PasteAbove
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
        lit_if(action == Action::InsertPrerequisite, Refusal::Quiet)
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

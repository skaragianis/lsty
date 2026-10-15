use std::{
    ops::Range,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use jiff::Timestamp;

use super::ids::{self, Warned};
use crate::{
    config::Config,
    dates::{Clock, format_elapsed},
    discover::Found,
    errs::{Nr, Ref, Source, lossy},
    todotxt::{File, Line, Tags},
};

/// Where a task stands in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Open and not underway.
    Todo,
    /// Open and underway: a `started:` token is present.
    Doing,
    /// Done.
    Done,
}

impl State {
    /// The state as written in output: `todo`, `doing` or `done`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Done => "done",
        }
    }
}

/// Who is working on a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// Nobody: the task is not underway.
    None,
    /// Underway without an `owner:`.
    Human,
    /// Underway with an `owner:`.
    Agent,
}

/// The position of a task in [`Snapshot::tasks`]. It is only meaningful
/// for the snapshot that gave it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId {
    index: usize,
    serial: u32,
}

impl TaskId {
    pub(super) const fn new(index: usize, serial: u32) -> Self {
        Self { index, serial }
    }

    /// The index into [`Snapshot::tasks`].
    pub const fn index(self) -> usize {
        self.index
    }
}

/// The serial the next snapshot gets. Every snapshot of the process has
/// its own, so a task used on the wrong snapshot is caught in debug
/// builds.
pub(super) fn next_serial() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A non-blank line with the state derived from the whole list.
///
/// A task keeps no borrow of its line: [`Snapshot::line`] gives the
/// line back, and its tags come from the line. Its fields are read
/// through methods, so what it says always agrees with the list it came
/// from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the flags are independent facts that readers filter on"
)]
pub struct Task {
    pub(super) nr: Nr,
    pub(super) source: Source,
    pub(super) state: State,
    pub(super) claim: Claim,
    pub(super) blockers: Vec<TaskId>,
    pub(super) dangling: Vec<Range<usize>>,
    pub(super) cycle: bool,
    pub(super) later: bool,
    pub(crate) overdue: bool,
    pub(super) stale: bool,
    pub(super) key: TaskId,
    pub(super) id: Option<Range<usize>>,
    pub(super) started: Option<Timestamp>,
    pub(super) ended: Option<Timestamp>,
}

impl Task {
    /// The line number in its file.
    pub const fn nr(&self) -> Nr {
        self.nr
    }

    /// The file the task lives in.
    pub const fn source(&self) -> Source {
        self.source
    }

    /// How a reference names the task by its line: `12`, or `done:12`
    /// in the done file.
    pub fn handle(&self) -> String {
        self.source.numbered(self.nr)
    }

    /// Todo, doing or done.
    pub const fn state(&self) -> State {
        self.state
    }

    /// Who is working on it.
    pub const fn claim(&self) -> Claim {
        self.claim
    }

    /// How `show` and the detail panel word the claim: `finished by X`,
    /// `finished (by you)`, `claimed (by you)` or `claimed by X`. `owner`
    /// is the line's `owner:` value.
    pub fn claim_text(&self, owner: Option<&[u8]>) -> Option<Vec<u8>> {
        match (self.state == State::Done, self.claim, owner) {
            (true, _, Some(owner)) => Some([b"finished by ", owner].concat()),
            (true, Claim::Human, None) => Some(b"finished (by you)".to_vec()),
            (false, Claim::Human, _) => Some(b"claimed (by you)".to_vec()),
            (false, Claim::Agent, Some(owner)) => {
                Some([b"claimed by ", owner].concat())
            }
            _ => None,
        }
    }

    /// This task's position in [`Snapshot::tasks`], valid only for the
    /// snapshot that gave the task out.
    pub const fn task_id(&self) -> TaskId {
        self.key
    }

    /// Whether the task is not done.
    pub fn is_open(&self) -> bool {
        self.state != State::Done
    }

    /// Waiting on an open prerequisite or part of a cycle; a done task has
    /// neither.
    pub fn blocked(&self) -> bool {
        self.cycle || !self.blockers.is_empty()
    }

    /// Whether a `dep:` names an id found in neither file.
    pub(crate) fn has_dangling(&self) -> bool {
        !self.dangling.is_empty()
    }

    /// The open prerequisites, once each, in the order of the `dep:`
    /// tokens. Empty for a done task.
    pub fn blockers(&self) -> &[TaskId] {
        &self.blockers
    }

    /// Part of a dependency cycle.
    pub const fn cycle(&self) -> bool {
        self.cycle
    }

    /// The `t:` value as `show` and the detail panel print it, with the
    /// mark of a task that starts later.
    pub fn threshold_text(&self, threshold: &str) -> String {
        if self.later {
            format!("{threshold} (later)")
        } else {
            threshold.to_owned()
        }
    }

    /// The `started:` instant in the clock's zone, as `2026-10-01 09:30`,
    /// and for a task that is underway the time it has taken so far, in
    /// parentheses, with `, stale` inside them when `mark_stale` is set
    /// and the task is.
    pub fn started_text(
        &self,
        clock: &Clock,
        mark_stale: bool,
    ) -> Option<String> {
        let when = local_text(self.started?, clock);
        let stale = if mark_stale && self.stale {
            ", stale"
        } else {
            ""
        };
        Some(match self.elapsed(clock.now()) {
            Some(took) if self.state == State::Doing => {
                format!("{when} ({}{stale})", format_elapsed(took))
            }
            _ => when,
        })
    }

    /// The `ended:` instant in the clock's zone, as `2026-10-01 09:30`,
    /// and for a done task with a `started:` the time it took, in
    /// parentheses.
    pub fn ended_text(&self, clock: &Clock) -> Option<String> {
        let when = local_text(self.ended?, clock);
        Some(match self.elapsed(clock.now()) {
            Some(took) if self.state == State::Done => {
                format!("{when} ({})", format_elapsed(took))
            }
            _ => when,
        })
    }

    /// Has a valid `t:` after today, whatever its state.
    pub const fn later(&self) -> bool {
        self.later
    }

    /// Open, claimed by an agent for more than the stale limit.
    pub const fn stale(&self) -> bool {
        self.stale
    }

    /// The instant of the first `started:` token, when it is valid.
    pub const fn started(&self) -> Option<Timestamp> {
        self.started
    }

    /// The instant of the first `ended:` token, when it is valid.
    pub const fn ended(&self) -> Option<Timestamp> {
        self.ended
    }

    /// The time a task with a valid `started:` has been underway (now
    /// minus it) or, when done, took (`ended:` minus it); `None` for any
    /// other task. A later `started:` than the end gives zero.
    pub fn elapsed(&self, now: Timestamp) -> Option<Duration> {
        let end = match self.state {
            State::Doing => now,
            State::Done => self.ended?,
            State::Todo => return None,
        };
        let since = end.duration_since(self.started?);
        Some(Duration::try_from(since).unwrap_or(Duration::ZERO))
    }
}

/// `at` in the clock's zone, as `2026-10-01 09:30`.
fn local_text(at: Timestamp, clock: &Clock) -> String {
    at.to_zoned(clock.zone().clone())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

/// What [`Snapshot::build`] needs besides the files.
#[derive(Clone, Debug)]
pub struct Params {
    /// The reading of time for `later`, `overdue` and `stale`.
    pub clock: Clock,
    /// How long an agent may hold a claim before it is stale.
    pub stale_after: Duration,
}

impl Params {
    /// The parameters for reading time from `clock` with the claim
    /// timeout of `config`.
    #[must_use]
    pub fn new(clock: Clock, config: &Config) -> Self {
        Self {
            clock,
            stale_after: config.claim_stale_after,
        }
    }
}

/// Both files as read at one moment, with derived state.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub(super) serial: u32,
    pub(super) found: Found,
    pub(super) todo: File,
    pub(super) done: File,
    pub(super) tasks: Vec<Task>,
    pub(super) todo_tasks: usize,
    pub(super) by_id: Vec<usize>,
    /// The values of second and later `id:` tags on a line, sorted.
    pub(super) extra_ids: Vec<Vec<u8>>,
    pub(super) warned: Vec<Warned>,
}

/// The line `line` falls back to; no task of a snapshot points at it.
fn blank() -> &'static Line {
    static BLANK: OnceLock<Line> = OnceLock::new();
    BLANK.get_or_init(|| Line::parse([]))
}

impl Snapshot {
    /// The number that tells this snapshot from every other one of the
    /// process.
    pub const fn serial(&self) -> u32 {
        self.serial
    }

    /// The files the snapshot was read from.
    pub const fn found(&self) -> &Found {
        &self.found
    }

    /// The todo file.
    pub const fn todo(&self) -> &File {
        &self.todo
    }

    /// The done file.
    pub const fn done(&self) -> &File {
        &self.done
    }

    /// The tasks of the todo file in file order, then those of the done
    /// file.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Ids found in both files or on several lines of one.
    #[cfg(test)]
    pub(super) fn warnings(&self) -> impl ExactSizeIterator<Item = &str> {
        self.warned.iter().map(Warned::text)
    }

    /// Takes the snapshot apart, so a rebuild can reuse the files.
    pub fn into_files(self) -> (Found, File, File) {
        (self.found, self.todo, self.done)
    }

    /// The task behind `id`.
    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.check(id);
        self.tasks.get(id.index())
    }

    /// The task on line `nr` of `source`; `None` for a blank or missing
    /// line.
    pub fn at(&self, source: Source, nr: Nr) -> Option<&Task> {
        let tasks = self.in_source(source);
        let at = tasks.binary_search_by_key(&nr, |i| i.nr).ok()?;
        tasks.get(at)
    }

    /// The tasks of one file, in file order.
    pub fn in_source(&self, source: Source) -> &[Task] {
        let (todo, done) = self
            .tasks
            .split_at_checked(self.todo_tasks)
            .unwrap_or((&self.tasks, &[]));
        source.pick(todo, done)
    }

    /// The file of `source`.
    pub fn file(&self, source: Source) -> &File {
        source.pick(&self.todo, &self.done)
    }

    /// The line of `task`, which must come from this snapshot.
    pub fn line(&self, task: &Task) -> &Line {
        self.check(task.key);
        self.file(task.source)
            .line(task.nr)
            .unwrap_or_else(|| blank())
    }

    /// The reference to `task` that errors carry: its place, and its `id:`
    /// when it has one.
    pub fn ref_of(&self, task: &Task) -> Ref {
        Ref {
            nr: task.nr,
            source: task.source,
            id: self.id_of(task).map(lossy),
        }
    }

    /// The typed tags of `task`'s line.
    pub(crate) fn tags(&self, task: &Task) -> Tags<'_> {
        self.line(task).tags()
    }

    /// The `id:` of `task`, if it has one.
    pub fn id_of(&self, task: &Task) -> Option<&[u8]> {
        self.line(task).raw().get(task.id.clone()?)
    }

    /// The `dep:` ids of `task` that are found in neither file.
    pub fn dangling<'a>(
        &'a self,
        task: &'a Task,
    ) -> impl Iterator<Item = &'a [u8]> {
        let raw = self.line(task).raw();
        task.dangling.iter().filter_map(|r| raw.get(r.clone()))
    }

    /// Catches an id that belongs to another snapshot.
    fn check(&self, id: TaskId) {
        debug_assert_eq!(id.serial, self.serial, "task of another snapshot");
    }

    /// The id of the task at `index`, which must be in range.
    #[cfg(test)]
    pub(super) const fn id_for(&self, index: usize) -> TaskId {
        TaskId::new(index, self.serial)
    }

    pub(super) fn id_at(&self, index: usize) -> &[u8] {
        self.tasks
            .get(index)
            .and_then(|task| self.id_of(task))
            .unwrap_or_default()
    }

    /// The warnings about the ids that `tasks` carry, in warning order.
    pub fn warnings_for<'a>(
        &self,
        tasks: impl IntoIterator<Item = &'a Task>,
    ) -> Vec<String> {
        let ids: Vec<&[u8]> =
            tasks.into_iter().filter_map(|i| self.id_of(i)).collect();
        ids::warnings_for(&self.warned, &ids)
    }
}

//! Rows: the tasks the list shows, with what its columns need.

use std::{cell::RefCell, rc::Rc};

use jiff::Timestamp;
use lsty_core::{
    config::{Config, Hidden},
    errs::{Nr, Source, lossy},
    links::link_in,
    list::{Snapshot, SortField, State, Task, TaskId},
    query::Category,
    theme::Slot,
    todotxt::{Kind, Line, Priority, TokenView, inside_span, is_claim_key},
};

use crate::{
    reldate::{Cell, Today, claim_cells, date_cell},
    sticky::{Sticky, WORD},
    text::{Span, width_of},
    track::{Place, place_of},
    view::Scope,
};

const MARKER_DONE: &str = "✓";
const MARKER_BLOCKED: &str = "⧗";
pub(crate) const MARKER_DOING: &str = "▶";

/// The tags that `show_ids` shows in the text.
const ID_KEYS: [&[u8]; 2] = [b"id", b"dep"];

/// The widths of the first and later rows, and the rows a text took
/// for them.
pub(crate) type Wrapped = ((usize, usize), Rc<Vec<Vec<Span<'static>>>>);

/// One task of the list.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub(crate) id: TaskId,
    pub(crate) nr: Nr,
    pub(crate) source: Source,
    /// The slot that styles the whole row, when there is one.
    pub(crate) base: Option<Slot>,
    pub(crate) marker: Span<'static>,
    /// The priority column of the row, if it has one.
    pub(crate) priority: Option<Span<'static>>,
    pub(crate) date: Cell,
    pub(crate) owner: Cell,
    /// The state and duration of the claim.
    pub(crate) took: Cell,
    /// The width of the task text, in cells.
    pub(crate) text_width: usize,
    /// The view does not match the task; it is shown until the cursor
    /// leaves it.
    pub(crate) sticky: bool,
    /// The rows the text takes with `wrap` on, once worked out.
    pub(crate) lines: RefCell<Option<Wrapped>>,
}

/// What a row takes from the clock and the config.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Context {
    pub(crate) today: Today,
    pub(crate) now: Timestamp,
    pub(crate) inline: Inline,
}

/// Which tags a row shows in its text.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Inline {
    ids: bool,
}

impl Inline {
    pub(crate) const fn new(config: &Config) -> Self {
        Self {
            ids: config.show_ids,
        }
    }

    /// Whether `tok` is a part of the task text: every token but the tags
    /// that the text leaves out.
    fn in_text(self, tok: &TokenView<'_>) -> bool {
        tok.kind != Kind::Tag
            || !(matches!(tok.key, b"pri" | b"due")
                || is_claim_key(tok.key)
                || (!self.ids && ID_KEYS.contains(&tok.key)))
    }
}

/// Which kinds of tasks the list shows.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch for each kind of item that can be hidden"
)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shows {
    later: bool,
    blocked: bool,
    done: bool,
    archive: bool,
}

impl Shows {
    pub(crate) fn new(config: &Config) -> Self {
        let on = |what| !config.hidden_items.contains(&what);
        Self {
            later: on(Hidden::Later),
            blocked: on(Hidden::Blocked),
            done: on(Hidden::Done),
            archive: on(Hidden::Archive),
        }
    }

    /// Whether the switch of `what` is on.
    pub(crate) const fn on(self, what: Hidden) -> bool {
        match what {
            Hidden::Later => self.later,
            Hidden::Blocked => self.blocked,
            Hidden::Done => self.done,
            Hidden::Archive => self.archive,
        }
    }

    /// Whether the archive switch, or a filter that asks for archived
    /// tasks (`archived`), lets `task` into the list.
    pub(crate) fn lists(self, task: &Task, archived: bool) -> bool {
        self.archive || archived || task.source() != Source::Done
    }

    /// Whether the switches let `task` into the view. A category in
    /// `asked` is shown whatever its switch says.
    pub(crate) fn holds(self, task: &Task, asked: &[Category]) -> bool {
        let on = |flag: bool, cat: Category, member: bool| {
            !member || flag || asked.contains(&cat)
        };
        self.lists(task, asked.contains(&Category::Archived))
            && on(self.done, Category::Done, task.state() == State::Done)
            && on(self.later, Category::Later, task.later())
            && on(self.blocked, Category::Blocked, task.blocked())
    }

    /// Switches `what` the other way.
    pub(crate) const fn flip(&mut self, what: Hidden) {
        let flag = match what {
            Hidden::Later => &mut self.later,
            Hidden::Blocked => &mut self.blocked,
            Hidden::Done => &mut self.done,
            Hidden::Archive => &mut self.archive,
        };
        *flag = !*flag;
    }
}

/// The rows of the tasks that `scope` lets in, and of the sticky ones,
/// in the order of `sort`.
pub(crate) fn build(
    snap: &Snapshot,
    scope: &Scope<'_>,
    ctx: Context,
    sort: SortField,
    sticky: &Sticky,
) -> Vec<Row> {
    let mut tasks: Vec<&Task> = snap.tasks().iter().collect();
    if sort == SortField::File {
        tasks = sticky.placed(tasks);
    } else {
        snap.sort(&mut tasks, sort);
    }
    let listed: Vec<_> = tasks
        .into_iter()
        .filter_map(|task| {
            if scope.holds(snap, task) {
                Some((task, false))
            } else {
                sticky.has(place_of(task)).then_some((task, true))
            }
        })
        .collect();
    listed
        .into_iter()
        .map(|(task, stuck)| {
            if stuck {
                Row::stuck(snap, task, ctx)
            } else {
                Row::new(snap, task, ctx)
            }
        })
        .collect()
}

impl Row {
    /// Where the row's line stands in its file.
    pub(crate) fn place(&self) -> Place {
        (self.source, self.nr)
    }

    fn new(snap: &Snapshot, task: &Task, ctx: Context) -> Self {
        let line = snap.line(task);
        let base = base_slot(task);
        let mut date = date_cell(task, line, ctx.today);
        if base.is_some() && !date.text.is_empty() {
            date.slot = base;
        }
        let owner = owner(line);
        let (owner, took) = claim_cells(task, owner.as_deref(), ctx.now);
        Self {
            id: task.task_id(),
            nr: task.nr(),
            source: task.source(),
            base,
            marker: marker(task, base),
            priority: priority_span(task, line, base),
            date,
            owner,
            took,
            text_width: tokens_width(line, ctx.inline),
            sticky: false,
            lines: RefCell::default(),
        }
    }

    /// The row of a task that the view does not match: muted, with the
    /// word in place of the date and claim.
    fn stuck(snap: &Snapshot, task: &Task, ctx: Context) -> Self {
        let base = Some(Slot::Muted);
        Self {
            base,
            marker: marker(task, base),
            priority: priority_span(task, snap.line(task), base),
            date: Cell {
                text: WORD.to_owned(),
                slot: base,
            },
            owner: Cell::default(),
            took: Cell::default(),
            sticky: true,
            lines: RefCell::default(),
            ..Self::new(snap, task, ctx)
        }
    }
}

fn owner(line: &Line) -> Option<String> {
    line.tokens()
        .find(|t| t.is_tag(b"owner"))
        .map(|t| lossy(t.value))
}

pub(crate) fn base_slot(task: &Task) -> Option<Slot> {
    if task.source() == Source::Done {
        Some(Slot::Muted)
    } else if task.state() == State::Done {
        Some(Slot::Done)
    } else if task.blocked() {
        Some(Slot::Blocked)
    } else {
        None
    }
}

fn marker(task: &Task, base: Option<Slot>) -> Span<'static> {
    if task.state() == State::Done {
        Span::new(MARKER_DONE, base)
    } else if task.blocked() {
        Span::new(MARKER_BLOCKED, Some(Slot::Blocked))
    } else if task.state() == State::Doing {
        Span::new(MARKER_DOING, Some(Slot::Doing))
    } else {
        Span::new(" ", None)
    }
}

/// The priority column: `(A)`, or nothing for no priority and for a
/// done task.
fn priority_span(
    task: &Task,
    line: &Line,
    base: Option<Slot>,
) -> Option<Span<'static>> {
    let pri = line.priority().filter(|_| task.state() != State::Done)?;
    let slot = base.unwrap_or_else(|| priority_slot(pri));
    let text = format!("({pri})");
    Some(Span::new(text, Some(slot)))
}

fn priority_slot(pri: Priority) -> Slot {
    match pri.letter() {
        b'A' => Slot::PriorityA,
        b'B' => Slot::PriorityB,
        b'C' => Slot::PriorityC,
        _ => Slot::PriorityOther,
    }
}

pub(crate) fn token_slot(tok: &TokenView<'_>, base: Option<Slot>) -> Slot {
    match (tok.kind, base) {
        (_, Some(slot)) => slot,
        (_, None) if link_in(tok).is_some() => Slot::Link,
        (Kind::Project, None) => Slot::Project,
        (Kind::Context, None) => Slot::Context,
        (Kind::Word, None) if tok.quoted => Slot::Code,
        (Kind::Word, None) => Slot::Text,
        (Kind::Tag, None) => Slot::Muted,
    }
}

/// The description without the tokens other columns show or that are
/// hidden, one space between tokens.
pub(crate) fn text_spans(
    line: &Line,
    base: Option<Slot>,
    inline: Inline,
) -> Vec<Span<'_>> {
    let mut out = Vec::new();
    let plain = base.or(Some(Slot::Text));
    for tok in line.tokens().filter(|t| inline.in_text(t)) {
        let slot = Some(token_slot(&tok, base));
        if !out.is_empty() {
            // only a blank inside a backtick span takes its style
            let within = tok.quoted && inside_span(line.raw(), tok.start);
            out.push(Span::new(" ", if within { slot } else { plain }));
        }
        out.push(Span::new(lossy(tok.text), slot));
    }
    out
}

/// The width of the text of a line, one space between tokens, without
/// building any spans.
fn tokens_width(line: &Line, inline: Inline) -> usize {
    let mut total = 0;
    let mut count = 0_usize;
    for tok in line.tokens().filter(|tok| inline.in_text(tok)) {
        total += width_of(&lossy(tok.text));
        count += 1;
    }
    total + count.saturating_sub(1)
}

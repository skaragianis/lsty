//! Rows: the items the list shows, with what its columns need.

use std::borrow::Cow;

use jiff::Timestamp;
use lsty_core::config::{Config, Hidden};
use lsty_core::errs::{Nr, Source};
use lsty_core::list::{Item, ItemId, Snapshot, SortField, State};
use lsty_core::query::Category;
use lsty_core::theme::Slot;
use lsty_core::todotxt::{Kind, Line, Priority, TokenView};

use crate::reldate::{Cell, Today, claim_cell, date_cell};
use crate::sticky::{Sticky, WORD};
use crate::text::{Span, width_of};
use crate::track::{Place, place_of};
use crate::view::Scope;

const MARKER_DONE: &str = "✓";
const MARKER_BLOCKED: &str = "⧗";
pub(crate) const MARKER_DOING: &str = "▶";

/// The tags that other columns show or that stay hidden.
const HIDDEN_KEYS: [&[u8]; 6] =
    [b"id", b"dep", b"started", b"owner", b"pri", b"due"];

/// One item of the list.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub(crate) id: ItemId,
    pub(crate) nr: Nr,
    pub(crate) source: Source,
    /// The slot that styles the whole row, when there is one.
    pub(crate) base: Option<Slot>,
    pub(crate) marker: Span<'static>,
    pub(crate) date: Cell,
    pub(crate) claim: Cell,
    /// The width of the tags column value, in cells.
    pub(crate) tags_width: usize,
    /// The view does not match the item; it is shown until the cursor
    /// leaves it.
    pub(crate) sticky: bool,
}

/// What a row takes from the clock and the config.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Context {
    pub(crate) today: Today,
    pub(crate) now: Timestamp,
    pub(crate) inline: Inline,
}

/// Which names a row shows in its text rather than in the tags column.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Inline {
    projects: bool,
    contexts: bool,
}

impl Inline {
    pub(crate) const fn new(config: &Config) -> Self {
        Self {
            projects: config.inline_projects,
            contexts: config.inline_contexts,
        }
    }

    /// Whether `tok` is a name the tags column shows.
    fn in_column(self, tok: &TokenView<'_>) -> bool {
        match tok.kind {
            Kind::Project => !self.projects,
            Kind::Context => !self.contexts,
            Kind::Word | Kind::Tag => false,
        }
    }
}

/// Which kinds of items the list shows.
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
    /// items (`asked`), lets `item` into the list.
    pub(crate) fn lists(self, item: &Item, asked: &[Category]) -> bool {
        self.archive
            || item.source() != Source::Done
            || asked.contains(&Category::Archived)
    }

    /// Whether the switches let `item` into the view. A category in
    /// `asked` is shown whatever its switch says.
    pub(crate) fn holds(self, item: &Item, asked: &[Category]) -> bool {
        let on = |flag: bool, cat: Category, member: bool| {
            !member || flag || asked.contains(&cat)
        };
        on(
            self.archive,
            Category::Archived,
            item.source() == Source::Done,
        ) && on(self.done, Category::Done, item.state() == State::Done)
            && on(self.later, Category::Later, item.later())
            && on(self.blocked, Category::Blocked, item.blocked())
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

/// What orders the rows and which of them stay though they do not
/// match.
#[derive(Clone, Copy)]
pub(crate) struct Arrange<'a> {
    pub(crate) sort: SortField,
    pub(crate) sticky: &'a Sticky,
}

/// The rows of the items that `scope` lets in, and of the sticky ones,
/// in the order of `how`.
pub(crate) fn build(
    snap: &Snapshot,
    scope: &Scope<'_>,
    ctx: Context,
    how: Arrange<'_>,
) -> Vec<Row> {
    let mut items: Vec<&Item> = snap.items().iter().collect();
    if how.sort == SortField::File {
        items = how.sticky.placed(items);
    } else {
        snap.sort(&mut items, how.sort);
    }
    items
        .into_iter()
        .filter_map(|item| {
            if scope.holds(snap, item) {
                Some(Row::new(snap, item, ctx))
            } else {
                let at = place_of(item);
                how.sticky.has(at).then(|| Row::stuck(snap, item, ctx))
            }
        })
        .collect()
}

impl Row {
    /// Where the row's line stands in its file.
    pub(crate) fn place(&self) -> Place {
        (self.source, self.nr)
    }

    fn new(snap: &Snapshot, item: &Item, ctx: Context) -> Self {
        let line = snap.line(item);
        let base = base_slot(item);
        let mut date = date_cell(item, line, ctx.today);
        if base.is_some() && !date.text.is_empty() {
            date.slot = base;
        }
        let owner = owner(line);
        Self {
            id: item.item_id(),
            nr: item.nr(),
            source: item.source(),
            base,
            marker: marker(item, base),
            date,
            claim: claim_cell(item, owner.as_deref(), ctx.now),
            tags_width: tags_width(line, ctx.inline),
            sticky: false,
        }
    }

    /// The row of an item that the view does not match: muted, with the
    /// word in place of the date and claim.
    fn stuck(snap: &Snapshot, item: &Item, ctx: Context) -> Self {
        let base = Some(Slot::Muted);
        Self {
            base,
            marker: marker(item, base),
            date: Cell {
                text: WORD.to_owned(),
                slot: base,
            },
            claim: Cell::default(),
            sticky: true,
            ..Self::new(snap, item, ctx)
        }
    }
}

fn owner(line: &Line) -> Option<Cow<'_, str>> {
    line.tokens()
        .find(|t| t.is_tag(b"owner"))
        .map(|t| String::from_utf8_lossy(t.value))
}

pub(crate) fn base_slot(item: &Item) -> Option<Slot> {
    if item.source() == Source::Done {
        Some(Slot::Muted)
    } else if item.state() == State::Done {
        Some(Slot::Done)
    } else if item.blocked() {
        Some(Slot::Blocked)
    } else {
        None
    }
}

fn marker(item: &Item, base: Option<Slot>) -> Span<'static> {
    if item.state() == State::Done {
        Span::new(MARKER_DONE, base)
    } else if item.blocked() {
        Span::new(MARKER_BLOCKED, Some(Slot::Blocked))
    } else if item.state() == State::Doing {
        Span::new(MARKER_DOING, Some(Slot::Doing))
    } else {
        Span::new(" ", None)
    }
}

/// The priority column: `(A)`, or nothing for no priority and for a
/// done item.
pub(crate) fn priority_span(
    item: &Item,
    line: &Line,
    base: Option<Slot>,
) -> Option<Span<'static>> {
    let pri = line.priority().filter(|_| item.state() != State::Done)?;
    let slot = base.unwrap_or_else(|| priority_slot(pri));
    let text = format!("({})", char::from(pri.letter()));
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

fn is_hidden(tok: &TokenView<'_>) -> bool {
    tok.kind == Kind::Tag && HIDDEN_KEYS.contains(&tok.key)
}

pub(crate) fn token_slot(tok: &TokenView<'_>, base: Option<Slot>) -> Slot {
    match (tok.kind, base) {
        (_, Some(slot)) => slot,
        (Kind::Project, None) => Slot::Project,
        (Kind::Context, None) => Slot::Context,
        (Kind::Word, None) => Slot::Text,
        (Kind::Tag, None) if tok.key == b"file" => Slot::Link,
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
    spans_of(line, base, |tok| !inline.in_column(tok) && !is_hidden(tok))
}

/// The projects and contexts of the tags column, one space between them.
pub(crate) fn tag_spans(
    line: &Line,
    base: Option<Slot>,
    inline: Inline,
) -> Vec<Span<'_>> {
    spans_of(line, base, |tok| inline.in_column(tok))
}

fn spans_of<'a>(
    line: &'a Line,
    base: Option<Slot>,
    keep: impl Fn(&TokenView<'a>) -> bool,
) -> Vec<Span<'a>> {
    let mut out = Vec::new();
    for tok in line.tokens().filter(|t| keep(t)) {
        let slot = Some(token_slot(&tok, base));
        if !out.is_empty() {
            out.push(Span::new(" ", slot));
        }
        out.push(Span::new(String::from_utf8_lossy(tok.text), slot));
    }
    out
}

/// The width of the tags column of a line, without building any text.
fn tags_width(line: &Line, inline: Inline) -> usize {
    let mut total = 0;
    let mut count = 0_usize;
    for tok in line.tokens().filter(|tok| inline.in_column(tok)) {
        total += width_of(&String::from_utf8_lossy(tok.text));
        count += 1;
    }
    total + count.saturating_sub(1)
}

//! Rows: the items the list shows, with what its columns need.

use std::borrow::Cow;

use jiff::Timestamp;
use lsty_core::config::{Config, Hidden};
use lsty_core::errs::Source;
use lsty_core::list::{Item, ItemId, Snapshot, State};
use lsty_core::theme::Slot;
use lsty_core::todotxt::{Kind, Line, Priority, TokenView};
use unicode_width::UnicodeWidthStr;

use crate::reldate::{Cell, Today, claim_cell, date_cell};
use crate::text::{Span, visible};

const MARKER_DONE: &str = "✓";
const MARKER_BLOCKED: &str = "⧗";
const MARKER_DOING: &str = "▶";

/// The tags that other columns show or that stay hidden.
const HIDDEN_KEYS: [&[u8]; 6] =
    [b"id", b"dep", b"started", b"owner", b"pri", b"due"];

/// One item of the list.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub(crate) id: ItemId,
    pub(crate) nr: u32,
    pub(crate) source: Source,
    /// The slot that styles the whole row, when there is one.
    pub(crate) base: Option<Slot>,
    pub(crate) marker: Span<'static>,
    pub(crate) date: Cell,
    pub(crate) claim: Cell,
    /// The width of the tags column value, in cells.
    pub(crate) tags_width: usize,
}

/// What a row takes from the clock and the config.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Context {
    pub(crate) today: Today,
    pub(crate) now: Timestamp,
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

    /// Whether the archive switch lets `item` into the list.
    pub(crate) fn lists(self, item: &Item) -> bool {
        self.archive || item.source() != Source::Done
    }

    /// Whether `item` is in the list.
    pub(crate) fn holds(self, item: &Item) -> bool {
        self.lists(item)
            && (self.done || item.state() != State::Done)
            && (self.later || !item.later())
            && (self.blocked || !item.blocked())
    }
}

/// The rows of the items `shows` lets in, in file order.
pub(crate) fn build(snap: &Snapshot, shows: Shows, ctx: Context) -> Vec<Row> {
    snap.items()
        .iter()
        .filter(|item| shows.holds(item))
        .map(|item| Row::new(snap, item, ctx))
        .collect()
}

impl Row {
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
            nr: item.nr().get(),
            source: item.source(),
            base,
            marker: marker(item, base),
            date,
            claim: claim_cell(item, owner.as_deref(), ctx.now),
            tags_width: tags_width(line),
        }
    }
}

fn owner(line: &Line) -> Option<Cow<'_, str>> {
    line.tokens()
        .find(|t| t.kind == Kind::Tag && t.key == b"owner")
        .map(|t| String::from_utf8_lossy(t.value))
}

fn base_slot(item: &Item) -> Option<Slot> {
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

fn is_name(tok: &TokenView<'_>) -> bool {
    matches!(tok.kind, Kind::Project | Kind::Context)
}

fn token_slot(tok: &TokenView<'_>, base: Option<Slot>) -> Slot {
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
pub(crate) fn text_spans(line: &Line, base: Option<Slot>) -> Vec<Span<'_>> {
    spans_of(line, base, |tok| !is_name(tok) && !is_hidden(tok))
}

/// The projects and contexts, one space between them.
pub(crate) fn tag_spans(line: &Line, base: Option<Slot>) -> Vec<Span<'_>> {
    spans_of(line, base, is_name)
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

/// The width of the projects and contexts of a line, without building
/// any text.
fn tags_width(line: &Line) -> usize {
    let mut total = 0;
    let mut count = 0_usize;
    for tok in line.tokens().filter(is_name) {
        total += visible(String::from_utf8_lossy(tok.text)).width();
        count += 1;
    }
    total + count.saturating_sub(1)
}

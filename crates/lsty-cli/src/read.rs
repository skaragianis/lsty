//! `ls`, `lsa`, `lsp`, `lsc` and `lsprj`: the commands that list.

use lsty_core::errs::{Result, Source};
use lsty_core::list::{Item, Snapshot, SortField};
use lsty_core::query::{Category, Context, Query};
use lsty_core::theme::Slot;
use lsty_core::todotxt::Kind;

use crate::commands::Call;
use crate::output::{Field, Json, Output};
use crate::paint::Painter;
use crate::parse::Opt;

/// Which items a command is interested in, besides the query.
pub(crate) enum Accept {
    Any,
    /// Open items with a priority, from the set when there is one.
    Priority(Option<Priorities>),
}

/// The letters of a priority list such as `A-CF`.
pub(crate) struct Priorities(Vec<(u8, u8)>);

/// What one list command asks for.
pub(crate) struct Request<'a> {
    /// Whether the done file is in scope without being asked for.
    pub(crate) all: bool,
    pub(crate) args: &'a [String],
    pub(crate) accept: Accept,
}

/// The items a list command shows, with the counts of its footer.
pub(crate) struct Selection<'s> {
    pub(crate) shown: Vec<&'s Item>,
    pub(crate) total: usize,
    pub(crate) hidden: usize,
}

/// `lsty ls`.
pub fn ls(call: &mut Call<'_, '_>) -> Result<Output> {
    let args = &call.parsed.args;
    list_items(
        call,
        &Request {
            all: false,
            args,
            accept: Accept::Any,
        },
    )
}

/// `lsty lsa`.
pub fn lsa(call: &mut Call<'_, '_>) -> Result<Output> {
    let args = &call.parsed.args;
    list_items(
        call,
        &Request {
            all: true,
            args,
            accept: Accept::Any,
        },
    )
}

/// `lsty lsp`.
pub fn lsp(call: &mut Call<'_, '_>) -> Result<Output> {
    let args = call.parsed.args.as_slice();
    let (accept, args) = match args.split_first() {
        Some((first, rest)) if is_priorities(first) => {
            let set = Priorities::parse(&first.to_ascii_uppercase());
            (Accept::Priority(Some(set)), rest)
        }
        _ => (Accept::Priority(None), args),
    };
    list_items(
        call,
        &Request {
            all: false,
            args,
            accept,
        },
    )
}

/// `lsty lsc`.
pub fn lsc(call: &mut Call<'_, '_>) -> Result<Output> {
    list_names(call, Kind::Context, b'@')
}

/// `lsty lsprj`.
pub fn lsprj(call: &mut Call<'_, '_>) -> Result<Output> {
    list_names(call, Kind::Project, b'+')
}

/// Whether `arg` has the shape of a priorities argument: one letter,
/// `X-Y`, or upper-case letters and hyphens from a letter to a letter.
pub(crate) fn is_priorities(arg: &str) -> bool {
    let b = arg.as_bytes();
    match b {
        [one] => one.is_ascii_alphabetic(),
        [lo, b'-', hi] if lo.is_ascii_alphabetic() => hi.is_ascii_alphabetic(),
        [first, middle @ .., last] => {
            first.is_ascii_uppercase()
                && last.is_ascii_uppercase()
                && middle.iter().all(|c| c.is_ascii_uppercase() || *c == b'-')
        }
        [] => false,
    }
}

impl Priorities {
    pub(crate) fn parse(letters: &str) -> Self {
        let b = letters.as_bytes();
        let mut ranges = Vec::new();
        let mut i = 0;
        while let Some(&lo) = b.get(i) {
            let range = b.get(i + 1) == Some(&b'-') && i + 2 < b.len();
            let hi = b.get(i + 2).copied().filter(|_| range);
            ranges.push((lo, hi.unwrap_or(lo)));
            i += if range { 3 } else { 1 };
        }
        Self(ranges)
    }

    pub(crate) fn holds(&self, letter: u8) -> bool {
        self.0.iter().any(|&(lo, hi)| (lo..=hi).contains(&letter))
    }
}

impl Accept {
    fn accepts(&self, snap: &Snapshot, item: &Item) -> bool {
        let Self::Priority(set) = self else {
            return true;
        };
        let Some(p) = snap.line(item).effective_priority() else {
            return false;
        };
        item.is_open() && set.as_ref().is_none_or(|s| s.holds(p.letter()))
    }
}

/// What decides which items are in scope and shown.
struct Scope<'a> {
    query: &'a Query,
    ctx: Context<'a>,
    accept: &'a Accept,
    with_done: bool,
    show_later: bool,
}

impl<'s> Selection<'s> {
    fn choose(snap: &'s Snapshot, scope: &Scope<'_>) -> Self {
        let mut sel = Self {
            shown: Vec::new(),
            total: 0,
            hidden: 0,
        };
        for item in snap.items() {
            sel.add(snap, item, scope);
        }
        sel
    }

    fn add(&mut self, snap: &'s Snapshot, item: &'s Item, scope: &Scope<'_>) {
        if !scope.with_done && item.source() == Source::Done {
            return;
        }
        self.total += 1;
        let matches = scope.query.matches(snap, item, &scope.ctx)
            && scope.accept.accepts(snap, item);
        if !matches {
            return;
        }
        if scope.query.hides_later(item, scope.show_later) {
            self.hidden += 1;
        } else {
            self.shown.push(item);
        }
    }
}

pub(crate) fn select<'s>(
    call: &Call<'_, '_>,
    snap: &'s Snapshot,
    query: &Query,
    req: &Request<'_>,
) -> Selection<'s> {
    let scope = Scope {
        query,
        ctx: Context::from_clock(&call.clock),
        accept: &req.accept,
        with_done: req.all || query.asks_for(Category::Archived),
        show_later: call.has(Opt::Later),
    };
    let mut sel = Selection::choose(snap, &scope);
    let field = call
        .option(Opt::Sort)
        .and_then(SortField::parse)
        .unwrap_or(SortField::File);
    snap.sort(&mut sel.shown, field);
    sel
}

fn list_items(call: &Call<'_, '_>, req: &Request<'_>) -> Result<Output> {
    let query = call.parse_query(req.args)?;
    let snap = call.load()?;
    let sel = select(call, &snap, &query, req);
    let messages = snap.warnings_for(sel.shown.iter().copied());
    let raw = call.has(Opt::Raw);
    let mut lines: Vec<Vec<u8>> = sel
        .shown
        .iter()
        .map(|item| {
            if raw {
                snap.line(item).raw().to_vec()
            } else {
                list_line(&call.paint, &snap, item)
            }
        })
        .collect();
    if !raw {
        let footer = format!(
            "-- {} of {} shown, {} hidden",
            sel.shown.len(),
            sel.total,
            sel.hidden
        );
        lines.push(call.paint.paint(Slot::Muted, footer.as_bytes()));
        lines.extend(text_lines(&call.paint, messages.iter()));
    }
    let fields = if call.json() {
        let items = call.item_json(&snap).objects(&sel.shown);
        vec![
            Field::new("items", items),
            hidden_count(&sel),
            Field::strs("messages", &messages),
        ]
    } else {
        Vec::new()
    };
    Ok(Output { fields, lines })
}

fn hidden_count(sel: &Selection<'_>) -> Field {
    Field::int(
        "hidden_count",
        i64::try_from(sel.hidden).unwrap_or(i64::MAX),
    )
}

/// The item number, as the list prints it: padded to the width of the
/// file's line count, with `done:` before a number of the done file.
fn number(snap: &Snapshot, item: &Item) -> String {
    let width = snap.file(item.source()).lines.len().to_string().len();
    let nr = format!("{:0width$}", item.nr().get());
    match item.source() {
        Source::Todo => nr,
        Source::Done => format!("done:{nr}"),
    }
}

/// The number of an item, a space and the line as it is stored, with
/// the number and the parts of the line coloured.
pub fn list_line(paint: &Painter, snap: &Snapshot, item: &Item) -> Vec<u8> {
    let raw = snap.line(item);
    let mut line = paint.paint(Slot::Muted, number(snap, item).as_bytes());
    line.reserve(raw.raw().len() + 1);
    line.push(b' ');
    paint.item(&mut line, raw, item);
    line
}

/// Each text in the `text` slot.
pub fn text_lines<T: AsRef<[u8]>>(
    paint: &Painter,
    texts: impl Iterator<Item = T>,
) -> impl Iterator<Item = Vec<u8>> {
    texts.map(|text| paint.paint(Slot::Text, text.as_ref()))
}

fn list_names(call: &Call<'_, '_>, kind: Kind, sigil: u8) -> Result<Output> {
    let args = &call.parsed.args;
    let req = Request {
        all: false,
        args,
        accept: Accept::Any,
    };
    let query = call.parse_query(args)?;
    let snap = call.load()?;
    let sel = select(call, &snap, &query, &req);
    let counts = snap.names(&sel.shown, kind);
    let messages = snap.warnings_for(sel.shown.iter().copied());
    let slot = if kind == Kind::Project {
        Slot::Project
    } else {
        Slot::Context
    };
    let mut lines: Vec<Vec<u8>> = counts
        .iter()
        .map(|c| call.paint.paint(slot, &[&[sigil][..], &c.name].concat()))
        .collect();
    lines.extend(text_lines(&call.paint, messages.iter()));
    let names = counts.iter().map(|c| {
        Json::object(&[
            Field::new("name", Json::bytes(&c.name)),
            Field::int("open", i64::try_from(c.open).unwrap_or(i64::MAX)),
            Field::int("total", i64::try_from(c.total).unwrap_or(i64::MAX)),
        ])
    });
    Ok(Output {
        fields: vec![
            Field::array("names", names),
            hidden_count(&sel),
            Field::strs("messages", &messages),
        ],
        lines,
    })
}

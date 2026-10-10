//! `ls`, `lsa`, `lsp`, `lsc` and `lsprj`: the commands that list.

use lsty_core::{
    errs::{Result, Source},
    list::{Item, Snapshot, SortField},
    query::{Category, Context, Query},
    theme::Slot,
    todotxt::Kind,
};

use crate::{
    commands::Call,
    output::{Field, Json, Output},
    paint::Painter,
    parse::Opt,
};

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

/// The items a list command shows, with the total of its footer.
pub(crate) struct Selection<'s> {
    pub(crate) shown: Vec<&'s Item>,
    pub(crate) total: usize,
}

/// `lsty ls`.
pub(crate) fn ls(call: &Call<'_>) -> Result<Output> {
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
pub(crate) fn lsa(call: &Call<'_>) -> Result<Output> {
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
pub(crate) fn lsp(call: &Call<'_>) -> Result<Output> {
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
pub(crate) fn lsc(call: &Call<'_>) -> Result<Output> {
    list_names(call, Kind::Context, b'@')
}

/// `lsty lsprj`.
pub(crate) fn lsprj(call: &Call<'_>) -> Result<Output> {
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

pub(crate) fn select<'s>(
    call: &Call<'_>,
    snap: &'s Snapshot,
    query: &Query,
    req: &Request<'_>,
) -> Selection<'s> {
    let with_done = req.all || query.asks_for(Category::Archived);
    let ctx = Context::from_clock(call.ops.clock);
    let mut sel = Selection {
        shown: Vec::new(),
        total: 0,
    };
    for item in snap.items() {
        if !with_done && item.source() == Source::Done {
            continue;
        }
        sel.total += 1;
        if query.matches(snap, item, &ctx) && req.accept.accepts(snap, item) {
            sel.shown.push(item);
        }
    }
    let field = call
        .option(Opt::Sort)
        .and_then(SortField::parse)
        .unwrap_or(SortField::File);
    snap.sort(&mut sel.shown, field);
    sel
}

/// The filter's warnings, then those about the shown items.
fn messages(
    query: &Query,
    snap: &Snapshot,
    sel: &Selection<'_>,
) -> Vec<String> {
    let mut messages = query.warnings().to_vec();
    messages.extend(snap.warnings_for(sel.shown.iter().copied()));
    messages
}

fn list_items(call: &Call<'_>, req: &Request<'_>) -> Result<Output> {
    let query = call.parse_query(req.args)?;
    let snap = call.load()?;
    let sel = select(call, &snap, &query, req);
    let messages = messages(&query, &snap, &sel);
    let raw = call.has(Opt::Raw);
    Ok(Output::either(
        call.json(),
        || {
            vec![
                Field::new("items", call.item_json(&snap).objects(&sel.shown)),
                Field::strs("messages", &messages),
            ]
        },
        || {
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
                let footer =
                    format!("-- {} of {} shown", sel.shown.len(), sel.total);
                lines.push(call.paint.paint(Slot::Muted, footer.as_bytes()));
                lines.extend(text_lines(&call.paint, messages.iter()));
            }
            lines
        },
    ))
}

/// How many decimal digits `n` has; one for zero.
fn digits(n: usize) -> usize {
    n.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// The number of an item, a space and the line as it is stored, with
/// the number and the parts of the line coloured.
pub(crate) fn list_line(
    paint: &Painter,
    snap: &Snapshot,
    item: &Item,
) -> Vec<u8> {
    let raw = snap.line(item);
    let width = digits(snap.file(item.source()).lines.len());
    let number = item
        .source()
        .numbered(format_args!("{:0width$}", item.nr().get()));
    let mut line = Vec::with_capacity(raw.raw().len() + number.len() + 16);
    paint.paint_into(&mut line, Slot::Muted, number.as_bytes());
    line.push(b' ');
    paint.item(&mut line, raw, item);
    line
}

/// Each text in the `text` slot.
pub(crate) fn text_lines<T: AsRef<[u8]>>(
    paint: &Painter,
    texts: impl Iterator<Item = T>,
) -> impl Iterator<Item = Vec<u8>> {
    texts.map(|text| paint.paint(Slot::Text, text.as_ref()))
}

fn list_names(call: &Call<'_>, kind: Kind, sigil: u8) -> Result<Output> {
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
    let messages = messages(&query, &snap, &sel);
    let slot = if kind == Kind::Project {
        Slot::Project
    } else {
        Slot::Context
    };
    Ok(Output::either(
        call.json(),
        || {
            let names = counts.iter().map(|(name, c)| {
                Json::object(&[
                    Field::new("name", Json::bytes(name)),
                    Field::count("open", c.open),
                    Field::count("total", c.total),
                ])
            });
            vec![
                Field::array("names", names),
                Field::strs("messages", &messages),
            ]
        },
        || {
            let mut lines: Vec<Vec<u8>> = counts
                .keys()
                .map(|name| {
                    call.paint.paint(slot, &[&[sigil][..], name].concat())
                })
                .collect();
            lines.extend(text_lines(&call.paint, messages.iter()));
            lines
        },
    ))
}

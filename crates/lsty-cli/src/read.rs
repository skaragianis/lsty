//! `ls`, `lsa`, `lsp`, `lsc` and `lsprj`: the commands that list.

use std::{
    io::{BufWriter, Write},
    path::Path,
};

use lsty_core::{
    errs::{Result, Source, lossy, unplaced},
    list::{Snapshot, SortField, Task},
    query::{Category, Context, Query},
    theme::Slot,
    todotxt::{Kind, Line},
};

use crate::{
    commands::Call,
    inline::Places,
    output::{Field, Json, Output},
    paint::Painter,
    parse::{Hide, Opt},
    task_json::TaskJson,
};

/// How a read numbers its tasks for a caller that shows a list: by their
/// place in it, else by `id:`, else by line. A reading that is not the one
/// the list was built from has no place to give. It owns a copy of the
/// places: a read streams its lines after the call that made them is gone.
#[derive(Clone)]
pub(crate) struct Numbers {
    places: Places,
    same: bool,
}

impl Numbers {
    /// The number of `task`.
    pub(crate) fn of(&self, snap: &Snapshot, task: &Task) -> String {
        let place = self.places.get(&(task.source(), task.nr()));
        place.filter(|_| self.same).map_or_else(
            || {
                let id = snap.id_of(task).map(lossy);
                unplaced(task.source(), task.nr(), id.as_deref())
            },
            usize::to_string,
        )
    }
}

impl Call<'_> {
    /// The numbering of what `snap` shows, when the caller shows a list.
    pub(crate) fn numbers(&self, snap: &Snapshot) -> Option<Numbers> {
        let same = |aim: &Snapshot| {
            [Source::Todo, Source::Done].into_iter().all(|source| {
                let (a, b) = (aim.file(source), snap.file(source));
                a.lines
                    .iter()
                    .map(Line::raw)
                    .eq(b.lines.iter().map(Line::raw))
            })
        };
        Some(Numbers {
            places: self.places?.clone(),
            same: self.ops.aim.is_none_or(same),
        })
    }
}

/// The letters of a priority list such as `A-CF`.
pub(crate) struct Priorities(Vec<(u8, u8)>);

/// The tasks a list command shows, with the total of its footer.
struct Selection<'s> {
    shown: Vec<&'s Task>,
    total: usize,
}

/// `lsty ls`.
pub(crate) fn ls(call: &Call<'_>) -> Result<Output> {
    list_tasks(call, false, &call.parsed.args, None)
}

/// `lsty lsa`.
pub(crate) fn lsa(call: &Call<'_>) -> Result<Output> {
    list_tasks(call, true, &call.parsed.args, None)
}

/// `lsty lsp`.
pub(crate) fn lsp(call: &Call<'_>) -> Result<Output> {
    let args = call.parsed.args.as_slice();
    let (set, args) = match args.split_first() {
        Some((first, rest)) if is_priorities(first) => {
            (Priorities::parse(&first.to_ascii_uppercase()), rest)
        }
        _ => (Priorities::parse("A-Z"), args),
    };
    list_tasks(call, false, args, Some(&set))
}

/// `lsty lsc`.
pub(crate) fn lsc(call: &Call<'_>) -> Result<Output> {
    list_names(call, Kind::Context, b'@', Slot::Context)
}

/// `lsty lsprj`.
pub(crate) fn lsprj(call: &Call<'_>) -> Result<Output> {
    list_names(call, Kind::Project, b'+', Slot::Project)
}

/// Whether `arg` has the shape of a priorities argument: one letter,
/// `X-Y`, or upper-case letters and hyphens from a letter to a letter.
fn is_priorities(arg: &str) -> bool {
    match arg.as_bytes() {
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
            let range = b.get(i + 1) == Some(&b'-');
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

/// The tasks of `snap` the query matches; `set` keeps only the open
/// tasks whose priority it holds.
fn select<'s>(
    call: &Call<'_>,
    snap: &'s Snapshot,
    query: &Query,
    all: bool,
    set: Option<&Priorities>,
) -> Selection<'s> {
    let with_done = all || query.asks_for(Category::Archived);
    let ctx = Context::from_clock(call.ops.clock);
    let mut sel = Selection {
        shown: Vec::new(),
        total: 0,
    };
    for task in snap.tasks() {
        if !with_done && task.source() == Source::Done {
            continue;
        }
        sel.total += 1;
        let held = set.is_none_or(|s| {
            task.is_open()
                && snap
                    .line(task)
                    .effective_priority()
                    .is_some_and(|p| s.holds(p.letter()))
        });
        if held && query.matches(snap, task, &ctx) {
            sel.shown.push(task);
        }
    }
    let field = call
        .option(Opt::Sort)
        .and_then(SortField::parse)
        .unwrap_or_default();
    snap.sort(&mut sel.shown, field);
    sel
}

/// The filter's warnings, then those about the shown tasks.
fn messages(
    query: &Query,
    snap: &Snapshot,
    sel: &Selection<'_>,
) -> Vec<String> {
    let mut messages = query.warnings().to_vec();
    messages.extend(snap.warnings_for(sel.shown.iter().copied()));
    messages
}

fn list_tasks(
    call: &Call<'_>,
    all: bool,
    args: &[String],
    set: Option<&Priorities>,
) -> Result<Output> {
    let query = call.parse_query(args)?;
    let snap = call.load()?;
    let sel = select(call, &snap, &query, all, set);
    let messages = messages(&query, &snap, &sel);
    let raw = call.has(Opt::Raw);
    let json = call.json();
    let ids: Vec<_> = sel.shown.iter().map(|t| t.task_id()).collect();
    let home = call.home.map(Path::to_path_buf);
    let paint = call.paint.clone();
    let hide = call.parsed.hide;
    let numbers = call.numbers(&snap);
    let footer = format!("-- {} of {} shown", sel.shown.len(), sel.total);
    let fields = if json {
        vec![Field::streamed("tasks"), Field::strs("messages", &messages)]
    } else {
        Vec::new()
    };
    Ok(Output {
        fields,
        stream: Some(Box::new(move |w| {
            let tasks = ids.iter().filter_map(|id| snap.task(*id));
            let mut w = BufWriter::new(w);
            if json {
                let json = TaskJson {
                    snap: &snap,
                    home: home.as_deref(),
                };
                json.write_array(&mut w, tasks)?;
            } else {
                let how = (!raw).then_some(hide);
                let body = tasks.map(|t| {
                    shown_line(&paint, &snap, t, how, numbers.as_ref())
                });
                let tail = (!raw)
                    .then(|| paint.paint(Slot::Muted, footer.as_bytes()));
                let notes =
                    text_lines(&paint, messages.iter().filter(|_| !raw));
                for line in body.chain(tail).chain(notes) {
                    w.write_all(&line)?;
                    w.write_all(b"\n")?;
                }
            }
            w.flush()
        })),
        ..Output::default()
    })
}

/// The line of a task in a list: as it is stored with no `hide`, else made
/// by [`list_line`].
fn shown_line(
    paint: &Painter,
    snap: &Snapshot,
    task: &Task,
    hide: Option<Hide>,
    numbers: Option<&Numbers>,
) -> Vec<u8> {
    match hide {
        None => snap.line(task).raw().to_vec(),
        Some(hide) => list_line(paint, snap, task, hide, numbers),
    }
}

/// The number of a task, a space and the line as it is stored, with
/// the number and the parts of the line coloured.
pub(crate) fn list_line(
    paint: &Painter,
    snap: &Snapshot,
    task: &Task,
    hide: Hide,
    numbers: Option<&Numbers>,
) -> Vec<u8> {
    let raw = snap.line(task);
    let mut line = Vec::with_capacity(raw.raw().len() + 64);
    let label = numbers.map(|n| n.of(snap, task));
    if let Some(label) = label {
        paint.paint_into(&mut line, Slot::Muted, label.as_bytes());
    } else {
        let (number, at) = padded(snap, task);
        paint.paint_into(&mut line, Slot::Muted, &number[at..]);
    }
    if hide == Hide::default() {
        line.push(b' ');
        paint.task(&mut line, raw, task);
    } else {
        let rest = hidden(raw.raw(), hide);
        if let Some((&gap, text)) = rest.split_first() {
            line.push(gap);
            paint.task(&mut line, &Line::parse(text), task);
        }
    }
    line
}

/// The file line of a task, padded with zeros to the width of the file's
/// last line, in the tail of an array from the index given.
fn padded(snap: &Snapshot, task: &Task) -> ([u8; 32], usize) {
    let lines = snap.file(task.source()).lines.len();
    let width = lines.checked_ilog10().map_or(1, |log| log as usize + 1);
    // By hand on the stack: a list prints thousands of these, and the
    // formatting machinery costs more than the rest of the line.
    let mut number = [b'0'; 32];
    let mut at = number.len();
    let mut rest = task.nr().get();
    // A line number is at most the count of lines, so it has at most
    // `width` digits.
    for _ in 0..width {
        at -= 1;
        number[at] = b"0123456789"[(rest % 10) as usize];
        rest /= 10;
    }
    let prefix = task.source().prefix().as_bytes();
    at -= prefix.len();
    number[at..at + prefix.len()].copy_from_slice(prefix);
    (number, at)
}

/// A byte `[[:graph:]]` matches: anything but controls and blanks.
fn graph(b: u8) -> bool {
    b > 0x20 && b != 0x7f
}

/// The text after the number with the hidden parts cut out, gap first:
/// empty when nothing is left. A token goes with the white space before
/// it, and only when a sigil starts it and something follows.
fn hidden(raw: &[u8], hide: Hide) -> Vec<u8> {
    let mut text = vec![b' '];
    let own = match raw {
        // a line that starts so is never done: that needs `x `
        [b'(', b'A'..=b'Z', b')', b' ', rest @ ..] if hide.priority => rest,
        _ => raw,
    };
    text.extend_from_slice(own);
    for (on, sigil) in [(hide.projects, b'+'), (hide.contexts, b'@')] {
        if on {
            text = cut(&text, sigil);
        }
    }
    text
}

/// `text` without each run of white space, `sigil` and graph bytes.
fn cut(text: &[u8], sigil: u8) -> Vec<u8> {
    let mut kept = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let tail = &text[at..];
        let token = tail.len() > 2
            && (tail[0] == 0x0b || tail[0].is_ascii_whitespace())
            && tail[1] == sigil
            && graph(tail[2]);
        if token {
            at += 2;
            at += text[at..].iter().take_while(|&&b| graph(b)).count();
        } else {
            kept.push(text[at]);
            at += 1;
        }
    }
    kept
}

/// Each text in the `text` slot.
pub(crate) fn text_lines<T: AsRef<[u8]>>(
    paint: &Painter,
    texts: impl Iterator<Item = T>,
) -> impl Iterator<Item = Vec<u8>> {
    texts.map(|text| paint.paint(Slot::Text, text.as_ref()))
}

fn list_names(
    call: &Call<'_>,
    kind: Kind,
    sigil: u8,
    slot: Slot,
) -> Result<Output> {
    let query = call.parse_query(&call.parsed.args)?;
    let snap = call.load()?;
    let sel = select(call, &snap, &query, false, None);
    let counts = snap.names(&sel.shown, kind);
    let messages = messages(&query, &snap, &sel);
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

#[cfg(test)]
mod read_tests;

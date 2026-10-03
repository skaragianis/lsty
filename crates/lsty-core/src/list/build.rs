//! Deriving the snapshot of two files read at the same moment.

use std::collections::HashMap;
use std::ops::Range;

use jiff::{SignedDuration, Timestamp};

use super::cycle;
use super::ids::{self, source_of};
use super::snapshot::{
    Claim, Item, ItemId, Params, Snapshot, State, next_serial,
};
use crate::dates::Date;
use crate::discover::Found;
use crate::errs::{Nr, Source};
use crate::todotxt::{File, Kind, Line, Tags};

/// A line with its tags, before anything is derived across lines.
struct Row<'a> {
    nr: Nr,
    line: &'a Line,
    tags: Tags<'a>,
}

impl Row<'_> {
    fn id(&self) -> &[u8] {
        self.tags.id.unwrap_or_default()
    }

    fn open(&self) -> bool {
        !self.line.is_done()
    }
}

/// Every non-blank line of `file`, with its number.
fn rows(file: &File) -> impl Iterator<Item = Row<'_>> {
    file.lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.is_blank())
        .filter_map(|(i, line)| {
            Some(Row {
                nr: Nr::from_index(i)?,
                line,
                tags: line.tags(),
            })
        })
}

/// What [`derive`] finds out about the lines.
struct Derived {
    items: Vec<Item>,
    todo_items: usize,
    by_id: Vec<usize>,
    warned: Vec<ids::Warned>,
}

impl Snapshot {
    /// Derives the snapshot of two files read at the same moment. It reads
    /// nothing: the clock is the one in `params`.
    pub fn build(
        files: Found,
        todo: File,
        done: File,
        params: &Params,
    ) -> Self {
        let serial = next_serial();
        let derived = derive(&files, &todo, &done, params, serial);
        let warnings = derived
            .warned
            .iter()
            .map(ids::Warned::text)
            .map(String::from);
        Self {
            serial,
            warnings: warnings.collect(),
            files,
            todo,
            done,
            items: derived.items,
            todo_items: derived.todo_items,
            by_id: derived.by_id,
            warned: derived.warned,
        }
    }
}

fn derive(
    files: &Found,
    todo: &File,
    done: &File,
    params: &Params,
    serial: u32,
) -> Derived {
    let todo_rows: Vec<Row<'_>> = rows(todo).collect();
    let todo_items = todo_rows.len();
    let all: Vec<Row<'_>> = todo_rows.into_iter().chain(rows(done)).collect();
    let ids: Vec<&[u8]> = all.iter().map(Row::id).collect();
    let by_id = ids::sorted(&ids);
    let key = |i: usize| ids.get(i).copied().unwrap_or_default();
    let first = first_index(&by_id, key);
    let warned =
        ids::warnings(&by_id, todo_items, [&files.todo, &files.done], key);
    let mut items: Vec<Item> = all
        .iter()
        .enumerate()
        .map(|(i, row)| {
            item(row, ItemId::new(i, serial), source_of(i, todo_items))
        })
        .collect();
    link(&mut items, &all, &first, serial);
    flag(&mut items, &all, params);
    Derived {
        items,
        todo_items,
        by_id,
        warned,
    }
}

/// An item with what one line alone says.
fn item(row: &Row<'_>, item_id: ItemId, source: Source) -> Item {
    let tags = &row.tags;
    let state = if row.line.is_done() {
        State::Done
    } else if tags.underway {
        State::Doing
    } else {
        State::Todo
    };
    Item {
        nr: row.nr,
        source,
        state,
        claim: claim(tags),
        blockers: Vec::new(),
        dangling: Vec::new(),
        cycle: false,
        later: false,
        overdue: false,
        stale: false,
        key: item_id,
        id: id_range(row),
        started: tags.started,
    }
}

fn claim(tags: &Tags<'_>) -> Claim {
    if !tags.underway {
        Claim::None
    } else if tags.owner.is_some_and(|o| !o.is_empty()) {
        Claim::Agent
    } else {
        Claim::Human
    }
}

/// Where the value of the first `id:` tag sits in the line's bytes.
fn id_range(row: &Row<'_>) -> Option<Range<usize>> {
    let id = row.tags.id.filter(|id| !id.is_empty())?;
    tag_value(row.line, b"id", id)
}

/// Where the first `key:` tag with value `value` sits in the line.
fn tag_value(line: &Line, key: &[u8], value: &[u8]) -> Option<Range<usize>> {
    let tag = line
        .tokens()
        .find(|t| t.kind == Kind::Tag && t.key == key && t.value == value)?;
    let start = tag.start + tag.key.len() + 1;
    Some(start..start + value.len())
}

/// The first item of each id in file order, the todo file first.
fn first_index<'k>(
    by_id: &[usize],
    key: impl Fn(usize) -> &'k [u8],
) -> HashMap<&'k [u8], usize> {
    let mut first = HashMap::with_capacity(by_id.len());
    for &i in by_id {
        first.entry(key(i)).or_insert(i);
    }
    first
}

/// Resolves every `dep:`: a prerequisite that is open blocks an open item,
/// and an id that is nowhere is dangling.
fn link(
    items: &mut [Item],
    rows: &[Row<'_>],
    first: &HashMap<&[u8], usize>,
    serial: u32,
) {
    for (item, row) in items.iter_mut().zip(rows) {
        for &dep in &row.tags.deps {
            let Some(&pre) = first.get(dep) else {
                item.dangling.extend(tag_value(row.line, b"dep", dep));
                continue;
            };
            if row.open() && rows.get(pre).is_some_and(Row::open) {
                item.blockers.push(ItemId::new(pre, serial));
            }
        }
    }
}

/// Cycles, later, overdue and stale.
fn flag(items: &mut [Item], rows: &[Row<'_>], params: &Params) {
    let cyclic = cycle::members(items);
    let today = params.clock.today();
    let now = params.clock.now();
    let after = SignedDuration::try_from(params.stale_after)
        .unwrap_or(SignedDuration::MAX);
    for ((item, row), in_cycle) in items.iter_mut().zip(rows).zip(cyclic) {
        item.cycle = in_cycle;
        item.later = date(row.tags.threshold).is_some_and(|d| d > today);
        item.overdue =
            item.is_open() && date(row.tags.due).is_some_and(|d| d < today);
        item.stale = item.is_open() && is_stale(item, now, after);
    }
}

fn date(text: Option<&str>) -> Option<Date> {
    Date::parse(text?)
}

fn is_stale(item: &Item, now: Timestamp, after: SignedDuration) -> bool {
    item.claim == Claim::Agent
        && item
            .started
            .is_some_and(|started| now.duration_since(started) > after)
}

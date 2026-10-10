//! Deriving the snapshot of two files read at the same moment.

use std::{collections::HashMap, ops::Range};

use jiff::{SignedDuration, Timestamp};

use super::{
    cycle, ids,
    snapshot::{Claim, Item, ItemId, Params, Snapshot, State, next_serial},
};
use crate::{
    dates::Date,
    discover::Found,
    errs::{Nr, Source},
    todotxt::{File, Line, Tags},
};

/// What derive keeps of a line besides its item: what the whole-list
/// steps read.
struct Row<'a> {
    line: &'a Line,
    /// Where the line's `dep:` values sit in [`Lines::deps`].
    deps: Range<usize>,
}

/// The items of both files with the values the whole-list steps need,
/// each line's tags read once.
struct Lines<'a> {
    items: Vec<Item>,
    rows: Vec<Row<'a>>,
    /// The `id:` value of every line, empty when there is none.
    ids: Vec<&'a [u8]>,
    /// The `dep:` values of every line, in line order.
    deps: Vec<&'a [u8]>,
    /// The values of second and later `id:` tags.
    extra_ids: Vec<Vec<u8>>,
}

/// The clock readings an item's own flags are worked out against.
struct When {
    today: Date,
    now: Timestamp,
    stale_after: SignedDuration,
}

impl When {
    fn of(params: &Params) -> Self {
        Self {
            today: params.clock.today(),
            now: params.clock.now(),
            stale_after: SignedDuration::try_from(params.stale_after)
                .unwrap_or(SignedDuration::MAX),
        }
    }
}

impl<'a> Lines<'a> {
    fn with_capacity(n: usize) -> Self {
        Self {
            items: Vec::with_capacity(n),
            rows: Vec::with_capacity(n),
            ids: Vec::with_capacity(n),
            deps: Vec::new(),
            extra_ids: Vec::new(),
        }
    }

    /// Adds every non-blank line of `file`.
    fn add_file(
        &mut self,
        file: &'a File,
        source: Source,
        when: &When,
        serial: u32,
    ) {
        let lines = file.lines.iter().enumerate();
        for (i, line) in lines.filter(|(_, line)| !line.is_blank()) {
            if let Some(nr) = Nr::from_index(i) {
                let at = Place {
                    nr,
                    source,
                    key: ItemId::new(self.items.len(), serial),
                };
                self.add(line, &at, when);
            }
        }
    }

    fn add(&mut self, line: &'a Line, at: &Place, when: &When) {
        let tags = line.tags();
        let start = self.deps.len();
        self.deps.extend_from_slice(&tags.deps);
        let extra = tags.extra.iter().filter(|t| t.key == b"id");
        self.extra_ids.extend(extra.map(|t| t.value.to_vec()));
        self.items.push(item(line, &tags, at, when));
        self.ids.push(tags.id.unwrap_or_default());
        self.rows.push(Row {
            line,
            deps: start..self.deps.len(),
        });
    }
}

/// Where an item sits.
struct Place {
    nr: Nr,
    source: Source,
    key: ItemId,
}

/// What [`derive`] finds out about the lines.
struct Derived {
    items: Vec<Item>,
    todo_items: usize,
    by_id: Vec<usize>,
    extra_ids: Vec<Vec<u8>>,
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
        Self {
            serial,
            files,
            todo,
            done,
            items: derived.items,
            todo_items: derived.todo_items,
            by_id: derived.by_id,
            extra_ids: derived.extra_ids,
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
    let when = When::of(params);
    let mut lines = Lines::with_capacity(todo.lines.len() + done.lines.len());
    lines.add_file(todo, Source::Todo, &when, serial);
    let todo_items = lines.items.len();
    lines.add_file(done, Source::Done, &when, serial);
    let by_id = ids::sorted(&lines.ids);
    let key = |i: usize| lines.ids.get(i).copied().unwrap_or_default();
    let first = first_index(&by_id, key);
    let warned =
        ids::warnings(&by_id, todo_items, [&files.todo, &files.done], key);
    link(&mut lines, &first, serial);
    let cyclic = cycle::members(&lines.items);
    for (item, in_cycle) in lines.items.iter_mut().zip(cyclic) {
        item.cycle = in_cycle;
    }
    lines.extra_ids.sort_unstable();
    // Blank lines reserved room that the snapshot would keep.
    lines.items.shrink_to_fit();
    Derived {
        items: lines.items,
        todo_items,
        by_id,
        extra_ids: lines.extra_ids,
        warned,
    }
}

/// An item with what one line alone says.
fn item(line: &Line, tags: &Tags<'_>, at: &Place, when: &When) -> Item {
    let state = if line.is_done() {
        State::Done
    } else if tags.underway {
        State::Doing
    } else {
        State::Todo
    };
    let open = state != State::Done;
    let claim = claim(tags);
    Item {
        nr: at.nr,
        source: at.source,
        state,
        claim,
        blockers: Vec::new(),
        dangling: Vec::new(),
        cycle: false,
        later: date(tags.threshold).is_some_and(|d| d > when.today),
        overdue: open && date(tags.due).is_some_and(|d| d < when.today),
        stale: open && is_stale(claim, tags.started, when),
        key: at.key,
        id: id_range(line, tags),
        started: tags.started,
    }
}

fn claim(tags: &Tags<'_>) -> Claim {
    if !tags.underway {
        Claim::None
    } else if tags.owner.is_some() {
        Claim::Agent
    } else {
        Claim::Human
    }
}

/// Where the value of the first `id:` tag sits in the line's bytes.
fn id_range(line: &Line, tags: &Tags<'_>) -> Option<Range<usize>> {
    tag_value(line, b"id", tags.id?)
}

/// Where the first `key:` tag with value `value` sits in the line.
fn tag_value(line: &Line, key: &[u8], value: &[u8]) -> Option<Range<usize>> {
    let tag = line.tokens().find(|t| t.is_tag(key) && t.value == value)?;
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
fn link(lines: &mut Lines<'_>, first: &HashMap<&[u8], usize>, serial: u32) {
    let open = |r: &Row<'_>| !r.line.is_done();
    for (item, row) in lines.items.iter_mut().zip(&lines.rows) {
        for &dep in lines.deps.get(row.deps.clone()).unwrap_or_default() {
            let Some(&pre) = first.get(dep) else {
                item.dangling.extend(tag_value(row.line, b"dep", dep));
                continue;
            };
            if open(row) && lines.rows.get(pre).is_some_and(open) {
                item.blockers.push(ItemId::new(pre, serial));
            }
        }
    }
}

fn date(text: Option<&str>) -> Option<Date> {
    Date::parse(text?)
}

fn is_stale(claim: Claim, started: Option<Timestamp>, when: &When) -> bool {
    claim == Claim::Agent
        && started.is_some_and(|started| {
            when.now.duration_since(started) > when.stale_after
        })
}

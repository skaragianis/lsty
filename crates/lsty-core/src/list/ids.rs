//! The id index: item indices sorted by id, then by position, so one id's
//! items are a contiguous run in file order.

use std::path::Path;

use super::snapshot::{Item, Snapshot};
use crate::errs::{Code, Error, Field, Ref, Result};
use crate::fsio;

/// A warning and the id it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Warned {
    id: Vec<u8>,
    text: String,
}

impl Warned {
    #[cfg(test)]
    pub(super) fn text(&self) -> &str {
        &self.text
    }
}

/// The indices of the items that have an id, sorted by id, then index.
pub(super) fn sorted(ids: &[&[u8]]) -> Vec<usize> {
    let mut order: Vec<usize> = ids
        .iter()
        .enumerate()
        .filter(|(_, id)| !id.is_empty())
        .map(|(i, _)| i)
        .collect();
    order.sort_unstable_by(|&a, &b| {
        let (ida, idb) = (ids.get(a), ids.get(b));
        ida.cmp(&idb).then(a.cmp(&b))
    });
    order
}

/// The run of `order` whose items carry `id`, in file order.
pub(super) fn run<'k>(
    order: &[usize],
    id: &[u8],
    key: impl Fn(usize) -> &'k [u8],
) -> std::ops::Range<usize> {
    let lo = order.partition_point(|&i| key(i) < id);
    let hi = order.partition_point(|&i| key(i) <= id);
    lo..hi
}

/// Counts of the items of one id in each file.
struct Group<'a> {
    id: &'a [u8],
    first: usize,
    todo: usize,
    done: usize,
}

fn groups<'a>(
    order: &[usize],
    todo_items: usize,
    key: impl Fn(usize) -> &'a [u8],
) -> Vec<Group<'a>> {
    let mut out: Vec<Group<'a>> = Vec::new();
    for &i in order {
        let id = key(i);
        match out.last_mut() {
            Some(g) if g.id == id => bump(g, i, todo_items),
            _ => {
                let mut g = Group {
                    id,
                    first: i,
                    todo: 0,
                    done: 0,
                };
                bump(&mut g, i, todo_items);
                out.push(g);
            }
        }
    }
    out.sort_unstable_by_key(|g| g.first);
    out
}

fn bump(group: &mut Group<'_>, index: usize, todo_items: usize) {
    if index < todo_items {
        group.todo += 1;
    } else {
        group.done += 1;
    }
}

fn base_name(path: &Path) -> String {
    fsio::base_name(path).to_string_lossy().into_owned()
}

/// The warnings of both files' ids: first those in both files, then those
/// repeated within a file, each in order of first appearance.
pub(super) fn warnings<'a>(
    order: &[usize],
    todo_items: usize,
    names: [&Path; 2],
    key: impl Fn(usize) -> &'a [u8],
) -> Vec<Warned> {
    let [todo, done] = names.map(base_name);
    let groups = groups(order, todo_items, key);
    let warned = |g: &Group<'_>, text: String| Warned {
        id: g.id.to_vec(),
        text,
    };
    let mut out = Vec::new();
    for g in groups.iter().filter(|g| g.todo > 0 && g.done > 0) {
        let id = String::from_utf8_lossy(g.id);
        let text =
            format!("id:{id} is in {todo} and {done}; using the {todo} copy");
        out.push(warned(g, text));
    }
    for g in &groups {
        let id = String::from_utf8_lossy(g.id);
        for (n, name) in [(g.todo, &todo), (g.done, &done)] {
            if n >= 2 {
                let text = format!("id:{id} is on {n} lines of {name}");
                out.push(warned(g, text));
            }
        }
    }
    out
}

/// The warnings about any of `ids`.
pub(super) fn warnings_for(warned: &[Warned], ids: &[&[u8]]) -> Vec<String> {
    warned
        .iter()
        .filter(|w| ids.contains(&w.id.as_slice()))
        .map(|w| w.text.clone())
        .collect()
}

impl Snapshot {
    /// The indices of the items that carry `id`, in file order.
    fn with_id(&self, id: &[u8]) -> &[usize] {
        let found = run(&self.by_id, id, |i| self.id_at(i));
        self.by_id.get(found).unwrap_or_default()
    }

    /// Whether a line of either file carries `id`, as its first `id:`
    /// tag or as a later one.
    pub(crate) fn id_in_use(&self, id: &[u8]) -> bool {
        !self.with_id(id).is_empty()
            || self
                .extra_ids
                .binary_search_by(|e| e.as_slice().cmp(id))
                .is_ok()
    }

    /// Every item that carries `id`, in file order, the todo file first.
    pub(crate) fn items_with_id<'a>(
        &'a self,
        id: &[u8],
    ) -> impl Iterator<Item = &'a Item> + use<'a> {
        let found = self.with_id(id);
        found.iter().filter_map(|&i| self.items.get(i))
    }

    /// The item that carries `id`. An id on one line of each file gives
    /// the todo file's item; an id on several lines of a file is
    /// `ambiguous`; an id on no line is `no_item`.
    pub fn by_id(&self, id: impl AsRef<[u8]>) -> Result<&Item> {
        let id = id.as_ref();
        let found = self.with_id(id);
        let Some(&first) = found.first() else {
            return Err(no_item(id));
        };
        let split = found.partition_point(|&i| i < self.todo_items);
        let (todo, done) = found.split_at(split);
        for group in [todo, done] {
            if group.len() > 1 {
                return Err(self.ambiguous(id, group));
            }
        }
        self.items.get(first).ok_or_else(|| no_item(id))
    }

    fn ambiguous(&self, id: &[u8], group: &[usize]) -> Error {
        let text = String::from_utf8_lossy(id).into_owned();
        let refs = group
            .iter()
            .filter_map(|&i| self.items.get(i))
            .map(|item| Ref {
                nr: item.nr,
                source: item.source,
                id: Some(text.clone()),
            })
            .collect();
        Error::new(
            Code::Ambiguous,
            format!("id:{text} is on {} lines", group.len()),
        )
        .with_field("id", Field::Str(text))
        .with_field("refs", Field::Refs(refs))
    }
}

fn no_item(id: &[u8]) -> Error {
    let text = String::from_utf8_lossy(id).into_owned();
    Error::new(Code::NoItem, format!("no item id:{text}"))
        .with_field("id", Field::Str(text))
}

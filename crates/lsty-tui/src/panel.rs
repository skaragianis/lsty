//! A panel of the sidebar: its entries, its cursor and its view.

use std::collections::{BTreeMap, BTreeSet};

use lsty_core::list::Count;
use lsty_core::theme::Slot;
use lsty_core::todotxt::name_parent;

use crate::text::{Span, blanks, fit, pad, width};

/// The columns of the marker that stands before a label.
const MARKER: usize = 1;
/// The columns between a label and its count.
const COUNT_GAP: usize = 1;
/// The columns that a level of the tree is indented by.
const INDENT: usize = 2;
const CHEVRON_OPEN: &str = "▾";
const CHEVRON_SHUT: &str = "▸";
const RULE: &str = "─";

/// One row of a panel.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    label: Vec<Span<'static>>,
    count: String,
    /// The full name of a tree node; empty for the other entries.
    name: Vec<u8>,
    parent: bool,
    rule: bool,
}

impl Entry {
    /// The line between the groups of the Filters panel.
    pub(crate) const fn rule() -> Self {
        Self {
            label: Vec::new(),
            count: String::new(),
            name: Vec::new(),
            parent: false,
            rule: true,
        }
    }

    /// An entry that is a line of text, such as a toggle or a filter.
    pub(crate) fn text(text: impl Into<String>) -> Self {
        let label = vec![Span::new(text.into(), Some(Slot::Text))];
        Self {
            label,
            count: String::new(),
            name: Vec::new(),
            parent: false,
            rule: false,
        }
    }

    /// The cells the entry wants: marker, label and count.
    fn natural(&self) -> usize {
        if self.rule {
            return 0;
        }
        let count = if self.count.is_empty() {
            0
        } else {
            COUNT_GAP + Span::new(self.count.as_str(), None).cells()
        };
        MARKER + width(&self.label) + count
    }

    /// The spans of the entry, `inner` cells wide before padding.
    pub(crate) fn spans(&self, inner: usize) -> Vec<Span<'_>> {
        if self.rule {
            let dashes = RULE.repeat(inner.saturating_sub(MARKER + 1));
            return vec![
                Span::new(" ", None),
                Span::new(dashes, Some(Slot::Muted)),
            ];
        }
        let count = Span::new(self.count.as_str(), Some(Slot::Muted));
        let gap = if self.count.is_empty() { 0 } else { COUNT_GAP };
        let room = inner.saturating_sub(MARKER + count.cells() + gap);
        let label = fit(self.label.clone(), room);
        let blank =
            inner.saturating_sub(MARKER + width(&label) + count.cells());
        let mut out = vec![Span::new(" ", None)];
        out.extend(label);
        out.push(Span::blanks(blank, None));
        out.push(count);
        out
    }
}

/// The names whose parent is the key, in byte order.
type Kids<'a> = BTreeMap<&'a [u8], Vec<&'a [u8]>>;

/// How an entry of a tree is labelled, indented and counted.
struct Tree<'a> {
    sigil: &'static str,
    nodes: &'a BTreeMap<Vec<u8>, Count>,
    folded: &'a BTreeSet<Vec<u8>>,
    kids: Kids<'a>,
}

impl<'a> Tree<'a> {
    fn new(
        sigil: &'static str,
        nodes: &'a BTreeMap<Vec<u8>, Count>,
        folded: &'a BTreeSet<Vec<u8>>,
    ) -> Self {
        let mut kids = Kids::new();
        for name in nodes.keys() {
            kids.entry(name_parent(name)).or_default().push(name);
        }
        Self {
            sigil,
            nodes,
            folded,
            kids,
        }
    }

    /// The entries, each parent followed by its children, as far as the
    /// folds let them show.
    fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        let top = self.kids.get(&b""[..]).map_or(&[][..], Vec::as_slice);
        let mut stack = vec![(top.iter(), 0_usize)];
        while let Some((names, depth)) = stack.last_mut() {
            let depth = *depth;
            let Some(&name) = names.next() else {
                stack.pop();
                continue;
            };
            out.push(self.entry(name, depth));
            if self.folded.contains(name) {
                continue;
            }
            if let Some(kids) = self.kids.get(name) {
                stack.push((kids.iter(), depth + 1));
            }
        }
        out
    }

    fn entry(&self, name: &[u8], depth: usize) -> Entry {
        let parent = name_parent(name);
        let tail = name.get(parent.len()..).unwrap_or_default();
        let text = String::from_utf8_lossy(tail);
        let label = if parent.is_empty() {
            format!("{}{text}", self.sigil)
        } else {
            text.into_owned()
        };
        let mut spans = Vec::new();
        if depth > 0 {
            spans.push(Span::blanks(INDENT * depth, None));
        }
        let has_kids = self.kids.contains_key(name);
        if has_kids {
            spans.push(Span::new(self.chevron(name), Some(Slot::Muted)));
        }
        spans.push(Span::new(label, Some(Slot::Text)));
        let count = self.nodes.get(name).copied().unwrap_or_default();
        Entry {
            label: spans,
            count: format!("{}/{}", count.open, count.total),
            name: name.to_vec(),
            parent: has_kids,
            rule: false,
        }
    }

    fn chevron(&self, name: &[u8]) -> &'static str {
        if self.folded.contains(name) {
            CHEVRON_SHUT
        } else {
            CHEVRON_OPEN
        }
    }
}

/// One of the three lists of the sidebar.
#[derive(Debug)]
pub(crate) struct Panel {
    sigil: &'static str,
    nodes: BTreeMap<Vec<u8>, Count>,
    folded: BTreeSet<Vec<u8>>,
    pub(crate) entries: Vec<Entry>,
    /// The widest entry, with every tree unfolded.
    pub(crate) natural: usize,
    pub(crate) cursor: usize,
    pub(crate) top: usize,
}

impl Panel {
    /// A tree whose top level names carry `sigil`.
    pub(crate) fn tree(sigil: &'static str) -> Self {
        Self {
            sigil,
            ..Self::list()
        }
    }

    /// A plain list.
    pub(crate) fn list() -> Self {
        Self {
            sigil: "",
            nodes: BTreeMap::new(),
            folded: BTreeSet::new(),
            entries: Vec::new(),
            natural: 0,
            cursor: 0,
            top: 0,
        }
    }

    /// Replaces the names of the tree and lays it out again.
    pub(crate) fn set_nodes(&mut self, nodes: BTreeMap<Vec<u8>, Count>) {
        self.nodes = nodes;
        let none = BTreeSet::new();
        let open = Tree::new(self.sigil, &self.nodes, &none).entries();
        self.natural = natural_of(&open);
        self.refill();
    }

    /// Replaces the entries of a plain list.
    pub(crate) fn set_entries(&mut self, entries: Vec<Entry>) {
        self.natural = natural_of(&entries);
        self.entries = entries;
    }

    fn refill(&mut self) {
        let tree = Tree::new(self.sigil, &self.nodes, &self.folded);
        self.entries = tree.entries();
    }

    fn selectable(&self) -> Vec<usize> {
        let rows = self.entries.iter().enumerate();
        rows.filter(|(_, e)| !e.rule).map(|(i, _)| i).collect()
    }

    /// Moves the cursor by `n` entries, skipping rules.
    pub(crate) fn move_by(&mut self, n: i64) {
        let sel = self.selectable();
        let Some(last) = sel.len().checked_sub(1) else {
            return;
        };
        let at = sel.iter().position(|&i| i == self.cursor);
        let at = at.map_or(-1, |p| i64::try_from(p).unwrap_or(i64::MAX));
        let to = at
            .saturating_add(n)
            .clamp(0, i64::try_from(last).unwrap_or(0));
        let to = usize::try_from(to).unwrap_or(0);
        self.cursor = sel.get(to).copied().unwrap_or(self.cursor);
    }

    /// Goes to entry number `count`, to the last entry when `end` is set
    /// and there is no count, and to the first one otherwise.
    pub(crate) fn jump(&mut self, count: usize, end: bool) {
        let sel = self.selectable();
        let Some(last) = sel.len().checked_sub(1) else {
            return;
        };
        let to = if count > 0 {
            (count - 1).min(last)
        } else if end {
            last
        } else {
            0
        };
        self.cursor = sel.get(to).copied().unwrap_or(self.cursor);
    }

    /// Moves the view the least amount that shows the cursor entry.
    pub(crate) fn scroll(&mut self, rows: usize) {
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + rows {
            self.top = (self.cursor + 1).saturating_sub(rows);
        }
        self.top = self.top.min(self.entries.len().saturating_sub(rows));
    }

    /// Keeps the cursor on an entry.
    pub(crate) fn clamp(&mut self) {
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    /// Folds the node under the cursor, or the parent of a leaf. Returns
    /// whether a node changed.
    pub(crate) fn fold(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.cursor) else {
            return false;
        };
        let target = if entry.parent {
            entry.name.clone()
        } else {
            name_parent(&entry.name).to_vec()
        };
        if target.is_empty() {
            return false;
        }
        self.folded.insert(target.clone());
        self.settle(&target);
        true
    }

    /// Unfolds the node under the cursor.
    pub(crate) fn unfold(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.cursor) else {
            return false;
        };
        if !entry.parent {
            return false;
        }
        let target = entry.name.clone();
        self.folded.remove(&target);
        self.settle(&target);
        true
    }

    /// Lays the tree out again and puts the cursor on `name`.
    fn settle(&mut self, name: &[u8]) {
        self.refill();
        let at = self.entries.iter().position(|e| e.name == name);
        self.cursor = at.unwrap_or(self.cursor);
    }
}

fn natural_of(entries: &[Entry]) -> usize {
    entries.iter().map(Entry::natural).max().unwrap_or(0)
}

/// Spans of one row of `entry`, padded to `inner` cells.
pub(crate) fn row_spans(entry: &Entry, inner: usize) -> Vec<Span<'_>> {
    let mut spans = entry.spans(inner);
    pad(&mut spans, inner, None);
    spans
}

/// Blank cells for a row that holds no entry.
pub(crate) fn empty_row(inner: usize) -> Vec<Span<'static>> {
    vec![Span::new(blanks(inner), None)]
}

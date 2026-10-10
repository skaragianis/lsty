//! A panel of the sidebar: its entries, its cursor and its view.

use std::collections::{BTreeMap, BTreeSet};

use lsty_core::config::Hidden;
use lsty_core::list::Count;
use lsty_core::theme::Slot;
use lsty_core::todotxt::name_parent;

use crate::text::{RULE, Span, blanks, fit, pad, width, width_of, window};

/// The columns of the marker that stands before a label.
const MARKER: usize = 1;
/// The columns between a label and its count.
const COUNT_GAP: usize = 1;
/// The columns that a level of the tree is indented by.
const INDENT: usize = 2;
const CHEVRON_OPEN: &str = "▾";
const CHEVRON_SHUT: &str = "▸";

/// One row of a panel.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    label: Vec<Span<'static>>,
    count: String,
    kind: Kind,
    /// Whether the entry is the one that filters the list.
    active: bool,
}

/// What an entry is.
#[derive(Clone, Debug)]
enum Kind {
    /// The line between the groups of the Filters panel.
    Rule,
    /// A name of a tree; `parent` when it has children.
    Node { name: Vec<u8>, parent: bool },
    /// A line of text that can be the panel's active entry.
    Filter { name: Vec<u8> },
    /// A line of text with a checkbox for a show switch.
    Toggle(Hidden),
}

impl Entry {
    /// The line between the groups of the Filters panel.
    pub(crate) const fn rule() -> Self {
        Self {
            label: Vec::new(),
            count: String::new(),
            kind: Kind::Rule,
            active: false,
        }
    }

    /// A filter: a line of text that can be the panel's active entry.
    pub(crate) fn filter(name: &str, active: bool) -> Self {
        let label = vec![Span::new(name.to_owned(), Some(Slot::Text))];
        Self {
            label,
            count: String::new(),
            kind: Kind::Filter {
                name: name.as_bytes().to_vec(),
            },
            active,
        }
    }

    /// A show toggle, a line of text with a checkbox.
    pub(crate) fn toggle(what: Hidden, text: String) -> Self {
        let label = vec![Span::new(text, Some(Slot::Text))];
        Self {
            label,
            count: String::new(),
            kind: Kind::Toggle(what),
            active: false,
        }
    }

    /// Whether `other` is the same entry: the same name, or the same
    /// switch.
    fn is_same(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (Kind::Toggle(this), Kind::Toggle(that)) => this == that,
            _ => !self.name().is_empty() && self.name() == other.name(),
        }
    }

    /// The show toggle the entry switches, when it is one.
    pub(crate) const fn toggles(&self) -> Option<Hidden> {
        match self.kind {
            Kind::Toggle(what) => Some(what),
            Kind::Rule | Kind::Node { .. } | Kind::Filter { .. } => None,
        }
    }

    /// Whether the entry is a line between groups.
    pub(crate) const fn is_rule(&self) -> bool {
        matches!(self.kind, Kind::Rule)
    }

    /// Whether the entry is a name with children.
    pub(crate) const fn is_parent(&self) -> bool {
        matches!(self.kind, Kind::Node { parent: true, .. })
    }

    /// The column of the fold mark of a node, counted from the first
    /// cell of the row; `None` for an entry without children.
    pub(crate) fn chevron_col(&self) -> Option<usize> {
        if !self.is_parent() {
            return None;
        }
        let indent = match self.label.as_slice() {
            [lead, _, ..] if lead.slot.is_none() => lead.cells(),
            _ => 0,
        };
        Some(MARKER + indent)
    }

    /// The name that selecting the entry makes active.
    pub(crate) fn name(&self) -> &[u8] {
        match &self.kind {
            Kind::Node { name, .. } | Kind::Filter { name } => name,
            Kind::Rule | Kind::Toggle(_) => &[],
        }
    }

    /// The cells the entry wants: marker, label and count.
    fn natural(&self) -> usize {
        if self.is_rule() {
            return 0;
        }
        let count = if self.count.is_empty() {
            0
        } else {
            COUNT_GAP + width_of(&self.count)
        };
        MARKER + width(&self.label) + count
    }

    /// The label, in the selection style when the entry is active.
    fn shown(&self) -> Vec<Span<'static>> {
        let mut label = self.label.clone();
        if self.active {
            for span in label.iter_mut().filter(|s| s.slot.is_some()) {
                span.slot = Some(Slot::Selection);
            }
        }
        label
    }

    /// The spans of the entry, `inner` cells wide before padding.
    pub(crate) fn spans(&self, inner: usize) -> Vec<Span<'_>> {
        if self.is_rule() {
            let dashes = RULE.repeat(inner.saturating_sub(MARKER + 1));
            return vec![
                Span::new(" ", None),
                Span::new(dashes, Some(Slot::Muted)),
            ];
        }
        let count = Span::new(self.count.as_str(), Some(Slot::Muted));
        let gap = if self.count.is_empty() { 0 } else { COUNT_GAP };
        let room = inner.saturating_sub(MARKER + count.cells() + gap);
        let label = fit(self.shown(), room);
        let blank =
            inner.saturating_sub(MARKER + width(&label) + count.cells());
        let mark = if self.active {
            Span::new(">", Some(Slot::Selection))
        } else {
            Span::new(" ", None)
        };
        let mut out = vec![mark];
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
    active: Option<&'a [u8]>,
    nodes: &'a BTreeMap<Vec<u8>, Count>,
    folded: &'a BTreeSet<Vec<u8>>,
    kids: Kids<'a>,
}

impl<'a> Tree<'a> {
    fn new(
        sigil: &'static str,
        active: Option<&'a [u8]>,
        nodes: &'a BTreeMap<Vec<u8>, Count>,
        folded: &'a BTreeSet<Vec<u8>>,
    ) -> Self {
        let mut kids = Kids::new();
        for name in nodes.keys() {
            kids.entry(name_parent(name)).or_default().push(name);
        }
        Self {
            sigil,
            active,
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
        let active = self.active == Some(name);
        let count = self.nodes.get(name).copied().unwrap_or_default();
        Entry {
            label: spans,
            count: format!("{}/{}", count.open, count.total),
            kind: Kind::Node {
                name: name.to_vec(),
                parent: has_kids,
            },
            active,
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
    /// The name that filters the list, if any.
    pub(crate) active: Option<Vec<u8>>,
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

    /// What stands before the names of the top level.
    pub(crate) const fn sigil(&self) -> &'static str {
        self.sigil
    }

    /// A plain list.
    pub(crate) fn list() -> Self {
        Self {
            sigil: "",
            nodes: BTreeMap::new(),
            folded: BTreeSet::new(),
            active: None,
            entries: Vec::new(),
            natural: 0,
            cursor: 0,
            top: 0,
        }
    }

    /// Replaces the names of the tree and lays it out again.
    /// The active name stays listed, with the names above it, when no
    /// item has it.
    pub(crate) fn set_nodes(&mut self, mut nodes: BTreeMap<Vec<u8>, Count>) {
        let mut name = self.active.as_deref().unwrap_or_default();
        while !name.is_empty() {
            nodes.entry(name.to_vec()).or_default();
            name = name_parent(name);
        }
        self.nodes = nodes;
        let none = BTreeSet::new();
        let active = self.active.as_deref();
        let open = Tree::new(self.sigil, active, &self.nodes, &none).entries();
        self.natural = natural_of(&open);
        self.refill();
    }

    /// Replaces the entries of a plain list.
    pub(crate) fn set_entries(&mut self, entries: Vec<Entry>) {
        self.natural = natural_of(&entries);
        self.entries = entries;
    }

    fn refill(&mut self) {
        let active = self.active.as_deref();
        let tree = Tree::new(self.sigil, active, &self.nodes, &self.folded);
        self.entries = tree.entries();
    }

    fn selectable(&self) -> Vec<usize> {
        let rows = self.entries.iter().enumerate();
        rows.filter(|(_, e)| !e.is_rule()).map(|(i, _)| i).collect()
    }

    /// Moves the cursor by `n` entries, skipping rules.
    pub(crate) fn move_by(&mut self, n: isize) {
        let sel = self.selectable();
        let Some(last) = sel.len().checked_sub(1) else {
            return;
        };
        let at = sel.iter().position(|&i| i == self.cursor);
        let to = at.map_or(0, |p| p + 1).saturating_add_signed(n);
        let to = to.saturating_sub(1).min(last);
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
        self.top = window(self.top, self.cursor, self.entries.len(), rows);
    }

    /// Puts the cursor back on the entry it was on, found by name among
    /// the entries now, or on the next entry that is still there. `was`
    /// are the entries the cursor stood among.
    pub(crate) fn land(&mut self, was: &[Entry], cursor: usize) {
        let last = self.entries.len().saturating_sub(1);
        let after = was.get(cursor..).unwrap_or_default();
        let found = after
            .iter()
            .filter(|e| !e.is_rule())
            .find_map(|e| self.entries.iter().position(|now| now.is_same(e)));
        let past = if cursor < was.len() {
            last
        } else {
            cursor.min(last)
        };
        self.cursor = found.unwrap_or(past);
    }

    /// Keeps the cursor on an entry.
    pub(crate) fn clamp(&mut self) {
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    /// Folds the nodes named `names`.
    pub(crate) fn set_folded(&mut self, names: &[String]) {
        self.folded = names.iter().map(|n| n.as_bytes().to_vec()).collect();
    }

    /// The folded nodes that exist and are text, in order.
    pub(crate) fn folded_names(&self) -> Vec<String> {
        self.folded
            .iter()
            .filter(|name| self.nodes.contains_key(*name))
            .filter_map(|name| String::from_utf8(name.clone()).ok())
            .collect()
    }

    /// Puts the cursor on entry `at` and folds the node there, or
    /// unfolds it when it is folded. Returns whether a node changed.
    pub(crate) fn flip_fold(&mut self, at: usize) -> bool {
        self.cursor = at;
        let Some(entry) = self.entries.get(at) else {
            return false;
        };
        if !entry.is_parent() {
            return false;
        }
        if self.folded.contains(entry.name()) {
            self.unfold()
        } else {
            self.fold()
        }
    }

    /// Folds the node under the cursor, or the parent of a leaf. Returns
    /// whether a node changed.
    pub(crate) fn fold(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.cursor) else {
            return false;
        };
        let target = if entry.is_parent() {
            entry.name().to_vec()
        } else {
            name_parent(entry.name()).to_vec()
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
        if !entry.is_parent() {
            return false;
        }
        let target = entry.name().to_vec();
        self.folded.remove(&target);
        self.settle(&target);
        true
    }

    /// Lays the tree out again and puts the cursor on `name`.
    fn settle(&mut self, name: &[u8]) {
        self.refill();
        let at = self.entries.iter().position(|e| e.name() == name);
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

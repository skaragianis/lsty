//! The detail panel: everything about the task under the cursor.

use std::borrow::Cow;

use lsty_core::{
    dates::Date,
    errs::{Source, lossy},
    links::{self, Link},
    list::{State, Task},
    theme::Slot,
    todotxt::{Kind, Line, Tags, TokenView, is_structural},
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    focus::Focus,
    layout::{BORDER, cells},
    model::Model,
    open::exists_mark,
    reldate::due_slot,
    rows::{base_slot, token_slot},
    text::{ELLIPSIS, RULE, Span, cut, pad, put, titled, width_of},
    wrap::wrap_words,
};

/// The title, numbered as the key that focuses the panel.
const TITLE: &str = "[2]─Detail";
pub(crate) const PADDING: usize = 1;
const LABEL_GAP: usize = 2;

/// One line of the metadata: a label and a value. A wide field has no
/// label column.
struct Field<'a> {
    label: Cow<'a, str>,
    value: Span<'a>,
    wide: bool,
}

impl<'a> Field<'a> {
    fn new(label: impl Into<Cow<'a, str>>, value: Span<'a>) -> Self {
        Self {
            label: label.into(),
            value,
            wide: false,
        }
    }

    fn text(label: &'static str, value: impl Into<String>) -> Self {
        Self::new(label, plain(value.into()))
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            label: Cow::Borrowed(""),
            value: Span::new(message.into(), Some(Slot::Warning)),
            wide: true,
        }
    }
}

fn plain<'a>(value: impl Into<Cow<'a, str>>) -> Span<'a> {
    Span::new(value, Some(Slot::Text))
}

/// `(1d overdue)`, `(today)`, `(tomorrow)` or `(in 3d)`.
fn relative(days: i32) -> String {
    match days {
        ..=-1 => format!("({}d overdue)", days.unsigned_abs()),
        0 => "(today)".to_owned(),
        1 => "(tomorrow)".to_owned(),
        _ => format!("(in {days}d)"),
    }
}

/// The words with a breakable space between each two.
fn spaced(words: Vec<Span<'_>>) -> Vec<Span<'_>> {
    let mut out = Vec::with_capacity(2 * words.len());
    for word in words {
        if !out.is_empty() {
            out.push(Span::new(" ", Some(Slot::Text)));
        }
        out.push(word);
    }
    out
}

/// A value split at white space into words of its slot, wrapped.
fn wrap_value(value: &Span<'_>, w: usize) -> Vec<Vec<Span<'static>>> {
    let words: Vec<_> = value
        .text
        .split_whitespace()
        .map(|word| Span::new(word, value.slot))
        .collect();
    wrap_words(&spaced(words), |_| w)
}

fn field_lines<'a>(
    f: &Field<'_>,
    column: usize,
    w: usize,
) -> Vec<Vec<Span<'a>>> {
    if f.wide {
        return wrap_value(&f.value, w);
    }
    let mut lines = wrap_value(&f.value, w.saturating_sub(column).max(1));
    for (i, line) in lines.iter_mut().enumerate() {
        let label = if i == 0 {
            let pad = column.saturating_sub(width_of(&f.label));
            let text = format!("{}{}", f.label, " ".repeat(pad));
            Span::new(text, Some(Slot::Muted))
        } else {
            Span::blanks(column, None)
        };
        line.insert(0, label);
    }
    lines
}

/// What the panel holds: its lines, how many of them are the
/// description, and where the `files` heading is.
pub(crate) struct Doc<'a> {
    pub(crate) lines: Vec<Vec<Span<'a>>>,
    pub(crate) desc: usize,
    pub(crate) files: Option<usize>,
}

/// The cells of content a line holds and the lines of a panel `area`
/// wide and high.
pub(crate) fn detail_size(area: Rect) -> (usize, usize) {
    let inner = usize::from(area.width).saturating_sub(BORDER);
    let rows = usize::from(area.height).saturating_sub(BORDER);
    (inner.saturating_sub(2 * PADDING), rows)
}

/// The first line of the last view of `n` lines in `h`.
pub(crate) const fn detail_last(n: usize, h: usize) -> usize {
    if n <= h { 0 } else { n - h + 1 }
}

/// Which line of the document each of `h` panel lines shows when the
/// view starts at line `top`; `None` is the mark of cut content.
pub(crate) fn detail_rows(
    n: usize,
    h: usize,
    top: usize,
) -> Vec<Option<usize>> {
    let top = top.min(detail_last(n, h));
    let mut room = h;
    let mut out = Vec::new();
    if top > 0 {
        out.push(None);
        room = h.saturating_sub(1);
    }
    if n.saturating_sub(top) <= room {
        out.extend((top..n).map(Some));
        return out;
    }
    out.extend((top..top + room.saturating_sub(1)).map(Some));
    out.push(None);
    out
}

impl Model {
    /// Draws the detail panel in `area`.
    pub(crate) fn draw_detail(&self, buf: &mut Buffer, area: Rect) {
        let w = usize::from(area.width);
        let inner = w.saturating_sub(BORDER);
        let (content, rows) = detail_size(area);
        let slot = Some(self.edge(&[Focus::Detail]));
        let top = titled("┌", TITLE, "┐", w);
        let at = |y: usize| area.y + cells(y);
        let put_line = |buf: &mut Buffer, y: usize, text: String| {
            let span = Span::new(text, slot);
            put(buf, (area.x, at(y)), w, &[span], &self.styles);
        };
        put_line(buf, 0, top);
        let doc = self.detail_doc(content, rows);
        let shown = detail_rows(doc.lines.len(), rows, self.detail_top());
        let selected = self.selected_line(&doc);
        let side = Span::new("│", slot);
        let right = area.x + area.width.saturating_sub(1);
        for i in 0..rows {
            let y = at(1 + i);
            for x in [area.x, right] {
                let sides = std::slice::from_ref(&side);
                put(buf, (x, y), 1, sides, &self.styles);
            }
            let spans = match shown.get(i) {
                Some(Some(n)) => doc.lines.get(*n).cloned(),
                Some(None) => {
                    Some(vec![Span::new(ELLIPSIS, Some(Slot::Muted))])
                }
                None => continue,
            };
            let mut spans = cut(spans.unwrap_or_default(), content);
            pad(&mut spans, content, None);
            let spans = [vec![Span::blanks(PADDING, None)], spans].concat();
            put(buf, (area.x + 1, y), inner, &spans, &self.styles);
            if selected.is_some_and(|n| shown.get(i) == Some(&Some(n))) {
                let row = Rect::new(area.x + 1, y, cells(inner), 1);
                buf.set_style(row, self.styles.fill(Slot::Cursorline));
            }
        }
        let bottom = format!("└{}┘", RULE.repeat(inner));
        put_line(buf, rows + 1, bottom);
    }

    /// The line of the link that the focused panel has selected.
    fn selected_line(&self, doc: &Doc<'_>) -> Option<usize> {
        let files = doc.files.filter(|_| self.focus == Focus::Detail)?;
        Some(files + 1 + self.detail_link())
    }

    /// The document of the task under the cursor, `w` cells wide. Empty
    /// when there is no task.
    pub(crate) fn detail_doc(&self, w: usize, rows: usize) -> Doc<'_> {
        let mut doc = Doc {
            lines: Vec::new(),
            desc: 0,
            files: None,
        };
        let Some(task) = self
            .rows
            .get(self.cursor)
            .and_then(|r| self.snap.task(r.id))
        else {
            return doc;
        };
        if rows == 0 {
            return doc;
        }
        let line = self.snap.line(task);
        doc.lines = wrap_words(&spaced(description(task, line)), |_| w);
        doc.desc = doc.lines.len();
        doc.lines
            .push(vec![Span::new(RULE.repeat(w), Some(Slot::Muted))]);
        doc.lines.extend(self.meta_lines(task, line, w));
        let files = self.link_lines(task);
        if !files.is_empty() {
            doc.files = Some(doc.lines.len());
            doc.lines.extend(files);
        }
        doc
    }

    fn meta_lines<'a>(
        &'a self,
        task: &'a Task,
        line: &'a Line,
        w: usize,
    ) -> Vec<Vec<Span<'a>>> {
        let tags = line.tags();
        let mut fields = vec![Field::text("state", task.state().as_str())];
        if task.source() == Source::Todo {
            fields.push(Field::text("nr", task.nr().to_string()));
        }
        if let Some(pri) = line.effective_priority() {
            fields.push(Field::text("priority", pri.to_string()));
        }
        fields.extend(self.date_fields(task, line, &tags));
        fields.extend(claim_fields(task, &tags));
        fields.extend(self.relation_fields(task));
        fields.extend(tag_fields(task, line, &tags));
        let column = label_column(&fields);
        fields
            .iter()
            .flat_map(|f| field_lines(f, column, w))
            .collect()
    }

    fn date_fields<'a>(
        &self,
        task: &Task,
        line: &Line,
        tags: &Tags<'_>,
    ) -> Vec<Field<'a>> {
        let mut out = Vec::new();
        out.extend(line.created().map(|d| Field::text("created", d)));
        out.extend(line.completed().map(|d| Field::text("completed", d)));
        out.extend(self.due_fields(task.is_open(), tags.due));
        if let Some(t) = tags.threshold {
            out.push(Field::text("t", task.threshold_text(t)));
        }
        out.extend(rec_field(line, tags));
        out.extend(self.started_field(task));
        out.extend(
            task.ended_text(&self.clock)
                .map(|t| Field::text("ended", t)),
        );
        out
    }

    fn due_fields<'a>(&self, open: bool, due: Option<&str>) -> Vec<Field<'a>> {
        let Some(due) = due else {
            return Vec::new();
        };
        let Some(date) = Date::parse(due) else {
            return Vec::new();
        };
        let field = Field::text("due", due);
        if !open {
            return vec![field];
        }
        let days = self.clock.today().days_until(date);
        let soon = self.deps.config.upcoming_days;
        let slot = Some(due_slot(days, soon));
        vec![field, Field::new("", Span::new(relative(days), slot))]
    }

    fn started_field<'a>(&self, task: &Task) -> Option<Field<'a>> {
        let value = task.started_text(&self.clock, false)?;
        Some(Field::text("started", value))
    }

    fn relation_fields<'a>(&self, task: &Task) -> Vec<Field<'a>> {
        let mut out = Vec::new();
        for blocker in task.blockers() {
            let Some(other) = self.snap.task(*blocker) else {
                continue;
            };
            let text = self.snap.line(other).text();
            let text = lossy(&text);
            let nr = other.nr();
            out.push(Field::text("blocked by", format!("{text} ({nr})")));
        }
        for id in self.snap.dangling(task) {
            let id = lossy(id);
            out.push(Field::warning(format!("missing prerequisite {id}")));
        }
        if task.cycle() {
            out.push(Field::warning("dependency cycle"));
        }
        out
    }

    /// `links`, then one line for each link.
    fn link_lines<'a>(&'a self, task: &Task) -> Vec<Vec<Span<'a>>> {
        let links = links::links(&self.snap, task, self.deps.home.as_deref());
        if links.is_empty() {
            return Vec::new();
        }
        let mut out = vec![vec![Span::new("links", Some(Slot::Muted))]];
        for link in &links {
            let mark = match link {
                Link::File(file) => exists_mark(file.path.exists()),
                Link::Url(_) => " ",
            };
            out.push(vec![
                Span::new(format!("{mark} "), Some(Slot::Text)),
                Span::new(lossy(link.shown()), Some(Slot::Link)),
            ]);
        }
        out
    }
}

/// The label column: the longest label and a gap.
fn label_column(fields: &[Field<'_>]) -> usize {
    let longest = fields
        .iter()
        .filter(|f| !f.wide)
        .map(|f| width_of(&f.label));
    longest.max().unwrap_or(0) + LABEL_GAP
}

/// Every token of the description, in the colours of the list.
fn description<'a>(task: &Task, line: &'a Line) -> Vec<Span<'a>> {
    let base = base_slot(task);
    line.tokens()
        .map(|tok: TokenView<'a>| {
            let slot = Some(token_slot(&tok, base));
            Span::new(lossy(tok.text), slot)
        })
        .collect()
}

fn rec_field<'a>(line: &Line, tags: &Tags<'_>) -> Option<Field<'a>> {
    tags.rec?;
    let tok = line.tokens().find(|t| t.is_tag(b"rec"))?;
    Some(Field::text("rec", lossy(tok.value)))
}

fn claim_fields<'a>(task: &Task, tags: &Tags<'_>) -> Vec<Field<'a>> {
    let claim = task.claim_text(tags.owner).map(|text| {
        let stale = if task.stale() { ", stale" } else { "" };
        format!("{}{stale}", lossy(&text))
    });
    let id = tags.id.map(|id| Field::text("id", lossy(id)));
    claim
        .map(|claim| Field::text("claim", claim))
        .into_iter()
        .chain(id)
        .collect()
}

fn tag_fields<'a>(
    task: &Task,
    line: &'a Line,
    tags: &Tags<'a>,
) -> Vec<Field<'a>> {
    let mut out = Vec::new();
    let tokens = line.tokens().filter(|t| t.kind == Kind::Tag);
    for tok in
        tokens.filter(|t| !is_structural(t.key, task.state() == State::Done))
    {
        let key = lossy(tok.key);
        let value = lossy(tok.value);
        out.push(Field::new(key, plain(value)));
    }
    for tok in &tags.extra {
        if tok.key == b"pri" && task.state() != State::Done {
            continue;
        }
        let text = format!("{} (ignored)", lossy(tok.text));
        out.push(Field::new("extra", Span::new(text, Some(Slot::Warning))));
    }
    out
}

#[cfg(test)]
mod detail_tests;

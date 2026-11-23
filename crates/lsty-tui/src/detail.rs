//! The detail panel: everything about the task under the cursor.

use std::borrow::Cow;

use lsty_cli::place_name;
use lsty_core::{
    dates::Date,
    errs::{Source, lossy},
    links::{self, Link},
    list::{State, Task},
    theme::{Parts, Slot, Today, claim_slot, due_slot, state_slot},
    todotxt::{Kind, Line, Tags, is_structural},
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    focus::Focus,
    layout::{BORDER, FRAME, cells},
    model::Model,
    open::exists_mark,
    text::{ELLIPSIS, RULE, Span, cut, framed, pad, put, width, width_of},
    track::place_of,
    wrap::wrap_words,
};

/// The title, numbered as the key that focuses the panel.
const TITLE: &str = "[2]─Detail";
pub(crate) const PADDING: usize = 1;
const LABEL_GAP: usize = 2;

/// One line of the metadata: a label and the words of a value. A wide
/// field has no label column.
struct Field<'a> {
    label: Cow<'a, str>,
    words: Vec<Span<'static>>,
    wide: bool,
}

impl<'a> Field<'a> {
    /// A field whose value is `value` split at white space.
    fn new(label: impl Into<Cow<'a, str>>, value: Span<'_>) -> Self {
        let Span { text, slot, .. } = value;
        let words = text
            .split_whitespace()
            .map(|word| Span::new(word.to_owned(), slot))
            .collect();
        Self::words(label, words)
    }

    fn words(
        label: impl Into<Cow<'a, str>>,
        words: Vec<Span<'static>>,
    ) -> Self {
        Self {
            label: label.into(),
            words,
            wide: false,
        }
    }

    fn text(label: &'static str, value: impl Into<String>) -> Self {
        Self::new(label, plain(value.into()))
    }

    fn warning(message: impl Into<String>) -> Self {
        let mut field =
            Self::new("", Span::new(message.into(), Some(Slot::Warning)));
        field.wide = true;
        field
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
fn spaced<'a>(words: impl IntoIterator<Item = Span<'a>>) -> Vec<Span<'a>> {
    let mut out = Vec::new();
    for word in words {
        if !out.is_empty() {
            out.push(Span::new(" ", Some(Slot::Text)));
        }
        out.push(word);
    }
    out
}

/// The words of a value, wrapped.
fn wrap_value(f: &Field<'_>, w: usize) -> Vec<Vec<Span<'static>>> {
    wrap_words(&spaced(f.words.iter().cloned()), |_| w)
}

fn field_lines<'a>(
    f: &Field<'_>,
    column: usize,
    w: usize,
) -> Vec<Vec<Span<'a>>> {
    if f.wide {
        return wrap_value(f, w);
    }
    let mut lines = wrap_value(f, w.saturating_sub(column).max(1));
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
    let cells = usize::from(area.width).saturating_sub(FRAME);
    let rows = usize::from(area.height).saturating_sub(BORDER);
    (cells, rows)
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
    /// The title, then the cursor task's id when it fits whole in `w`.
    fn title_spans(&self, w: usize) -> Vec<Span<'_>> {
        let title = Some(self.title_slot(Focus::Detail));
        let mut spans = vec![Span::new(TITLE, title)];
        let id = self.cursor_task().and_then(|t| self.snap.line(t).tags().id);
        if let Some(id) = id {
            let text = format!("id:{}", lossy(id));
            let id = [
                Span::new(" \"", title),
                Span::new(text, Some(Slot::Id)),
                Span::new("\"", title),
            ];
            if width_of(TITLE) + width(&id) + BORDER <= w {
                spans.extend(id);
            }
        }
        spans
    }

    /// Draws the detail panel in `area`.
    pub(crate) fn draw_detail(&self, buf: &mut Buffer, area: Rect) {
        let w = usize::from(area.width);
        let inner = w.saturating_sub(BORDER);
        let (content, rows) = detail_size(area);
        let slot = Some(self.edge(&[Focus::Detail]));
        let top = framed(("┌", self.title_spans(w), "┐"), w, slot);
        let at = |y: usize| area.y + cells(y);
        let put_line = |buf: &mut Buffer, y: usize, text: String| {
            let span = Span::new(text, slot);
            put(buf, (area.x, at(y)), w, &[span], &self.styles);
        };
        put(buf, (area.x, area.y), w, &top, &self.styles);
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
            let current =
                selected.is_some_and(|n| shown.get(i) == Some(&Some(n)));
            if current {
                let row = Rect::new(area.x + 1, y, cells(inner), 1);
                buf.set_style(row, self.styles.fill(Slot::Cursorline));
            }
            let styles = self.styles.row(current);
            put(buf, (area.x + 1, y), inner, &spans, styles);
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
        let Some(task) = self.cursor_task() else {
            return doc;
        };
        if rows == 0 {
            return doc;
        }
        let line = self.snap.line(task);
        let words = description(task, line, self.today());
        doc.lines = wrap_words(&spaced(words), |_| w);
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
        let mut fields = vec![Field::new(
            "state",
            Span::new(task.state_word(), Some(state_slot(task))),
        )];
        if task.source() == Source::Todo {
            fields.push(Field::text("nr", task.nr().to_string()));
        }
        if let Some(pri) = line.effective_priority() {
            let slot = Some(Slot::of_priority(pri));
            fields.push(Field::new(
                "priority",
                Span::new(pri.to_string(), slot),
            ));
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
        if !open {
            return vec![Field::text("due", due)];
        }
        let days = self.clock.today().days_until(date);
        let slot = Some(due_slot(days, self.deps.config.upcoming_days));
        vec![
            Field::new("due", Span::new(due.to_owned(), slot)),
            Field::new("", Span::new(relative(days), slot)),
        ]
    }

    fn started_field<'a>(&self, task: &Task) -> Option<Field<'a>> {
        let value = task.started_text(&self.clock, false)?;
        Some(Field::new(
            "started",
            Span::new(value, Some(claim_slot(task))),
        ))
    }

    fn relation_fields<'a>(&self, task: &Task) -> Vec<Field<'a>> {
        let mut out = Vec::new();
        for blocker in task.blockers() {
            let Some(other) = self.snap.task(*blocker) else {
                continue;
            };
            let parts = Parts::of(other, Some(self.today()));
            let mut words: Vec<_> = self
                .snap
                .line(other)
                .tokens()
                .filter(|t| t.kind == Kind::Word)
                .map(|t| Span::new(lossy(t.text), Some(parts.slot(&t))))
                .collect();
            let place = self.row_at(place_of(other)).map(|i| self.place(i));
            words.push(Span::new(
                format!("({})", place_name(place, &self.snap, other)),
                Some(Slot::Text),
            ));
            out.push(Field::words("blocked by", words));
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

/// Every token of the description, in the colours of its parts: the
/// task's state does not colour it.
fn description<'a>(
    task: &Task,
    line: &'a Line,
    today: Today,
) -> Vec<Span<'a>> {
    let parts = Parts::of(task, Some(today));
    line.tokens()
        .map(|tok| Span::new(lossy(tok.text), Some(parts.slot(&tok))))
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
    let id = tags
        .id
        .map(|id| Field::new("id", Span::new(lossy(id), Some(Slot::Id))));
    let slot = Some(claim_slot(task));
    claim
        .map(|claim| Field::new("claim", Span::new(claim, slot)))
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

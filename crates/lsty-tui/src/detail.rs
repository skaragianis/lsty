//! The detail panel: everything about the item under the cursor.

use std::borrow::Cow;

use lsty_core::dates::{Date, format_elapsed};
use lsty_core::links::linked_files;
use lsty_core::list::{Claim, Item, State};
use lsty_core::theme::Slot;
use lsty_core::todotxt::{Kind, Line, Tags, TokenView, is_single_valued};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use crate::layout::BORDER;
use crate::model::Model;
use crate::open::exists_mark;
use crate::reldate::due_slot;
use crate::rows::{base_slot, token_slot};
use crate::text::{
    ELLIPSIS, RULE, Span, cut, pad, prefix, put, titled, width_of,
};

const TITLE: &str = "Detail";
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

/// A field for `label` that is there only when `value` is not empty.
fn labelled(label: &'static str, value: &str) -> Option<Field<'static>> {
    (!value.is_empty()).then(|| Field::text(label, value))
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

/// Breaks words into lines of at most `w` cells, between words where it
/// can and inside a word that is longer than a line.
fn wrap_words<'a>(words: Vec<Span<'a>>, w: usize) -> Vec<Vec<Span<'a>>> {
    let w = w.max(1);
    let mut lines = Vec::new();
    let mut line: Vec<Span<'a>> = Vec::new();
    let mut used = 0;
    for word in words {
        for piece in chunks(word, w) {
            let cells = piece.cells();
            if used > 0 && used + 1 + cells <= w {
                line.push(Span::new(" ", None));
                used += 1;
            } else if used > 0 {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(piece);
            used += cells;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// `word` in pieces of at most `w` cells.
fn chunks(word: Span<'_>, w: usize) -> Vec<Span<'_>> {
    let slot = word.slot;
    let mut out = Vec::new();
    let mut rest = word.text;
    while Span::new(rest.as_ref(), None).cells() > w {
        let head = head_of(&rest, w).len();
        let (head, tail) = split_cow(rest, head);
        out.push(Span::new(head, slot));
        rest = tail;
    }
    out.push(Span::new(rest, slot));
    out
}

/// The start of `text` that fits `w` cells: at least one cluster.
fn head_of(text: &str, w: usize) -> &str {
    let head = prefix(text, w);
    if head.is_empty() {
        text.graphemes(true).next().unwrap_or(text)
    } else {
        head
    }
}

fn split_cow(text: Cow<'_, str>, at: usize) -> (Cow<'_, str>, Cow<'_, str>) {
    match text {
        Cow::Borrowed(s) => {
            let (a, b) = s.split_at(at);
            (Cow::Borrowed(a), Cow::Borrowed(b))
        }
        Cow::Owned(s) => {
            let (a, b) = s.split_at(at);
            (Cow::Owned(a.to_owned()), Cow::Owned(b.to_owned()))
        }
    }
}

/// A value split at white space into words of its slot, wrapped.
fn wrap_value(value: &Span<'_>, w: usize) -> Vec<Vec<Span<'static>>> {
    let words = value
        .text
        .split_whitespace()
        .map(|word| Span::new(word.to_owned(), value.slot))
        .collect();
    wrap_words(words, w)
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
        let content = inner.saturating_sub(2 * PADDING);
        let rows = usize::from(area.height).saturating_sub(BORDER);
        let slot = Some(Slot::PanelBorder);
        let top = titled("┌", TITLE, "┐", w);
        let at = |y: usize| area.y + u16::try_from(y).unwrap_or(0);
        let put_line = |buf: &mut Buffer, y: usize, text: String| {
            let span = Span::new(text, slot);
            put(buf, (area.x, at(y)), w, &[span], &self.styles);
        };
        put_line(buf, 0, top);
        let doc = self.detail_doc(content, rows);
        let shown = detail_rows(doc.lines.len(), rows, self.detail_top());
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
        }
        let bottom = format!("└{}┘", RULE.repeat(inner));
        put_line(buf, rows + 1, bottom);
    }

    /// The document of the item under the cursor, `w` cells wide. Empty
    /// when there is no item.
    pub(crate) fn detail_doc(&self, w: usize, rows: usize) -> Doc<'_> {
        let mut doc = Doc {
            lines: Vec::new(),
            desc: 0,
            files: None,
        };
        let Some(item) = self
            .rows
            .get(self.cursor)
            .and_then(|r| self.snap.item(r.id))
        else {
            return doc;
        };
        if rows == 0 {
            return doc;
        }
        let line = self.snap.line(item);
        doc.lines = wrap_words(description(item, line), w);
        doc.desc = doc.lines.len();
        doc.lines
            .push(vec![Span::new(RULE.repeat(w), Some(Slot::Muted))]);
        doc.lines.extend(self.meta_lines(item, line, w));
        let files = self.file_lines(item);
        if !files.is_empty() {
            doc.files = Some(doc.lines.len());
            doc.lines.extend(files);
        }
        doc
    }

    fn meta_lines<'a>(
        &'a self,
        item: &'a Item,
        line: &'a Line,
        w: usize,
    ) -> Vec<Vec<Span<'a>>> {
        let tags = line.tags();
        let mut fields = vec![Field::text("state", item.state().as_str())];
        if let Some(pri) = line.effective_priority() {
            fields.push(Field::text(
                "priority",
                char::from(pri.letter()).to_string(),
            ));
        }
        fields.extend(self.date_fields(item, line, &tags));
        fields.extend(claim_fields(item, &tags));
        fields.extend(self.relation_fields(item));
        fields.extend(tag_fields(item, line, &tags));
        let column = label_column(&fields);
        fields
            .iter()
            .flat_map(|f| field_lines(f, column, w))
            .collect()
    }

    fn date_fields<'a>(
        &self,
        item: &Item,
        line: &Line,
        tags: &Tags<'_>,
    ) -> Vec<Field<'a>> {
        let mut out = Vec::new();
        out.extend(line.created().and_then(|d| labelled("created", d)));
        out.extend(line.completed().and_then(|d| labelled("completed", d)));
        out.extend(self.due_fields(tags.due));
        if let Some(t) = tags.threshold {
            let later = if item.later() { " (later)" } else { "" };
            out.push(Field::text("t", format!("{t}{later}")));
        }
        out.extend(rec_field(line, tags));
        out.extend(self.started_field(item));
        out
    }

    fn due_fields<'a>(&self, due: Option<&str>) -> Vec<Field<'a>> {
        let Some(due) = due else {
            return Vec::new();
        };
        let Some(date) = Date::parse(due) else {
            return Vec::new();
        };
        let days = self.clock.today().days_until(date);
        let soon = self.deps.config.upcoming_days;
        let slot = Some(due_slot(days, soon));
        vec![
            Field::text("due", due),
            Field::new("", Span::new(relative(days), slot)),
        ]
    }

    fn started_field<'a>(&self, item: &Item) -> Option<Field<'a>> {
        let at = item.started()?;
        let local = at.to_zoned(self.clock.zone().clone());
        let mut value = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            local.year(),
            local.month(),
            local.day(),
            local.hour(),
            local.minute(),
        );
        if let Some(elapsed) = item.elapsed(self.clock.now()) {
            value = format!("{value} ({})", format_elapsed(elapsed));
        }
        Some(Field::text("started", value))
    }

    fn relation_fields<'a>(&self, item: &Item) -> Vec<Field<'a>> {
        let mut out = Vec::new();
        for blocker in item.blockers() {
            let Some(other) = self.snap.item(*blocker) else {
                continue;
            };
            let text = self.snap.line(other).text();
            let text = String::from_utf8_lossy(&text);
            let nr = other.nr().get();
            out.push(Field::text("blocked by", format!("{text} ({nr})")));
        }
        for id in self.snap.dangling(item) {
            let id = String::from_utf8_lossy(id);
            out.push(Field::warning(format!("missing prerequisite {id}")));
        }
        if item.cycle() {
            out.push(Field::warning("dependency cycle"));
        }
        out
    }

    /// `files`, then one line for each `file:` link.
    fn file_lines<'a>(&'a self, item: &Item) -> Vec<Vec<Span<'a>>> {
        let files = linked_files(&self.snap, item, self.deps.home.as_deref());
        if files.is_empty() {
            return Vec::new();
        }
        let mut out = vec![vec![Span::new("files", Some(Slot::Muted))]];
        for file in &files {
            let mark = exists_mark(file.exists);
            let raw = String::from_utf8_lossy(file.raw);
            let text = format!("{mark} {raw}");
            out.push(vec![Span::new(text, Some(Slot::Link))]);
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
fn description<'a>(item: &Item, line: &'a Line) -> Vec<Span<'a>> {
    let base = base_slot(item);
    line.tokens()
        .map(|tok: TokenView<'a>| {
            let slot = Some(token_slot(&tok, base));
            Span::new(String::from_utf8_lossy(tok.text), slot)
        })
        .collect()
}

fn rec_field<'a>(line: &Line, tags: &Tags<'_>) -> Option<Field<'a>> {
    tags.rec?;
    let tok = line
        .tokens()
        .find(|t| t.kind == Kind::Tag && t.key == b"rec")?;
    labelled("rec", &String::from_utf8_lossy(tok.value))
}

fn claim_fields<'a>(item: &Item, tags: &Tags<'_>) -> Vec<Field<'a>> {
    let owner = tags.owner.map(String::from_utf8_lossy);
    let owner = owner.as_deref().unwrap_or_default();
    let done = item.state() == State::Done;
    let mut claim = match item.claim() {
        _ if done && !owner.is_empty() => format!("finished by {owner}"),
        Claim::Human if done => "finished (by you)".to_owned(),
        Claim::Human => "claimed (by you)".to_owned(),
        Claim::Agent if !done => format!("claimed by {owner}"),
        Claim::Agent | Claim::None => String::new(),
    };
    if !claim.is_empty() && item.stale() {
        claim.push_str(", stale");
    }
    let id = tags.id.map(String::from_utf8_lossy);
    let id = id.as_deref().unwrap_or_default();
    [labelled("claim", &claim), labelled("id", id)]
        .into_iter()
        .flatten()
        .collect()
}

/// A tag the panel does not show on a line of its own.
fn is_structural(item: &Item, key: &[u8]) -> bool {
    if key == b"pri" {
        return item.state() == State::Done;
    }
    is_single_valued(key) || key == b"dep" || key == b"file"
}

fn tag_fields<'a>(
    item: &Item,
    line: &'a Line,
    tags: &Tags<'a>,
) -> Vec<Field<'a>> {
    let mut out = Vec::new();
    let tokens = line.tokens().filter(|t| t.kind == Kind::Tag);
    for tok in tokens.filter(|t| !is_structural(item, t.key)) {
        let key = String::from_utf8_lossy(tok.key);
        let value = String::from_utf8_lossy(tok.value);
        out.push(Field::new(key, plain(value)));
    }
    for tok in &tags.extra {
        if tok.key == b"pri" && item.state() != State::Done {
            continue;
        }
        let text = format!("{} (ignored)", String::from_utf8_lossy(tok.text));
        out.push(Field::new("extra", Span::new(text, Some(Slot::Warning))));
    }
    out
}

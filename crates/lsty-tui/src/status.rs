//! The status line: the mode, the sort, the doing count, a message, the
//! hints, the position and the version.

use lsty_core::{
    errs::Source,
    keys::{Action, Matcher, Mode},
    list::State,
    theme::Slot,
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    focus::Keys,
    hints::{cancelling, dim, hints, label},
    messages::Tone,
    model::Model,
    rows::MARKER_DOING,
    text::{Span, cut, fit, pad, put, width},
};

fn mode_slot(mode: Mode) -> Slot {
    match mode {
        Mode::Insert => Slot::ModeInsert,
        Mode::Visual => Slot::ModeVisual,
        Mode::Command => Slot::ModeCommand,
        Mode::Normal
        | Mode::Completion
        | Mode::Panel
        | Mode::Detail
        | Mode::Help
        | Mode::Picker => Slot::ModeNormal,
    }
}

impl Model {
    /// Draws the status line on row `y`, `width` cells wide.
    pub(crate) fn draw_status(&self, buf: &mut Buffer, at: (u16, u16)) {
        let (width_cells, y) = at;
        let w = usize::from(width_cells);
        let mut line = self.status_base();
        let configured = &self.deps.config.hints;
        let hidden = self.hidden_hints();
        let keys = &self.deps.keys;
        let table = self.key_mode();
        let accept = if table == Mode::Command {
            self.line
                .as_ref()
                .map_or(Action::Execute.hint(), |l| l.kind.enter())
        } else if self.choosing.is_some() {
            "save"
        } else {
            "open"
        };
        let answer = self.answers();
        let mut used = width(&line);
        let tail = self.right_end(used, w);
        let taken = if tail.is_empty() { 0 } else { width(&tail) + 2 };
        let room = w - taken;
        let label = |action| {
            let answer = answer(action);
            let word = self
                .tab_word(action)
                .or_else(|| label(table, action, accept, answer))?;
            Some((word, dim(answer)))
        };
        let mut shown = hints(table, configured, keys, hidden, label);
        if self.matchers.get(&table).is_some_and(Matcher::cancels) {
            let left = room.saturating_sub(used);
            cancelling(&mut shown, (self.typed_count() > 0).then_some(left));
        }
        for hint in shown {
            let cells = 2 + width(&hint);
            if used + cells > room {
                break;
            }
            used += cells;
            line.push(Span::new("  ", None));
            line.extend(hint);
        }
        let mut line = cut(line, room);
        pad(&mut line, w - width(&tail), None);
        line.extend(tail);
        pad(&mut line, w, None);
        let area = Rect::new(0, y, width_cells, 1);
        buf.set_style(area, self.styles.fill(Slot::StatusBg));
        put(buf, (0, y), w, &line, &self.styles);
    }

    /// The actions that have no hint now, whatever the mode lists.
    fn hidden_hints(&self) -> &'static [Action] {
        if self.typing_help() || self.typing_theme() {
            &[Action::CompleteNext]
        } else if self.keys_to() == Keys::Picker
            || self.output.as_ref().is_some_and(|o| o.help.is_none())
        {
            &[Action::Search]
        } else if self.search_in_effect() {
            &[]
        } else {
            &[Action::ClearSearch]
        }
    }

    /// The spans at the right end of a status line `w` cells wide whose
    /// left part is `base` cells, after a gap of two: the position, then
    /// the version, each with a space after it. The version gives way
    /// first, then the position. The position shows with the list or its
    /// field focused, and both go while a prompt line is open.
    fn right_end(&self, base: usize, w: usize) -> Vec<Span<'_>> {
        let mode = self.key_mode();
        let mut out = Vec::new();
        if matches!(
            mode,
            Mode::Normal | Mode::Visual | Mode::Insert | Mode::Completion
        ) {
            let new = self.edit.as_ref().is_some_and(|e| !e.replacing());
            let of = self.rows.len() + usize::from(new);
            let at = (self.field_at() + 1).min(of);
            let field = self.edit.as_ref().map_or(String::new(), |e| {
                format!(
                    "\u{2502}{}|{}",
                    e.field.cursor + 1,
                    e.field.text.len()
                )
            });
            let label = format!(" {at}|{of}{field} ");
            out.push(Span::new(label, Some(mode_slot(self.mode()))));
            out.push(Span::new(" ", None));
        }
        if self.line.is_none() {
            out.push(Span::new(self.deps.version, Some(Slot::Muted)));
            out.push(Span::new(" ", None));
        }
        while !out.is_empty() && base + 2 + width(&out) > w {
            out.truncate(out.len() - 2);
        }
        out
    }

    /// Whether a warning or an error is up: the status line then puts it
    /// first, and drops what does not fit after it.
    fn warned(&self) -> bool {
        !self.notices().is_empty()
            || self.message.text().is_some()
                && self.message.tone != Tone::Notice
    }

    /// The label at the start of the status line: the mode's letter, then
    /// the count typed so far.
    fn mode_label(&self) -> String {
        let name = self.mode().as_str().get(..1).unwrap_or("").to_uppercase();
        match self.typed_count() {
            0 => format!(" {name} "),
            count => format!(" {name} {count} "),
        }
    }

    /// The mode label and the space after it, where the status line starts.
    pub(crate) fn status_start(&self) -> Vec<Span<'_>> {
        vec![
            Span::new(self.mode_label(), Some(mode_slot(self.mode()))),
            Span::new(" ", None),
        ]
    }

    /// The status line before the hints.
    pub(crate) fn status_base(&self) -> Vec<Span<'_>> {
        let mut out = self.status_start();
        let w = usize::from(self.width);
        let warned = self.line.is_none() && self.warned();
        for (i, part) in self.status_parts().into_iter().enumerate() {
            let left = w.saturating_sub(width(&out) + 2 * usize::from(i > 0));
            let part = match (warned, i) {
                (true, 0) => fit(part, left),
                (true, _) if width(&part) > left => {
                    pad(&mut out, w, None);
                    break;
                }
                _ => part,
            };
            if i > 0 {
                out.push(Span::new("  ", None));
            }
            out.extend(part);
        }
        out
    }

    /// The pieces after the mode. A warning, an error or a refusal comes
    /// first, then the start notices, then the rest in its order; a plain
    /// message comes last of the rest.
    fn status_parts(&self) -> Vec<Vec<Span<'_>>> {
        if let Some(line) = &self.line {
            let mut spans =
                vec![Span::new(line.kind.label(), Some(Slot::Text))];
            spans.extend(line.field.lines(usize::MAX, self.typed()).concat());
            return vec![spans];
        }
        let mut parts = Vec::new();
        if let Some((lo, hi)) = self.selected_rows() {
            let text = format!("{} selected", hi - lo + 1);
            parts.push(vec![Span::new(text, Some(Slot::Text))]);
        }
        let sort = format!("sort({})", self.sort.name());
        parts.push(vec![Span::new(sort, Some(Slot::Muted))]);
        if let Some(blocking) = self.blocking() {
            parts.push(vec![Span::new(blocking, Some(Slot::Text))]);
        }
        if let Some(doing) = self.doing() {
            parts.push(doing);
        }
        let mut head: Vec<_> = self
            .notices()
            .into_iter()
            .map(|notice| vec![Span::new(notice, Some(Slot::Warning))])
            .collect();
        if let Some(m) = self.message.text() {
            let alert = self.message.tone == Tone::Alert;
            let slot = if alert { Slot::Error } else { Slot::Warning };
            let m = vec![Span::new(m, Some(slot))];
            if self.message.tone == Tone::Notice {
                parts.push(m);
            } else {
                head.insert(0, m);
            }
        }
        head.extend(parts);
        head
    }

    fn doing(&self) -> Option<Vec<Span<'_>>> {
        let (doing, stale) = self.underway();
        if doing == 0 {
            return None;
        }
        let mut out = vec![Span::new(
            format!("{MARKER_DOING} {doing} in doing"),
            Some(Slot::Doing),
        )];
        if stale > 0 {
            let text = format!("({stale} stale)");
            out.extend([
                Span::new(" ", None),
                Span::new(text, Some(Slot::Warning)),
            ]);
        }
        Some(out)
    }

    /// How many tasks of the todo file are underway, and how many of
    /// those are stale.
    fn underway(&self) -> (usize, usize) {
        let tasks = self.snap.in_source(Source::Todo).iter();
        let doing = tasks.filter(|i| i.state() == State::Doing);
        doing.fold((0, 0), |(all, stale), task| {
            (all + 1, stale + usize::from(task.stale()))
        })
    }
}

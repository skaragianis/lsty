//! The status line: the mode, the file, the doing count, a message and the
//! hints.

use lsty_core::{
    errs::Source,
    keys::{Action, Mode},
    list::State,
    theme::Slot,
};
use ratatui::{buffer::Buffer, layout::Rect};

use crate::{
    hints::hints,
    messages::Tone,
    model::Model,
    rows::MARKER_DOING,
    text::{Span, cut, pad, put, width},
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
        let hidden: &[Action] = if self.typing_help() || self.typing_theme() {
            &[Action::CompleteNext]
        } else if self.opening.is_some()
            || self.output.as_ref().is_some_and(|o| o.help.is_none())
        {
            &[Action::Search]
        } else {
            &[]
        };
        let accept = if self.choosing.is_some() {
            "save"
        } else {
            "open"
        };
        let keys = &self.deps.keys;
        let table = self.key_mode();
        let mut used = width(&line);
        for hint in hints(table, configured, keys, hidden, accept) {
            let cells = 2 + width(&hint);
            if used + cells > w {
                break;
            }
            used += cells;
            line.push(Span::new("  ", None));
            line.extend(hint);
        }
        let mut line = cut(line, w);
        pad(&mut line, w, None);
        let area = Rect::new(0, y, width_cells, 1);
        buf.set_style(area, self.styles.fill(Slot::StatusBg));
        put(buf, (0, y), w, &line, &self.styles);
    }

    /// The status line before the hints.
    pub(crate) fn status_base(&self) -> Vec<Span<'_>> {
        let mode = format!(" {} ", self.mode().as_str().to_uppercase());
        let mut out = vec![
            Span::new(mode, Some(mode_slot(self.mode()))),
            Span::new(" ", None),
        ];
        for (i, part) in self.status_parts().into_iter().enumerate() {
            if i > 0 {
                out.push(Span::new("  ", None));
            }
            out.extend(part);
        }
        out
    }

    fn status_parts(&self) -> Vec<Vec<Span<'_>>> {
        if let Some(line) = &self.line {
            let text = format!("{}{}", line.kind.label(), line.field.text());
            return vec![vec![Span::new(text, Some(Slot::Text))]];
        }
        let mut parts = Vec::new();
        if let Some((lo, hi)) = self.selected_rows() {
            let text = format!("{} selected", hi - lo + 1);
            parts.push(vec![Span::new(text, Some(Slot::Text))]);
        }
        parts.push(vec![Span::new(self.file_name(), Some(Slot::Text))]);
        parts.push(vec![Span::new(self.sort.name(), Some(Slot::Muted))]);
        if let Some(blocking) = self.blocking() {
            parts.push(vec![Span::new(blocking, Some(Slot::Text))]);
        }
        if let Some(doing) = self.doing() {
            parts.push(doing);
        }
        for notice in self.notices() {
            parts.push(vec![Span::new(notice, Some(Slot::Warning))]);
        }
        if let Some(message) = &self.message {
            let slot = Some(match self.tone {
                Tone::Alert => Slot::Error,
                Tone::Notice => Slot::Warning,
            });
            parts.push(vec![Span::new(message.as_str(), slot)]);
        }
        parts
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

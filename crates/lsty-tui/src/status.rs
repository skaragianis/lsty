//! The status line: the mode, the file, the progress, a message and the
//! hints.

use lsty_core::errs::Source;
use lsty_core::keys::Mode;
use lsty_core::list::State;
use lsty_core::theme::Slot;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::hints::hints;
use crate::messages::Tone;
use crate::model::Model;
use crate::text::{Span, cut, pad, put, width};

/// The manual order: the only one so far.
const SORT: &str = "manual";

fn mode_slot(mode: Mode) -> Slot {
    match mode {
        Mode::Insert => Slot::ModeInsert,
        Mode::Visual => Slot::ModeVisual,
        Mode::Command => Slot::ModeCommand,
        Mode::Normal
        | Mode::Completion
        | Mode::Panel
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
        for hint in hints(self.key_mode(), configured, &self.deps.keys) {
            let mut more = vec![Span::new("  ", None)];
            more.extend(hint);
            if width(&line) + width(&more) > w {
                break;
            }
            line.extend(more);
        }
        let mut line = cut(line, w);
        pad(&mut line, w, None);
        let area = Rect::new(0, y, width_cells, 1);
        buf.set_style(area, self.styles.fill(Slot::StatusBg));
        put(buf, (0, y), w, &line, &self.styles);
    }

    /// The status line before the hints.
    fn status_base(&self) -> Vec<Span<'_>> {
        let mode = format!(" {} ", self.mode.as_str().to_uppercase());
        let mut out = vec![
            Span::new(mode, Some(mode_slot(self.mode))),
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
        if let Some(field) = &self.input {
            let text = format!("/{}", field.text());
            return vec![vec![Span::new(text, Some(Slot::Text))]];
        }
        let mut parts = vec![
            vec![Span::new(self.file_name(), Some(Slot::Text))],
            vec![Span::new(SORT, Some(Slot::Muted))],
        ];
        if let Some(progress) = self.progress() {
            parts.push(progress);
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

    fn progress(&self) -> Option<Vec<Span<'_>>> {
        let (doing, stale) = self.underway();
        if doing == 0 {
            return None;
        }
        let mut out = vec![Span::new(
            format!("▶ {doing} in progress"),
            Some(Slot::Doing),
        )];
        if stale > 0 {
            let text = format!(" ({stale} stale)");
            out.push(Span::new(text, Some(Slot::Warning)));
        }
        Some(out)
    }

    /// How many items of the todo file are underway, and how many of
    /// those are stale.
    fn underway(&self) -> (usize, usize) {
        let items = self.snap.in_source(Source::Todo).iter();
        let doing = items.filter(|i| i.state() == State::Doing);
        doing.fold((0, 0), |(all, stale), item| {
            (all + 1, stale + usize::from(item.stale()))
        })
    }
}

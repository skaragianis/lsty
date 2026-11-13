//! The status line: the mode, the file, the doing count, a message, the
//! hints and the version.

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
    text::{Span, cut, fit, pad, put, width, width_of},
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
        let taken = self.version_room(used, w);
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
        if taken > 0 {
            pad(&mut line, room + 2, None);
            line.extend([
                Span::new(self.deps.version, Some(Slot::Muted)),
                Span::new(" ", None),
            ]);
        }
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

    /// The cells at the right of a status line `w` cells wide that the
    /// version, the space after it and a gap of two take; none while a
    /// field is open, or when the left part (`base` cells) and all of
    /// that do not fit.
    fn version_room(&self, base: usize, w: usize) -> usize {
        let room = width_of(self.deps.version) + 3;
        if self.line.is_some() || base + room > w {
            0
        } else {
            room
        }
    }

    /// Whether a warning or an error is up: the status line then puts it
    /// first, and drops what does not fit after it.
    fn warned(&self) -> bool {
        !self.notices().is_empty()
            || self.message.is_some() && self.tone != Tone::Notice
    }

    /// The status line before the hints.
    pub(crate) fn status_base(&self) -> Vec<Span<'_>> {
        let name = self.mode().as_str().to_uppercase();
        let mode = match self.typed_count() {
            0 => format!(" {name} "),
            count => format!(" {name} {count} "),
        };
        let mut out = vec![
            Span::new(mode, Some(mode_slot(self.mode()))),
            Span::new(" ", None),
        ];
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
        let mut head: Vec<_> = self
            .notices()
            .into_iter()
            .map(|notice| vec![Span::new(notice, Some(Slot::Warning))])
            .collect();
        if let Some(m) = &self.message {
            let alert = self.tone == Tone::Alert;
            let slot = if alert { Slot::Error } else { Slot::Warning };
            let m = vec![Span::new(m.as_str(), Some(slot))];
            if self.tone == Tone::Notice {
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

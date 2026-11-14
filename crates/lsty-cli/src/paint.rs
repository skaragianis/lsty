//! Colour for the human output: the escape sequences of a theme's slots
//! and the slot of each part of a list line.

use std::{borrow::Cow, ops::Range};

use lsty_core::{
    config::Warning,
    dates::Date,
    errs::Error,
    list::{State, Task},
    theme::{Colour, Parts, Profile, Slot, Style, Theme, Today},
    todotxt::{Line, trim_range},
    visible::visible_bytes,
};

/// Ends every colour.
const RESET: &[u8] = b"\x1b[m";

/// Writes text in the styles of a theme. A painter that is off writes
/// text as it is.
#[derive(Clone, Debug)]
pub(crate) struct Painter {
    on: Option<On>,
}

/// What a painter that is on needs.
#[derive(Clone, Debug)]
struct On {
    /// The sequence that starts each slot's style, in the order of
    /// [`Slot::ALL`]; empty when the style changes nothing.
    sequences: [Vec<u8>; Slot::ALL.len()],
    /// Today and the number of days that count as soon, for due dates.
    today: Option<Today>,
}

impl Painter {
    /// A painter that writes text as it is.
    pub(crate) fn off() -> Self {
        Self { on: None }
    }

    /// A painter for `theme` on a terminal with `profile`.
    pub(crate) fn new(theme: &Theme, profile: Profile) -> Self {
        let sequences = Slot::ALL.map(|slot| open(theme.style(slot), profile));
        Self {
            on: Some(On {
                sequences,
                today: None,
            }),
        }
    }

    /// This painter, colouring due dates as of `today`, with
    /// `upcoming_days` days counting as soon.
    #[must_use]
    pub(crate) fn with_dates(
        mut self,
        today: Date,
        upcoming_days: u64,
    ) -> Self {
        if let Some(on) = &mut self.on {
            on.today = Some(Today {
                date: today,
                soon: upcoming_days,
            });
        }
        self
    }

    /// `text` in the style of `slot`.
    pub(crate) fn paint(&self, slot: Slot, text: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len() + 16);
        self.paint_into(&mut out, slot, text);
        out
    }

    /// Writes `text` in the style of `slot` to `out`.
    pub(crate) fn paint_into(
        &self,
        out: &mut Vec<u8>,
        slot: Slot,
        text: &[u8],
    ) {
        self.put(out, slot, &visible_bytes(text));
    }

    /// Writes `text`, which is already safe to print, in the style of
    /// `slot` to `out`.
    fn put(&self, out: &mut Vec<u8>, slot: Slot, text: &[u8]) {
        let sequences = self
            .on
            .as_ref()
            .map(|on| &on.sequences[slot.index()])
            .filter(|s| !s.is_empty());
        match sequences {
            Some(open) if !text.is_empty() => {
                out.extend_from_slice(open);
                out.extend_from_slice(text);
                out.extend_from_slice(RESET);
            }
            Some(_) | None => out.extend_from_slice(text),
        }
    }

    /// The line of `task` with its parts coloured by the rule of
    /// [`Parts`], the other words in `text`, or `blocked` for a blocked
    /// task. A done line is one `done` run.
    pub(crate) fn task(&self, out: &mut Vec<u8>, line: &Line, task: &Task) {
        // the drawn line has nothing left to draw, so its parts are put
        // out as they are
        if let Cow::Owned(drawn) = visible_bytes(line.raw()) {
            return self.task(out, &Line::parse(drawn), task);
        }
        let raw = line.raw();
        if self.on.is_none() {
            out.extend_from_slice(raw);
            return;
        }
        if task.state() == State::Done {
            self.put(out, Slot::Done, raw);
            return;
        }
        let base = if task.blocked() {
            Slot::Blocked
        } else {
            Slot::Text
        };
        let today = self.on.as_ref().and_then(|on| on.today);
        let parts = Parts::of(task, today);
        let mut at = 0;
        for (start, end, slot) in parts.runs(line) {
            let from = at.min(start);
            self.gap(out, base, raw.get(from..start).unwrap_or_default());
            self.put(out, slot, raw.get(start..end).unwrap_or_default());
            at = end;
        }
        self.gap(out, base, raw.get(at..).unwrap_or_default());
    }

    /// Text between coloured parts: spaces stay plain, the words between
    /// them take `base`.
    fn gap(&self, out: &mut Vec<u8>, base: Slot, text: &[u8]) {
        let Range { start, end } = trim_range(text);
        out.extend_from_slice(text.get(..start).unwrap_or_default());
        self.put(out, base, text.get(start..end).unwrap_or_default());
        out.extend_from_slice(text.get(end..).unwrap_or_default());
    }

    /// The line of an error for people: `lsty: message`, then the hint,
    /// the first word in `error` and the rest in `text`.
    pub(crate) fn error_lines(&self, err: &Error) -> Vec<Vec<u8>> {
        let mut head = self.paint(Slot::Error, b"lsty:");
        head.push(b' ');
        let message = err.message().as_bytes();
        let drawn = if err.is_multiline() {
            let lines: Vec<_> =
                message.split(|&b| b == b'\n').map(visible_bytes).collect();
            Cow::Owned(lines.join(&b'\n'))
        } else {
            visible_bytes(message)
        };
        self.put(&mut head, Slot::Text, &drawn);
        let mut lines = vec![head];
        if let Some(hint) = err.hint() {
            lines.push(
                self.paint(Slot::Text, format!("hint: {hint}").as_bytes()),
            );
        }
        lines
    }

    /// The line of a warning: `lsty: warning:` in `warning`, the rest in
    /// `text`.
    pub(crate) fn warning_line(&self, warning: &Warning) -> Vec<u8> {
        let mut line = self.paint(Slot::Warning, b"lsty: warning:");
        line.push(b' ');
        let text = format!("{}: {}", warning.source, warning.message);
        self.paint_into(&mut line, Slot::Text, text.as_bytes());
        line
    }
}

/// `ESC [ params m` for the attributes and colours of `style`, or nothing
/// when it changes nothing.
fn open(style: Style, profile: Profile) -> Vec<u8> {
    let colour = |c: Colour, base: u8| sgr_colour(profile.reduce(c), base);
    let mut params = Vec::new();
    for (on, code) in [
        (style.bold, "1"),
        (style.italic, "3"),
        (style.underline, "4"),
        (style.faint, "2"),
    ] {
        if on {
            params.push(code.to_owned());
        }
    }
    let fg = style.fg.map(|c| colour(c, 30));
    let bg = style.bg.map(|c| colour(c, 40));
    params.extend(fg.into_iter().chain(bg));
    if style.strike {
        params.push("9".to_owned());
    }
    if params.is_empty() {
        return Vec::new();
    }
    format!("\x1b[{}m", params.join(";")).into_bytes()
}

/// The SGR parameters of a foreground (`base` 30) or background (`base`
/// 40) colour.
fn sgr_colour(colour: Colour, base: u8) -> String {
    match colour {
        Colour::Index(n @ 0..=7) => (base + n).to_string(),
        Colour::Index(n @ 8..=15) => (base + 60 + n - 8).to_string(),
        Colour::Index(n) => format!("{};5;{n}", base + 8),
        Colour::Rgb(red, green, blue) => {
            format!("{};2;{red};{green};{blue}", base + 8)
        }
    }
}

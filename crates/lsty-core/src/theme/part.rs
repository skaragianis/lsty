//! The slot of each part of a task: the one rule every view follows, so
//! that a `+project` is the project colour wherever it shows.

use crate::{
    dates::Date,
    errs::Source,
    links::link_in,
    list::{State, Task},
    theme::Slot,
    todotxt::{Kind, Line, TokenView, inside_span, is_claim_key},
};

/// Days that always count as soon, whatever `upcoming_days` says.
pub const WEEK: i32 = 7;

/// Today and the number of days that count as soon.
#[derive(Clone, Copy, Debug)]
pub struct Today {
    /// Today's date.
    pub date: Date,
    /// `upcoming_days` of the config.
    pub soon: u64,
}

/// The slot of a due date `days` from today when `soon` days count as
/// soon; `text` for one that is far off.
pub fn due_slot(days: i32, soon: u64) -> Slot {
    match days {
        ..=-1 => Slot::DueOverdue,
        0 => Slot::DueToday,
        _ if days < WEEK || i128::from(days) <= i128::from(soon) => {
            Slot::DueSoon
        }
        _ => Slot::Text,
    }
}

/// The slot of the state of `task`: the colour of its marker, and of its
/// row when the state colours the whole row.
pub fn state_slot(task: &Task) -> Slot {
    if task.source() == Source::Done {
        Slot::Muted
    } else if task.state() == State::Done {
        Slot::Done
    } else if task.blocked() {
        Slot::Blocked
    } else if task.state() == State::Doing {
        Slot::Doing
    } else {
        Slot::Text
    }
}

/// The slot of a claim that is stale or not.
fn claim(stale: bool) -> Slot {
    if stale { Slot::Warning } else { Slot::Doing }
}

/// The slot of the claim of `task`, as its `started:` and `owner:` tags
/// have it: `warning` when it is stale, `doing` for an open task, none
/// for a done one.
pub fn claim_slot(task: &Task) -> Slot {
    if task.is_open() {
        claim(task.stale())
    } else {
        Slot::Text
    }
}

/// What the rule needs besides the token: the date, for a `due:` tag,
/// whether the claim of the task is stale and whether it is closed. A
/// `due:` tag that is far off, or without a date to tell, is plain text.
#[derive(Clone, Copy, Debug, Default)]
pub struct Parts {
    /// Today, when there is a clock to tell.
    pub today: Option<Today>,
    /// The claim of the task is stale.
    pub stale: bool,
    /// The task is done, won't do or archived: its claim tags
    /// (`started:`, `owner:`, `ended:`) are plain tags and its `due:`
    /// is plain text.
    pub closed: bool,
}

impl Parts {
    /// The rule for the words of `task`.
    pub fn of(task: &Task, today: Option<Today>) -> Self {
        Self {
            today,
            stale: task.stale(),
            closed: !task.is_open(),
        }
    }

    /// The slot of `tok`.
    pub fn slot(self, tok: &TokenView<'_>) -> Slot {
        if link_in(tok).is_some() {
            return Slot::Link;
        }
        match tok.kind {
            Kind::Project => Slot::Project,
            Kind::Context => Slot::Context,
            Kind::Word if tok.quoted => Slot::Code,
            Kind::Word => Slot::Text,
            Kind::Tag => match tok.key {
                b"due" => self.due(tok.value),
                key if self.closed && is_claim_key(key) => Slot::Muted,
                b"started" | b"owner" => claim(self.stale),
                key if is_claim_key(key) => Slot::Doing,
                _ => Slot::Muted,
            },
        }
    }

    /// The slot of the first word of `text`, say a candidate of a
    /// completion; plain text when it has none.
    pub fn word(self, text: &str) -> Slot {
        // a word before it, so that `text` is not read as a task's header
        let line = Line::parse(format!("a {text}").as_bytes());
        line.tokens()
            .nth(1)
            .map_or(Slot::Text, |tok| self.slot(&tok))
    }

    /// The slot of the date `value` of a `due:` tag.
    pub fn due(self, value: &[u8]) -> Slot {
        let due = std::str::from_utf8(value).ok().and_then(Date::parse);
        match (self.today.filter(|_| !self.closed), due) {
            (Some(today), Some(due)) => {
                due_slot(today.date.days_until(due), today.soon)
            }
            _ => Slot::Text,
        }
    }

    /// The runs of `line` that are not plain text, as byte ranges and
    /// slots in order: the priority, then each part, a quoted run in
    /// one piece with the blanks inside it.
    pub fn runs(self, line: &Line) -> Vec<(usize, usize, Slot)> {
        let raw = line.raw();
        // after the done mark of a done task, before its dates
        let start = if line.is_done() { "x ".len() } else { 0 };
        let mut runs: Vec<_> = (line.priority().into_iter())
            .map(|p| (start, start + "(A)".len(), Slot::of_priority(p)))
            .collect();
        for tok in line.tokens() {
            let end = tok.start + tok.text.len();
            match (self.slot(&tok), runs.last_mut()) {
                (Slot::Text, _) => {}
                (Slot::Code, Some(last))
                    if last.2 == Slot::Code && inside_span(raw, last.1) =>
                {
                    last.1 = end;
                }
                (slot, _) => runs.push((tok.start, end, slot)),
            }
        }
        runs
    }
}

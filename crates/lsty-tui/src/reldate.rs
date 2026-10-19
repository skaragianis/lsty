//! The date and claim columns: what a due date, a completion date and a
//! claim read like.

use jiff::Timestamp;
use lsty_core::{
    dates::{Date, format_elapsed},
    list::{State, Task},
    theme::Slot,
    todotxt::Line,
};

/// The text of a column cell and the slot it is drawn in; no text is no
/// value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) text: String,
    pub(crate) slot: Option<Slot>,
}

impl Cell {
    fn new(text: impl Into<String>, slot: Slot) -> Self {
        Self {
            text: text.into(),
            slot: Some(slot),
        }
    }
}

/// What the date column needs from the clock.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Today {
    pub(crate) date: Date,
    /// `upcoming_days` of the config.
    pub(crate) soon: u64,
}

const WEEK: i32 = 7;
const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// The date column of a task: its due date, or for a done task its
/// completion date. Empty when there is nothing to show.
pub(crate) fn date_cell(task: &Task, line: &Line, today: Today) -> Cell {
    if task.state() == State::Done {
        return done_cell(line.completed(), today.date);
    }
    let due = line.tokens().find(|t| t.is_tag(b"due"));
    due.map_or_else(Cell::default, |tok| due_cell(tok.value, today))
}

fn due_cell(value: &[u8], today: Today) -> Cell {
    let due = std::str::from_utf8(value).ok().and_then(Date::parse);
    let Some(due) = due else {
        return Cell::new("?", Slot::Warning);
    };
    let days = today.date.days_until(due);
    let slot = due_slot(days, today.soon);
    let text = match days {
        ..=-1 => format!("{}d late", days.unsigned_abs()),
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        2..WEEK => weekday(due).to_owned(),
        _ => far(due, today.date),
    };
    Cell::new(text, slot)
}

/// A date a week or more away: month and day within the current year,
/// else the whole date.
fn far(due: Date, today: Date) -> String {
    let full = due.to_string();
    let year = &today.to_string()[..5];
    full.strip_prefix(year).unwrap_or(&full).to_owned()
}

/// The slot of a due date that is `days` from today.
pub(crate) fn due_slot(days: i32, soon: u64) -> Slot {
    match days {
        ..=-1 => Slot::DueOverdue,
        0 => Slot::DueToday,
        _ if days < WEEK || i128::from(days) <= i128::from(soon) => {
            Slot::DueSoon
        }
        _ => Slot::Text,
    }
}

fn weekday(date: Date) -> &'static str {
    WEEKDAYS[usize::from(date.weekday_from_monday())]
}

fn done_cell(completed: Option<&str>, today: Date) -> Cell {
    let days = completed
        .and_then(Date::parse)
        .map(|done| done.days_until(today));
    match days {
        Some(n @ 1..) => Cell::new(format!("{n}d ago"), Slot::Done),
        Some(..=0) | None => Cell::default(),
    }
}

/// The duration column of an underway task: for how long; and of a done
/// task that has a duration: how long it took.
pub(crate) fn claim_cell(task: &Task, now: Timestamp) -> Cell {
    let (slot, elapsed) = match (task.state(), task.elapsed(now)) {
        (State::Todo, _) | (State::Done, None) => return Cell::default(),
        (State::Done, Some(duration)) => {
            (Slot::Done, format_elapsed(duration))
        }
        (State::Doing, duration) => (
            Slot::Doing,
            duration.map_or_else(|| "?".to_owned(), format_elapsed),
        ),
    };
    let (slot, duration) = if task.stale() {
        (Slot::Warning, format!("!{elapsed}"))
    } else {
        (slot, elapsed)
    };
    Cell::new(duration, slot)
}

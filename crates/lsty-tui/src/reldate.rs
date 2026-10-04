//! The date and claim columns: what a due date, a completion date and a
//! claim read like.

use std::time::Duration;

use jiff::Timestamp;
use lsty_core::dates::{Date, format_elapsed};
use lsty_core::list::{Claim, Item, State};
use lsty_core::theme::Slot;
use lsty_core::todotxt::{Kind, Line};

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

/// The date column of an item: its due date, or for a done item its
/// completion date. Empty when there is nothing to show.
pub(crate) fn date_cell(item: &Item, line: &Line, today: Today) -> Cell {
    if item.state() == State::Done {
        return done_cell(line.completed(), today.date);
    }
    let due = line
        .tokens()
        .find(|t| t.kind == Kind::Tag && t.key == b"due");
    match due {
        Some(tok) => due_cell(tok.value, today),
        None => Cell::default(),
    }
}

fn due_cell(value: &[u8], today: Today) -> Cell {
    let due = std::str::from_utf8(value).ok().and_then(Date::parse);
    let Some(due) = due else {
        return Cell::new("due ?", Slot::Warning);
    };
    let days = today.date.days_until(due);
    let slot = due_slot(days, today.soon);
    let text = match days {
        ..=-1 => format!("{}d overdue", days.unsigned_abs()),
        0 => "due today".to_owned(),
        1 => "due tomorrow".to_owned(),
        2..WEEK => format!("due {}", weekday(due)),
        _ => format!("due {due}"),
    };
    Cell::new(text, slot)
}

/// The slot of a due date that is `days` from today.
pub(crate) fn due_slot(days: i32, soon: u64) -> Slot {
    let near = i64::from(days) <= i64::try_from(soon).unwrap_or(i64::MAX);
    match days {
        ..=-1 => Slot::DueOverdue,
        0 => Slot::DueToday,
        1..WEEK => Slot::DueSoon,
        _ if near => Slot::DueSoon,
        _ => Slot::Text,
    }
}

fn weekday(date: Date) -> &'static str {
    usize::try_from(date.weekday_from_monday())
        .ok()
        .and_then(|i| WEEKDAYS.get(i))
        .copied()
        .unwrap_or_default()
}

fn done_cell(completed: Option<&str>, today: Date) -> Cell {
    let days = completed
        .and_then(Date::parse)
        .map(|done| done.days_until(today));
    match days {
        Some(0) => Cell::new("done today", Slot::Done),
        Some(n @ 1..) => Cell::new(format!("done {n}d ago"), Slot::Done),
        Some(..=-1) | None => Cell::new("done", Slot::Done),
    }
}

/// The claim column of an underway item: who, and for how long.
pub(crate) fn claim_cell(
    item: &Item,
    owner: Option<&str>,
    now: Timestamp,
) -> Cell {
    if item.state() != State::Doing {
        return Cell::default();
    }
    let elapsed = item
        .elapsed(now)
        .map_or_else(|| "?".to_owned(), |d: Duration| format_elapsed(d));
    let owner = owner.unwrap_or_default();
    match (item.claim(), item.stale()) {
        (Claim::Agent, true) => {
            Cell::new(format!("{owner} stale {elapsed}"), Slot::Warning)
        }
        (Claim::Agent, false) => {
            Cell::new(format!("{owner} {elapsed}"), Slot::Doing)
        }
        (Claim::None | Claim::Human, _) => Cell::new(elapsed, Slot::Doing),
    }
}

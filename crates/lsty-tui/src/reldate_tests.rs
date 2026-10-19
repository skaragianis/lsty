use lsty_core::{dates::Date, list::Snapshot, theme::Slot};

use crate::{
    reldate::{Today, claim_cell, date_cell, due_slot},
    test_util::snapshot,
};

const NOW: &str = "2026-09-29T0900Z";

fn today(soon: u64) -> Today {
    Today {
        date: Date::new(2026, 9, 29).unwrap_or_else(|| panic!("date")),
        soon,
    }
}

fn first_date(snap: &Snapshot, soon: u64) -> (String, Option<Slot>) {
    let task = &snap.tasks()[0];
    let cell = date_cell(task, snap.line(task), today(soon));
    (cell.text, cell.slot)
}

#[test]
fn due_dates() {
    const ROWS: [(&str, &str, Slot); 14] = [
        ("due:2025-09-29", "365d late", Slot::DueOverdue),
        ("due:2026-09-27", "2d late", Slot::DueOverdue),
        ("due:2026-09-28", "1d late", Slot::DueOverdue),
        ("due:2026-09-29", "today", Slot::DueToday),
        ("due:2026-09-30", "tomorrow", Slot::DueSoon),
        ("due:2026-10-02", "fri", Slot::DueSoon),
        ("due:2026-10-05", "mon", Slot::DueSoon),
        ("due:2026-10-06", "10-06", Slot::DueSoon),
        ("due:2026-11-15", "11-15", Slot::Text),
        ("due:2026-12-31", "12-31", Slot::Text),
        ("due:2027-01-01", "2027-01-01", Slot::Text),
        ("due:2027-11-02", "2027-11-02", Slot::Text),
        ("due:soon", "?", Slot::Warning),
        ("due:2026-13-45", "?", Slot::Warning),
    ];
    for (tag, text, slot) in ROWS {
        let snap = snapshot(&format!("Pay rent {tag}\n"), "", NOW);
        assert_eq!(
            first_date(&snap, 7),
            (text.to_owned(), Some(slot)),
            "{tag}"
        );
    }
}

#[test]
fn a_date_8_days_away_in_the_next_year_has_its_year() {
    let snap = snapshot("Pay rent due:2027-01-05\n", "", NOW);
    let task = &snap.tasks()[0];
    let late = Today {
        date: Date::new(2026, 12, 28).unwrap_or_else(|| panic!("date")),
        soon: 7,
    };
    let cell = date_cell(task, snap.line(task), late);
    assert_eq!(cell.text, "2027-01-05");
}

#[test]
fn a_far_date_is_soon_within_upcoming_days() {
    let snap = snapshot("Pay rent due:2026-10-20\n", "", NOW);
    assert_eq!(first_date(&snap, 30).1, Some(Slot::DueSoon));
    assert_eq!(first_date(&snap, 7).1, Some(Slot::Text));
}

#[test]
fn no_due_date_is_no_cell() {
    let snap = snapshot("Pay rent\n", "", NOW);
    assert_eq!(first_date(&snap, 7), (String::new(), None));
}

#[test]
fn done_dates() {
    const ROWS: [(&str, &str); 8] = [
        ("x 2026-09-29 2026-09-20 Fix", ""),
        ("x 2026-09-28 Fix", "1d ago"),
        ("x 2026-09-29 Fix due:2026-09-29", ""),
        ("x 2026-09-26 2026-09-20 Fix", "3d ago"),
        ("x 2026-09-20 Fix", "9d ago"),
        ("x Fix", ""),
        ("x 2026-10-01 2026-09-20 Fix", ""),
        ("x 2026-09-29 Fix due:2026-09-01", ""),
    ];
    for (line, text) in ROWS {
        let snap = snapshot(&format!("{line}\n"), "", NOW);
        let slot = (!text.is_empty()).then_some(Slot::Done);
        assert_eq!(first_date(&snap, 7), (text.to_owned(), slot), "{line}");
    }
}

/// The duration text with its slot.
fn first_claim(todo: &str) -> (String, Option<Slot>) {
    let snap = snapshot(todo, "", NOW);
    let task = &snap.tasks()[0];
    let now = "2026-09-29T09:00:00Z".parse().unwrap_or_default();
    let duration = claim_cell(task, now);
    (duration.text, duration.slot)
}

fn claim(duration: &str, slot: Slot) -> (String, Option<Slot>) {
    (duration.to_owned(), Some(slot))
}

#[test]
fn claims() {
    let human = "Task started:2026-09-29T0700Z\n";
    assert_eq!(first_claim(human), claim("2h", Slot::Doing));
    let agent = "Task started:2026-09-29T0700Z owner:agent1\n";
    assert_eq!(first_claim(agent), claim("2h", Slot::Doing));
    let stale = "Task started:2026-09-29T0100Z owner:agent1\n";
    assert_eq!(first_claim(stale), claim("!8h", Slot::Warning));
    assert_eq!(first_claim("Task\n"), (String::new(), None));
}

#[test]
fn done_tasks_keep_their_duration() {
    let ended = "x 2026-09-29 Task started:2026-09-29T0600Z \
        ended:2026-09-29T0800Z";
    assert_eq!(first_claim(ended), claim("2h", Slot::Done));
    let never = "x 2026-09-29 Task started:2026-09-29T0600Z";
    assert_eq!(first_claim(never), (String::new(), None));
}

#[test]
fn a_claim_that_started_later_than_now_is_zero() {
    let future = "Task started:2026-09-29T1000Z\n";
    assert_eq!(first_claim(future).0, "0m");
}

#[test]
fn a_due_date_is_soon_within_a_week_or_the_days_set() {
    for (days, soon, slot) in [
        (-1, 0, Slot::DueOverdue),
        (0, 0, Slot::DueToday),
        (1, 0, Slot::DueSoon),
        (6, 0, Slot::DueSoon),
        (7, 0, Slot::Text),
        (7, 7, Slot::DueSoon),
        (8, 7, Slot::Text),
    ] {
        assert_eq!(due_slot(days, soon), slot, "{days} {soon}");
    }
}

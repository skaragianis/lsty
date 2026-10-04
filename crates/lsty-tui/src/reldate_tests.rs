use lsty_core::dates::Date;
use lsty_core::list::Snapshot;
use lsty_core::theme::Slot;

use crate::reldate::{Today, claim_cell, date_cell};
use crate::test_util::snapshot;

const NOW: &str = "2026-09-29T0900Z";

fn today(soon: u64) -> Today {
    Today {
        date: Date::new(2026, 9, 29).unwrap_or_else(|| panic!("date")),
        soon,
    }
}

fn first_date(snap: &Snapshot, soon: u64) -> (String, Option<Slot>) {
    let item = &snap.items()[0];
    let cell = date_cell(item, snap.line(item), today(soon));
    (cell.text, cell.slot)
}

#[test]
fn due_dates() {
    const ROWS: [(&str, &str, Slot); 9] = [
        ("due:2026-09-27", "2d overdue", Slot::DueOverdue),
        ("due:2026-09-28", "1d overdue", Slot::DueOverdue),
        ("due:2026-09-29", "due today", Slot::DueToday),
        ("due:2026-09-30", "due tomorrow", Slot::DueSoon),
        ("due:2026-10-02", "due fri", Slot::DueSoon),
        ("due:2026-10-05", "due mon", Slot::DueSoon),
        ("due:2026-10-06", "due 2026-10-06", Slot::DueSoon),
        ("due:2026-11-15", "due 2026-11-15", Slot::Text),
        ("due:soon", "due ?", Slot::Warning),
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
    const ROWS: [(&str, &str); 6] = [
        ("x 2026-09-29 2026-09-20 Fix", "done today"),
        ("x 2026-09-26 2026-09-20 Fix", "done 3d ago"),
        ("x 2026-09-20 Fix", "done 9d ago"),
        ("x Fix", "done"),
        ("x 2026-10-01 2026-09-20 Fix", "done"),
        ("x 2026-09-29 Fix due:2026-09-01", "done today"),
    ];
    for (line, text) in ROWS {
        let snap = snapshot(&format!("{line}\n"), "", NOW);
        assert_eq!(
            first_date(&snap, 7),
            (text.to_owned(), Some(Slot::Done)),
            "{line}"
        );
    }
}

fn first_claim(todo: &str, owner: Option<&str>) -> (String, Option<Slot>) {
    let snap = snapshot(todo, "", NOW);
    let item = &snap.items()[0];
    let now = "2026-09-29T09:00:00Z".parse().unwrap_or_default();
    let cell = claim_cell(item, owner, now);
    (cell.text, cell.slot)
}

#[test]
fn claims() {
    let human = "Task started:2026-09-29T0700Z\n";
    assert_eq!(
        first_claim(human, None),
        ("2h".to_owned(), Some(Slot::Doing))
    );
    let agent = "Task started:2026-09-29T0700Z owner:agent1\n";
    assert_eq!(
        first_claim(agent, Some("agent1")),
        ("agent1 2h".to_owned(), Some(Slot::Doing))
    );
    let stale = "Task started:2026-09-29T0100Z owner:agent1\n";
    assert_eq!(
        first_claim(stale, Some("agent1")),
        ("agent1 stale 8h".to_owned(), Some(Slot::Warning))
    );
    assert_eq!(first_claim("Task\n", None), (String::new(), None));
}

#[test]
fn a_claim_that_started_later_than_now_is_zero() {
    let future = "Task started:2026-09-29T1000Z\n";
    assert_eq!(first_claim(future, None).0, "0m");
}

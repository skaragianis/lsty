use super::Context;
use super::Query;
use super::test_util::{date, hits, hits_on, snapshot, utc};

/// Name, line, expected, with seven days of upcoming.
const ROWS: &[(&str, &str, bool)] = &[
    ("Upcoming", "X due:2026-10-06", true),
    ("Upcoming", "X due:2026-10-07", false),
    ("Upcoming", "X due:2026-09-28", false),
    ("Upcoming", "X due:2026-09-29", true),
    ("Upcoming", "x 2026-09-29 X due:2026-10-01", false),
    ("Today", "X due:2026-09-29", true),
    ("Today", "x 2026-09-29 X due:2026-09-29", false),
    ("Today", "X due:2026-09-30", false),
    ("Overdue", "X due:2026-09-28", true),
    ("Overdue", "X", false),
    ("Doing", "X", false),
    ("Doing", "X started:2026-09-29T0800Z", true),
    ("Waiting", "X dep:nope", false),
    ("No project", "X", true),
    ("No project", "X +p", false),
];

#[test]
fn builtins_match_their_query() {
    for &(name, line, want) in ROWS {
        let q = Query::builtin(name, 7).expect(name);
        assert_eq!(hits(&q, line), want, "09.33 {name} on {line}");
    }
}

#[test]
fn waiting_matches_the_blocked_item() {
    let snap = snapshot(&["A id:a", "B dep:a"], &[]);
    let zone = utc();
    let ctx = Context {
        today: date(2026, 9, 29),
        zone: &zone,
    };
    let q = Query::builtin("Waiting", 7).expect("found");
    let got: Vec<bool> = snap
        .items()
        .iter()
        .map(|i| q.matches(&snap, i, &ctx))
        .collect();
    assert_eq!(got, [false, true]);
}

#[test]
fn an_unknown_name_is_not_found() {
    assert!(Query::builtin("Nope", 7).is_none());
}

#[test]
fn upcoming_follows_the_window() {
    let q = Query::builtin("Upcoming", 3).expect("found");
    let today = date(2026, 9, 29);
    assert!(hits_on(&q, "X due:2026-10-02", today, &utc()));
    assert!(!hits_on(&q, "X due:2026-10-03", today, &utc()));
}

#[test]
fn upcoming_takes_a_window_of_any_length() {
    for days in [10_000, 99_999, u64::MAX] {
        let q = Query::builtin("Upcoming", days).expect("found");
        assert!(hits(&q, "X due:2026-10-01"), "{days}");
        assert!(!hits(&q, "X"), "{days}");
        assert!(!hits(&q, "X due:2026-09-28"), "{days}");
    }
}

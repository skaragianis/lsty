use jiff::tz::TimeZone;

use super::{
    Context, Query,
    test_util::{date, hits, hits_on},
};
use crate::list::test_util::{clock, snapshot};

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

/// The narrowing filter called `name`.
fn builtin(name: &str, days: u64) -> Query {
    let all = Query::builtins(days);
    all.into_iter()
        .find_map(|(n, q)| (n == name).then_some(q))
        .expect(name)
}

#[test]
fn builtins_match_their_query() {
    for &(name, line, want) in ROWS {
        let q = builtin(name, 7);
        assert_eq!(hits(&q, line), want, "09.33 {name} on {line}");
    }
}

#[test]
fn waiting_matches_the_blocked_item() {
    let snap = snapshot(&["A id:a", "B dep:a"], &[]);
    let clock = clock();
    let ctx = Context::from_clock(&clock);
    let q = builtin("Waiting", 7);
    let got: Vec<bool> = snap
        .items()
        .iter()
        .map(|i| q.matches(&snap, i, &ctx))
        .collect();
    assert_eq!(got, [false, true]);
}

#[test]
fn upcoming_follows_the_window() {
    let q = builtin("Upcoming", 3);
    let today = date(2026, 9, 29);
    assert!(hits_on(&q, "X due:2026-10-02", today, &TimeZone::UTC));
    assert!(!hits_on(&q, "X due:2026-10-03", today, &TimeZone::UTC));
}

#[test]
fn upcoming_takes_a_window_of_any_length() {
    for days in [10_000, 99_999, u64::MAX] {
        let q = builtin("Upcoming", days);
        assert!(hits(&q, "X due:2026-10-01"), "{days}");
        assert!(!hits(&q, "X"), "{days}");
        assert!(!hits(&q, "X due:2026-09-28"), "{days}");
    }
}

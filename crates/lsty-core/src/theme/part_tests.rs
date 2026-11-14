use super::*;
use crate::{dates::Date, todotxt::Line};

fn parts(stale: bool) -> Parts {
    let date = Date::parse("2026-11-14").unwrap();
    Parts {
        today: Some(Today { date, soon: 10 }),
        stale,
        closed: false,
    }
}

fn check(text: &str, parts: Parts, want: &[(&str, Slot)]) {
    let line = Line::parse(text);
    let got: Vec<_> = parts
        .runs(&line)
        .into_iter()
        .map(|(s, e, slot)| (&text[s..e], slot))
        .collect();
    assert_eq!(got, want, "{text}");
}

#[test]
fn each_part_of_a_task_has_its_slot() {
    let p = parts(false);
    check(
        "(A) a +p @c",
        p,
        &[
            ("(A)", Slot::PriorityA),
            ("+p", Slot::Project),
            ("@c", Slot::Context),
        ],
    );
    check("x `a b` c", p, &[("`a b`", Slot::Code)]);
    check("x (A) a", p, &[("(A)", Slot::PriorityA)]);
    check(
        "see https://x.io file:a",
        p,
        &[("https://x.io", Slot::Link), ("file:a", Slot::Link)],
    );
    check(
        "owner:me k:v",
        p,
        &[("owner:me", Slot::Doing), ("k:v", Slot::Muted)],
    );
    let closed = Parts {
        closed: true,
        ..parts(true)
    };
    check(
        "owner:me started:x ended:y due:2026-11-01",
        closed,
        &[
            ("owner:me", Slot::Muted),
            ("started:x", Slot::Muted),
            ("ended:y", Slot::Muted),
        ],
    );
    check("started:x", parts(true), &[("started:x", Slot::Warning)]);
    assert_eq!(p.word("+proj"), Slot::Project);
    assert_eq!(p.word("plain"), Slot::Text);
    // a file name is no task: `x` and a date are not its header
    assert_eq!(p.word("x +proj"), Slot::Text);
    assert_eq!(p.word("2026-01-02 +p"), Slot::Text);
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

#[test]
fn a_due_tag_is_coloured_by_its_date_or_plain() {
    let p = parts(false);
    check("due:2026-11-13", p, &[("due:2026-11-13", Slot::DueOverdue)]);
    check("due:2026-11-20", p, &[("due:2026-11-20", Slot::DueSoon)]);
    check("due:2027-01-01 due:x", p, &[]);
    check("due:2026-11-13", Parts::default(), &[]);
}

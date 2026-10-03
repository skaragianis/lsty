use super::{Line, Parts, Priority, build};

fn p(letter: u8) -> Option<Priority> {
    Priority::new(letter)
}

#[test]
fn build_rows() {
    let rows = [
        (
            "01.26 priority and created",
            Parts {
                priority: p(b'B'),
                created: Some("2026-09-20"),
                description: b"Buy milk",
                ..Parts::default()
            },
            "(B) 2026-09-20 Buy milk",
        ),
        (
            "description only",
            Parts {
                description: b"Buy milk",
                ..Parts::default()
            },
            "Buy milk",
        ),
        (
            "done with no dates",
            Parts {
                done: true,
                description: b"Fix bug",
                ..Parts::default()
            },
            "x Fix bug",
        ),
        (
            "done with completion date",
            Parts {
                done: true,
                completed: Some("2026-09-28"),
                description: b"Fix bug",
                ..Parts::default()
            },
            "x 2026-09-28 Fix bug",
        ),
        (
            "done with both dates",
            Parts {
                done: true,
                completed: Some("2026-09-28"),
                created: Some("2026-09-20"),
                description: b"Fix bug",
                ..Parts::default()
            },
            "x 2026-09-28 2026-09-20 Fix bug",
        ),
        (
            "done with priority carries it after the x",
            Parts {
                done: true,
                priority: p(b'A'),
                completed: Some("2016-05-20"),
                created: Some("2016-04-30"),
                description: b"measure space",
            },
            "x (A) 2016-05-20 2016-04-30 measure space",
        ),
        (
            "open priority only",
            Parts {
                priority: p(b'A'),
                description: b"Buy",
                ..Parts::default()
            },
            "(A) Buy",
        ),
        (
            "leading space in description is kept",
            Parts {
                priority: p(b'A'),
                description: b" Buy",
                ..Parts::default()
            },
            "(A)  Buy",
        ),
        ("empty parts build a blank line", Parts::default(), ""),
    ];
    for (name, parts, want) in rows {
        assert_eq!(build(&parts), want.as_bytes(), "{name}");
    }
}

#[test]
fn build_of_parsed_line_restores_it() {
    let rows = [
        "(A) 2026-09-20 Call client +work @phone due:2026-09-29",
        "x 2026-09-28 2026-09-20 Fix bug +work pri:B",
        "x 2026-09-28 Fix bug",
        "x Fix bug",
        "x (A) 2016-05-20 2016-04-30 measure space",
        "Buy milk",
        "(A)  Buy",
        "Buy milk 2026-09-29",
        "(A) ",
        "x 2026-09-28 ",
    ];
    for raw in rows {
        let line = Line::parse(raw);
        let parts = Parts {
            done: line.is_done(),
            priority: line.priority(),
            completed: line.completed(),
            created: line.created(),
            description: line.description(),
        };
        assert_eq!(build(&parts), raw.as_bytes(), "{raw:?}");
    }
}

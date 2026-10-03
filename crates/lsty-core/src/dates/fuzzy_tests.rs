use super::{Date, Fuzzy, Resolved, resolve};
use crate::errs::{Code, Field};

fn d(y: u16, m: u8, day: u8) -> Date {
    Date::new(y, m, day).expect("a real date")
}

fn tuesday() -> Date {
    d(2026, 9, 29)
}

#[test]
fn resolve_rows() {
    let rows = [
        ("today", tuesday(), d(2026, 9, 29)),
        ("tom", tuesday(), d(2026, 9, 30)),
        ("tomorrow", tuesday(), d(2026, 9, 30)),
        ("yesterday", tuesday(), d(2026, 9, 28)),
        ("fri", tuesday(), d(2026, 10, 2)),
        ("friday", tuesday(), d(2026, 10, 2)),
        ("mon", tuesday(), d(2026, 10, 5)),
        ("sun", tuesday(), d(2026, 10, 4)),
        ("tue", tuesday(), d(2026, 10, 6)),
        ("tuesday", tuesday(), d(2026, 10, 6)),
        ("Fri", tuesday(), d(2026, 10, 2)),
        ("TODAY", tuesday(), d(2026, 9, 29)),
        ("3D", tuesday(), d(2026, 10, 2)),
        ("3d", tuesday(), d(2026, 10, 2)),
        ("+3d", tuesday(), d(2026, 10, 2)),
        ("-3d", tuesday(), d(2026, 9, 26)),
        ("0d", tuesday(), d(2026, 9, 29)),
        ("1w", tuesday(), d(2026, 10, 6)),
        ("-1w", tuesday(), d(2026, 9, 22)),
        ("2w", tuesday(), d(2026, 10, 13)),
        ("1m", tuesday(), d(2026, 10, 29)),
        ("1y", tuesday(), d(2027, 9, 29)),
        ("-1m", tuesday(), d(2026, 8, 29)),
        ("1m", d(2026, 1, 31), d(2026, 2, 28)),
        ("1m", d(2028, 1, 31), d(2028, 2, 29)),
        ("1y", d(2028, 2, 29), d(2029, 2, 28)),
        ("1b", tuesday(), d(2026, 9, 30)),
        ("3b", tuesday(), d(2026, 10, 2)),
        ("4b", tuesday(), d(2026, 10, 5)),
        ("5b", tuesday(), d(2026, 10, 6)),
        ("1b", d(2026, 10, 2), d(2026, 10, 5)),
        ("1b", d(2026, 10, 3), d(2026, 10, 5)),
        ("1b", d(2026, 10, 4), d(2026, 10, 5)),
        ("-1b", d(2026, 10, 5), d(2026, 10, 2)),
        ("0b", d(2026, 10, 3), d(2026, 10, 3)),
        ("2026-10-10", tuesday(), d(2026, 10, 10)),
        ("9999d", d(2026, 1, 1), d(2053, 5, 18)),
        ("WEDNESDAY", tuesday(), d(2026, 9, 30)),
        ("thursday", tuesday(), d(2026, 10, 1)),
        ("thu", tuesday(), d(2026, 10, 1)),
        ("saturday", tuesday(), d(2026, 10, 3)),
        ("sat", tuesday(), d(2026, 10, 3)),
        ("1W", tuesday(), d(2026, 10, 6)),
        ("-0d", tuesday(), d(2026, 9, 29)),
        ("0000-01-01", tuesday(), d(0, 1, 1)),
    ];
    for (input, today, want) in rows {
        assert_eq!(
            resolve(input, today).ok(),
            Some(Resolved::On(want)),
            "{input} on {today}"
        );
    }
}

#[test]
fn empty_input_clears() {
    assert_eq!(resolve("", tuesday()).ok(), Some(Resolved::Clear));
}

const ACCEPTED: [&str; 4] = [
    "today, tomorrow (tom), yesterday",
    "a weekday: monday … sunday, mon … sun",
    "an offset: 3d, +2w, -1m, 1y, 5b",
    "a date: YYYY-MM-DD",
];

fn assert_invalid(input: &str, today: Date) {
    let Err(err) = resolve(input, today) else {
        panic!("{input:?} resolved");
    };
    assert_eq!(err.code(), Code::InvalidValue, "{input:?}");
    let fields: Vec<_> = err.fields().collect();
    assert_eq!(
        fields,
        [
            (
                "accepted",
                &Field::Strs(ACCEPTED.map(String::from).to_vec())
            ),
            ("value", &Field::Str(input.to_owned())),
        ],
        "{input:?}"
    );
}

#[test]
fn invalid_rows() {
    let inputs = [
        "d",
        "+d",
        "3",
        "3x",
        "3 d",
        " fri",
        "fr",
        "frid",
        "next fri",
        "2026-02-30",
        "26-10-10",
        "12345d",
        "9999y",
        "-9999y",
        "fri ",
        "+-3d",
        "+",
        "-",
        "2026-1-10",
        "3K",
        "３d",
        "2026-10-10T10",
        "2026-10-10 ",
        "fri+1d",
        "tomorrows",
        "é",
        "3é",
    ];
    for input in inputs {
        assert_invalid(input, tuesday());
    }
}

#[test]
fn out_of_range_rows() {
    assert_invalid("tom", d(9999, 12, 31));
    assert_invalid("fri", d(9999, 12, 31));
    assert_invalid("yesterday", d(0, 1, 1));
    assert_invalid("-1d", d(0, 1, 1));
    assert_invalid("1d", d(9999, 12, 31));
    assert_invalid("1b", d(9999, 12, 31));
}

#[test]
fn a_fuzzy_date_is_read_once_and_placed_on_any_day() {
    let rows = [
        ("fri", tuesday(), Some(d(2026, 10, 2))),
        ("FRI", d(2026, 10, 2), Some(d(2026, 10, 9))),
        ("+3d", tuesday(), Some(d(2026, 10, 2))),
        ("5b", tuesday(), Some(d(2026, 10, 6))),
        ("2026-02-03", tuesday(), Some(d(2026, 2, 3))),
        ("9999y", d(2026, 9, 29), None),
        ("-2027y", d(2026, 9, 29), None),
        ("9999y", d(0, 1, 1), Some(d(9999, 1, 1))),
        ("-2001y", d(2026, 9, 29), Some(d(25, 9, 29))),
        ("1y", d(9998, 12, 31), Some(d(9999, 12, 31))),
        ("1y", d(9999, 1, 1), None),
        ("1m", d(9999, 12, 1), None),
        ("-1y", d(1, 1, 1), Some(d(0, 1, 1))),
        ("-1y", d(0, 6, 1), None),
        ("-1m", d(0, 1, 1), None),
        ("-1m", d(0, 2, 1), Some(d(0, 1, 1))),
    ];
    for (input, today, want) in rows {
        let fuzzy = Fuzzy::parse(input).expect(input);
        assert_eq!(fuzzy.on(today), want, "{input} from {today}");
    }
}

#[test]
fn text_that_is_no_form_is_not_read() {
    for input in ["", "soon", "2026-02-30", "+d", "10000d", "3x", "1-2-3"] {
        assert_eq!(Fuzzy::parse(input), None, "{input:?}");
    }
}

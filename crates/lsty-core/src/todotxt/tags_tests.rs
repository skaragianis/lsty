use jiff::Timestamp;

use super::{Line, Priority, Rec, RecUnit};

fn texts(toks: &[super::TokenView<'_>]) -> Vec<String> {
    toks.iter()
        .map(|t| String::from_utf8_lossy(t.text).into_owned())
        .collect()
}

fn strs<'a>(items: &[&'a [u8]]) -> Vec<&'a str> {
    items
        .iter()
        .map(|b| std::str::from_utf8(b).unwrap())
        .collect()
}

/// Name, line, `id`, `dep:` values, extra tokens.
type IdRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static [&'static str],
    &'static [&'static str],
);
/// Name, line, file links as written and decoded.
type FileRow = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

fn pri(letter: u8) -> Option<Priority> {
    Priority::new(letter)
}

struct Row {
    name: &'static str,
    raw: &'static str,
    due: Option<&'static str>,
    threshold: Option<&'static str>,
    extra: &'static [&'static str],
    invalid: &'static [&'static str],
}

const fn row(name: &'static str, raw: &'static str) -> Row {
    Row {
        name,
        raw,
        due: None,
        threshold: None,
        extra: &[],
        invalid: &[],
    }
}

#[test]
fn due_and_threshold() {
    let rows = [
        Row {
            due: Some("2026-09-29"),
            threshold: Some("2026-09-27"),
            ..row("02.1 due and threshold", "Call due:2026-09-29 t:2026-09-27")
        },
        Row {
            invalid: &["due:2026-02-30"],
            ..row("02.2 impossible date", "Call due:2026-02-30")
        },
        Row {
            invalid: &["due:soon"],
            ..row("02.2 word value", "Call due:soon")
        },
        Row {
            invalid: &["due:2026-9-1"],
            ..row("02.2 unpadded date", "Call due:2026-9-1")
        },
        Row {
            due: Some("2026-10-01"),
            extra: &["due:2026-11-01"],
            ..row(
                "02.3 first due counts",
                "Call due:2026-10-01 due:2026-11-01",
            )
        },
        Row {
            extra: &["due:2026-10-01"],
            invalid: &["due:soon"],
            ..row("02.4 bad first due counts", "Call due:soon due:2026-10-01")
        },
        row("02.5 key is case sensitive", "Call Due:2026-10-01"),
        Row {
            threshold: Some("2024-02-29"),
            ..row("leap day", "Call t:2024-02-29")
        },
        Row {
            invalid: &["t:2025-02-29"],
            ..row("not a leap day", "Call t:2025-02-29")
        },
    ];
    for r in rows {
        let line = Line::parse(r.raw);
        let got = line.tags();
        assert_eq!(got.due, r.due, "{} due", r.name);
        assert_eq!(got.threshold, r.threshold, "{} threshold", r.name);
        assert_eq!(texts(&got.extra), r.extra, "{} extra", r.name);
        assert_eq!(texts(&got.invalid), r.invalid, "{} invalid", r.name);
    }
}

#[test]
fn rec() {
    let ok = |strict, n, unit| Rec::new(strict, n, unit);
    let rows = [
        ("02.6 1w", "rec:1w", ok(false, 1, RecUnit::Week)),
        ("02.6 strict", "rec:+1w", ok(true, 1, RecUnit::Week)),
        ("02.6 business", "rec:3b", ok(false, 3, RecUnit::Business)),
        ("02.6 12m", "rec:12m", ok(false, 12, RecUnit::Month)),
        ("02.6 1y", "rec:1y", ok(false, 1, RecUnit::Year)),
        ("02.6 max", "rec:9999d", ok(false, 9999, RecUnit::Day)),
        ("02.7 zero", "rec:0d", None),
        ("02.7 no number", "rec:w", None),
        ("02.7 bad unit", "rec:1x", None),
        ("02.7 negative", "rec:-1w", None),
        ("02.7 leading 0", "rec:01d", None),
        ("02.7 upper unit", "rec:1W", None),
        ("02.7 too big", "rec:10000d", None),
        ("plus only", "rec:+", None),
        ("plus no number", "rec:+d", None),
        ("double plus", "rec:++1d", None),
        ("long number", "rec:99999999999999999999d", None),
    ];
    for (name, raw, want) in rows {
        let line = Line::parse(raw);
        let got = line.tags();
        assert_eq!(got.rec, want, "{name}");
        assert_eq!(got.invalid.len(), usize::from(want.is_none()), "{name}");
    }
}

#[test]
fn rec_units_write_their_letter() {
    let letters: Vec<u8> = [
        RecUnit::Day,
        RecUnit::Week,
        RecUnit::Month,
        RecUnit::Year,
        RecUnit::Business,
    ]
    .map(RecUnit::letter)
    .to_vec();
    assert_eq!(letters, b"dwmyb");
}

#[test]
fn effective_priority() {
    let rows = [
        (
            "02.8 done uses pri tag",
            "x 2026-09-28 Fix pri:B",
            pri(b'B'),
            0,
        ),
        ("02.9 open uses field", "(A) Fix pri:B", pri(b'A'), 0),
        (
            "02.10 done field wins",
            "x (A) 2026-09-28 Fix pri:B",
            pri(b'A'),
            0,
        ),
        ("02.11 lowercase", "x 2026-09-28 Fix pri:b", None, 1),
        ("02.11 two letters", "x 2026-09-28 Fix pri:AB", None, 1),
        ("open pri tag is ordinary", "Fix pri:B", None, 0),
        ("first pri counts", "x Fix pri:C pri:D", pri(b'C'), 0),
    ];
    for (name, raw, want, invalid) in rows {
        let line = Line::parse(raw);
        assert_eq!(line.effective_priority(), want, "{name}");
        assert_eq!(line.tags().invalid.len(), invalid, "{name}");
    }
}

/// Name, line, `started` (RFC 3339), underway, owner, invalid tokens.
type ClaimRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    bool,
    Option<&'static str>,
    usize,
);

const CLAIM_ROWS: &[ClaimRow] = &[
    (
        "02.12 started and owner",
        "Add started:2026-09-28T1305Z owner:agent1",
        Some("2026-09-28T13:05:00Z"),
        true,
        Some("agent1"),
        0,
    ),
    (
        "02.13 two colons is a word",
        "Add started:2026-09-28T13:05Z",
        None,
        false,
        None,
        0,
    ),
    ("02.14 not a time", "Add started:yes", None, true, None, 1),
    (
        "02.14 impossible time",
        "Add started:2026-09-28T2505Z",
        None,
        true,
        None,
        1,
    ),
    (
        "02.14 one-digit hour",
        "Add started:2026-09-28T905Z",
        None,
        true,
        None,
        1,
    ),
    (
        "impossible date",
        "Add started:2026-02-30T0905Z",
        None,
        true,
        None,
        1,
    ),
    (
        "minute 60",
        "Add started:2026-09-28T1360Z",
        None,
        true,
        None,
        1,
    ),
    (
        "no zone letter",
        "Add started:2026-09-28T1305X",
        None,
        true,
        None,
        1,
    ),
    (
        "non-digit",
        "Add started:2026-09-28T13a5Z",
        None,
        true,
        None,
        1,
    ),
];

#[test]
fn started_and_owner() {
    for &(name, raw, started, underway, owner, invalid) in CLAIM_ROWS {
        let started: Option<Timestamp> = started.map(|s| s.parse().unwrap());
        let line = Line::parse(raw);
        let got = line.tags();
        assert_eq!(got.started, started, "{name} started");
        assert_eq!(got.underway, underway, "{name} underway");
        assert_eq!(got.owner, owner.map(str::as_bytes), "{name} owner");
        assert_eq!(got.invalid.len(), invalid, "{name} invalid");
    }
}

#[test]
fn deps_and_id() {
    let rows: [IdRow; 3] = [
        (
            "02.15 deps deduplicated",
            "Add dep:a dep:b dep:a",
            None,
            &["a", "b"],
            &[],
        ),
        (
            "02.16 first id counts",
            "Add id:k3f9 id:x1",
            Some("k3f9"),
            &[],
            &["id:x1"],
        ),
        (
            "02.21 foreign id kept",
            "Add id:a,b dep:a,b",
            Some("a,b"),
            &["a,b"],
            &[],
        ),
    ];
    for (name, raw, id, deps, extra) in rows {
        let line = Line::parse(raw);
        let got = line.tags();
        assert_eq!(got.id, id.map(str::as_bytes), "{name} id");
        assert_eq!(strs(&got.deps), deps, "{name} deps");
        assert_eq!(texts(&got.extra), extra, "{name} extra");
    }
}

/// A file link as written and as decoded.
type RawAndDecoded<'a> = (&'a [u8], &'a [u8]);

#[test]
fn files() {
    let rows: [FileRow; 4] = [
        (
            "02.22 percent decoded",
            "file:docs/contract%20v2.pdf",
            &[("docs/contract%20v2.pdf", "docs/contract v2.pdf")],
        ),
        ("02.23 bad hex kept", "file:a%2Gb", &[("a%2Gb", "a%2Gb")]),
        (
            "02.23 trailing percent kept",
            "file:100%",
            &[("100%", "100%")],
        ),
        (
            "02.28 two files and untyped tags",
            "Review file:a.pdf file:b.pdf color:red h:1",
            &[("a.pdf", "a.pdf"), ("b.pdf", "b.pdf")],
        ),
    ];
    for (name, raw, want) in rows {
        let line = Line::parse(raw);
        let tags = line.tags();
        let got: Vec<RawAndDecoded<'_>> =
            tags.files.iter().map(|f| (f.raw, &*f.decoded)).collect();
        let want: Vec<RawAndDecoded<'_>> = want
            .iter()
            .map(|(a, b)| (a.as_bytes(), b.as_bytes()))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn untyped_tags_stay_tokens() {
    let line = Line::parse("Review color:red h:1");
    let got = line.tags();
    assert!(got.invalid.is_empty() && got.extra.is_empty(), "02.28");
    assert_eq!(line.tokens().len(), 3, "02.28");
}

#[test]
fn single_valued_keys() {
    for key in ["due", "t", "rec", "pri", "id", "started", "owner"] {
        assert!(super::is_single_valued(key.as_bytes()), "{key}");
    }
    for key in ["dep", "file", "Due", "color", ""] {
        assert!(!super::is_single_valued(key.as_bytes()), "{key}");
    }
}

#[test]
fn done_and_blank_lines_have_tags() {
    let line = Line::parse("x 2026-09-28 Fix due:2026-10-01 owner:bob");
    let got = line.tags();
    assert_eq!(got.due, Some("2026-10-01"));
    assert_eq!(got.owner, Some(b"bob".as_slice()));
    assert_eq!(Line::parse("   ").tags(), super::Tags::default());
}

#[test]
fn rec_new_checks_the_count() {
    assert_eq!(Rec::new(false, 0, RecUnit::Day), None);
    assert_eq!(Rec::new(false, 10000, RecUnit::Day), None);
    let rec = Rec::new(true, 9999, RecUnit::Day).expect("in range");
    assert_eq!(
        (rec.strict(), rec.n(), rec.unit()),
        (true, 9999, RecUnit::Day)
    );
}

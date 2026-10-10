use jiff::tz::TimeZone;

use super::{
    Category, Context, Query,
    test_util::{date, hits, hits_bytes, hits_on, parse},
};
use crate::{list::test_util::snapshot, todotxt::Kind};

/// Name, query, line, expected.
type Row = (&'static str, &'static str, &'static str, bool);

const NAMES: &[Row] = &[
    ("09.1 empty", "", "Anything", true),
    ("09.2 exact", "+work", "A +work", true),
    ("09.2 dotted child", "+work", "A +work.acme", true),
    ("09.2 slash child", "+work", "A +work/q3", true),
    ("09.3 case", "+work", "A +Work", false),
    ("09.3 longer name", "+work", "A +workshop", false),
    ("09.3 dash", "+work", "A +work-q3", false),
    ("09.3 context", "+work", "A @work", false),
    ("09.4 any project", "+", "Call +work", true),
    ("09.4 any project none", "+", "Call", false),
    ("09.4 no project", "-+", "Call +work", false),
    ("09.4 no project none", "-+", "Call", true),
    ("09.5 and, one", "+ @", "Call +work", false),
    ("09.5 and, both", "+ @", "Call +work @phone", true),
    ("09.5 or, one", "+|@", "Call +work", true),
    ("09.5 or, both", "+|@", "Call +work @phone", true),
    ("09.6 negated context", "+work -@waiting", "A +work", true),
    (
        "09.6 negated hit",
        "+work -@waiting",
        "A +work @waiting",
        false,
    ),
    ("09.7 phone", "-@phone|@errand", "A @phone", false),
    ("09.7 errand", "-@phone|@errand", "A @errand", false),
    ("09.7 home", "-@phone|@errand", "A @home", true),
    ("context prefix", "@home", "A @home.kitchen", true),
    (
        "spaces between",
        "  +work   @phone ",
        "A +work @phone",
        true,
    ),
    (
        "09.8 in range",
        "pri:A|pri:B due:<=+3d",
        "(A) X due:2026-10-02",
        true,
    ),
    (
        "09.8 wrong pri",
        "pri:A|pri:B due:<=+3d",
        "(C) X due:2026-10-02",
        false,
    ),
    (
        "09.8 too late",
        "pri:A|pri:B due:<=+3d",
        "(B) X due:2026-10-03",
        false,
    ),
    ("09.8 no due", "pri:A|pri:B due:<=+3d", "(A) X", false),
    (
        "09.10 done pri tag",
        "pri:B",
        "x 2026-09-28 Fix pri:B",
        true,
    ),
    ("09.9 pri: with", "pri:", "(C) X", true),
    ("09.9 pri: without", "pri:", "X", false),
];

const DATES: &[Row] = &[
    ("09.11 today", "due:today", "X due:2026-09-29", true),
    ("09.11 today other", "due:today", "X due:2026-10-02", false),
    ("09.11 weekday", "due:fri", "X due:2026-10-02", true),
    ("09.11 weekday other", "due:fri", "X due:2026-09-29", false),
    ("09.11 iso", "due:2026-10-02", "X due:2026-10-02", true),
    ("09.12 before", "due:<today", "X due:2026-09-28", true),
    ("09.12 no field", "due:<today", "X", false),
    ("09.12 bad value", "due:<today", "X due:soon", false),
    ("09.13 negated no field", "-due:<today", "X", true),
    ("09.14 created in", "created:>=-7d", "2026-09-22 X", true),
    ("09.14 created out", "created:>=-7d", "2026-09-21 X", false),
    ("09.14 created none", "created:>=-7d", "X", false),
    (
        "09.15 completed",
        "completed:=yesterday",
        "x 2026-09-28 X",
        true,
    ),
    (
        "09.15 completed today",
        "completed:=yesterday",
        "x 2026-09-29 X",
        false,
    ),
    ("09.15 completed none", "completed:=yesterday", "x X", false),
    (
        "09.17 started before",
        "is:doing started:<-2d",
        "X started:2026-09-26T0900Z",
        true,
    ),
    (
        "09.17 started after",
        "is:doing started:<-2d",
        "X started:2026-09-28T0900Z",
        false,
    ),
    (
        "09.18 owner",
        "owner:agent1",
        "X started:2026-09-29T0800Z owner:agent1",
        true,
    ),
    (
        "09.18 owner any",
        "owner:",
        "X started:2026-09-29T0800Z owner:agent1",
        true,
    ),
    (
        "09.18 started any",
        "started:",
        "X started:2026-09-29T0800Z owner:agent1",
        true,
    ),
    (
        "09.19 owner case",
        "owner:Agent1",
        "X started:2026-09-29T0800Z owner:agent1",
        false,
    ),
    ("started bad", "started:", "X started:soon", false),
    ("created any", "created:", "2026-09-22 X", true),
    ("threshold", "t:<=today", "X t:2026-09-29", true),
    ("greater", "due:>today", "X due:2026-09-30", true),
    ("greater equal", "due:>=today", "X due:2026-09-29", true),
    ("09.20 any dep", "dep:b", "X dep:a dep:b", true),
    (
        "09.21 first counts",
        "due:2026-10-01",
        "X due:2026-11-01 due:2026-10-01",
        false,
    ),
    (
        "owner before due",
        "due:today",
        "X owner:a due:2026-09-29",
        true,
    ),
    ("key with colons", "a:b:c", "X a:b:c", true),
    ("not a real date", "due:", "X due:2026-02-30", false),
    ("far future", "due:>7980y", "X due:2026-10-02", false),
    ("far future below", "due:<7980y", "X due:2026-10-02", false),
    (
        "far future negated",
        "-due:>7980y",
        "X due:2026-10-02",
        true,
    ),
    ("far past", "due:>-2027y", "X due:2026-10-02", false),
    ("far past in range", "due:>-2001y", "X due:2026-10-02", true),
    ("far past below", "due:<-2001y", "X due:0020-01-01", true),
    (
        "far past below none",
        "due:<-2001y",
        "X due:2026-10-02",
        false,
    ),
    ("huge offset", "due:<9999y", "X due:2026-10-02", false),
    (
        "last day bound",
        "due:<9999-12-31",
        "X due:2026-10-01",
        true,
    ),
    (
        "last day bound above",
        "due:>9999-12-30",
        "X due:2026-10-01",
        false,
    ),
];

const WORDS: &[Row] = &[
    ("09.23 smartcase lower", "invoice", "Send Invoice", true),
    ("09.23 other", "invoice", "Send bill", false),
    ("09.24 upper on lower", "Invoice", "Send invoice", false),
    ("09.24 upper on upper", "Invoice", "Send Invoice", true),
    ("09.25 phrase", "\"two words\"", "one two words", true),
    ("09.25 phrase spacing", "\"two words\"", "two  words", false),
    ("09.26 tag text", "k3f9", "X id:k3f9", true),
    ("09.26 fri is a word", "fri", "X due:2026-10-02", false),
    ("09.27 quoted dash", "\"-3d\"", "move -3d", true),
    ("09.27 quoted project", "\"+work\"", "X +work", true),
    ("09.27 quoted project none", "\"+work\"", "X", false),
    ("substring", "voic", "Send invoice", true),
    ("phrase alternative", "\"a b\"|zzz", "x a b", true),
    ("negated phrase", "-\"a b\"", "x a b", false),
    ("unicode case", "émile", "Call Émile", true),
    ("description has dates", "2026", "2026-09-22 X", false),
    ("description has tags", "due:", "X due:2026-10-02", true),
    ("done mark is not described", "x", "x 2026-09-28 Fib", false),
    ("negated dash word", "--3d", "move -3d", false),
    ("negated dash word, none", "--3d", "X", true),
    ("replacement char", "\u{fffd}", "X \u{fffd}", true),
    ("title-case letter", "ǅ", "X ǅ", true),
    ("title-case folds", "ǆ", "X ǅ", true),
];

fn run(rows: &[Row]) {
    for &(name, query, line, want) in rows {
        let got = hits(&parse(query), line);
        assert_eq!(got, want, "{name}: {query:?} on {line:?}");
    }
}

#[test]
fn names_and_priorities() {
    run(NAMES);
}

#[test]
fn dates() {
    run(DATES);
}

#[test]
fn words() {
    run(WORDS);
}

const PRIORITIES: &[(&str, &str)] = &[
    ("pri:A-C", "ABC"),
    ("pri:<=B", "AB"),
    ("pri:>B", "CD"),
    ("pri:>=B", "BCD"),
    ("pri:", "ABCD"),
    ("pri:=C", "C"),
    ("pri:<C", "AB"),
    ("pri:>=C", "CD"),
    ("pri:<A", ""),
    ("pri:D", "D"),
    ("pri:>Z", ""),
    ("pri:C-C", "C"),
];

#[test]
fn priority_ranges() {
    for &(query, want) in PRIORITIES {
        let q = parse(query);
        let got: String = ['A', 'B', 'C', 'D']
            .into_iter()
            .filter(|p| hits(&q, &format!("({p}) X")))
            .collect();
        assert_eq!(got, want, "{query}");
    }
}

#[test]
fn started_uses_the_local_date() {
    let zone = TimeZone::get("Australia/Sydney").expect("zone");
    let q = parse("started:today");
    let line = "X started:2026-09-29T1500Z";
    assert!(hits_on(&q, line, date(2026, 9, 30), &zone), "09.16 sydney");
    assert!(
        !hits_on(&q, line, date(2026, 9, 30), &TimeZone::UTC),
        "utc is the 29th"
    );
}

#[test]
fn a_saved_filter_follows_the_calendar() {
    let q = parse("due:today");
    let line = "X due:2026-09-30";
    let utc = TimeZone::UTC;
    assert!(!hits_on(&q, line, date(2026, 9, 29), &utc), "09.35 29th");
    assert!(hits_on(&q, line, date(2026, 9, 30), &utc), "09.35 30th");
}

const TODO: &[&str] = &[
    "Plain",
    "Pre id:pre",
    "Blocked dep:pre",
    "Later t:2026-10-05",
    "Over due:2026-09-28",
    "Dangling dep:nope",
    "Agent started:2026-09-29T0100Z owner:agent1",
    "Doing started:2026-09-29T0800Z",
    "x 2026-09-28 Finished",
];

const STATES: &[(&str, &str)] = &[
    ("is:open", "Plain"),
    ("is:done", "Finished"),
    ("is:doing", "Doing"),
    ("is:blocked", "Blocked"),
    ("is:later", "Later"),
    ("is:overdue", "Over"),
    ("is:archived", "Archived"),
    ("is:dangling", "Dangling"),
    ("is:stale", "Agent"),
];

#[test]
fn states_match_their_own_item() {
    let snap = snapshot(TODO, &["Archived"]);
    let zone = TimeZone::UTC;
    let ctx = Context {
        today: date(2026, 9, 29),
        zone: &zone,
    };
    let named = |name: &str| {
        snap.items().iter().find(|item| {
            snap.line(item).description().starts_with(name.as_bytes())
        })
    };
    let plain = named("Plain").expect("plain");
    for &(query, own) in STATES {
        let q = parse(query);
        let item = named(own).expect("item");
        assert!(q.matches(&snap, item, &ctx), "09.22 {query} on {own}");
        let on_plain = q.matches(&snap, plain, &ctx);
        assert_eq!(on_plain, own == "Plain", "09.22 {query} on Plain");
    }
}

const OVERRIDES: &[(&str, &[Category])] = &[
    ("is:later +work", &[Category::Later]),
    ("-is:later", &[]),
    ("pri:A|is:done", &[Category::Done]),
    (
        "is:archived is:blocked",
        &[Category::Blocked, Category::Archived],
    ),
    ("is:open is:doing is:overdue", &[]),
];

#[test]
fn overrides_name_the_categories_asked_for() {
    for &(query, want) in OVERRIDES {
        let mut got = parse(query).overrides();
        let mut want = want.to_vec();
        got.sort();
        want.sort();
        assert_eq!(got, want, "09.30 {query}");
    }
}

#[test]
fn asks_for_ignores_negated_terms() {
    let rows = [
        ("is:later", Category::Later, true),
        ("-is:later", Category::Later, false),
        ("pri:A|is:done", Category::Done, true),
        ("is:done", Category::Later, false),
        ("+work", Category::Archived, false),
    ];
    for (query, cat, want) in rows {
        assert_eq!(parse(query).asks_for(cat), want, "{query}");
    }
}

#[test]
fn and_needs_every_query() {
    let q = Query::and([parse("+work"), parse("is:later"), Query::default()]);
    assert_eq!(q.overrides(), [Category::Later]);
    assert!(!hits(&q, "A +work"));
    assert!(hits(&Query::and([]), "A"));
}

#[test]
fn word_and_name_queries() {
    let rows = [
        (Query::word("+work"), "X +work", true),
        (Query::word("due:today"), "X due:2026-09-29", false),
        (Query::word("Inv"), "Send invoice", false),
        (Query::word("inv"), "Send Invoice", true),
        (Query::name(Kind::Project, b"work"), "X +work.acme", true),
        (Query::name(Kind::Context, b"work"), "X +work", false),
        (Query::name(Kind::Project, b"a|b"), "X +a|b", true),
    ];
    for (q, line, want) in rows {
        assert_eq!(hits(&q, line), want, "{line}");
    }
}

#[test]
fn a_description_that_is_not_utf8_still_matches() {
    let mut line = b"Send ".to_vec();
    line.push(0xff);
    line.extend_from_slice(b" Invoice\n");
    assert!(hits_bytes(&parse("invoice"), &line));
    assert!(hits_bytes(&parse("Invoice"), &line));
    assert!(!hits_bytes(&parse("bill"), &line));
}

#[test]
fn an_instant_before_the_calendar_is_before_every_date() {
    let zone = TimeZone::get("America/New_York").expect("zone");
    let line = "X started:0000-01-01T0000Z";
    let today = date(2026, 9, 29);
    let rows = [
        ("started:", true),
        ("started:<today", true),
        ("started:<=0000-01-01", true),
        ("started:>today", false),
        ("started:today", false),
        ("-started:<today", false),
    ];
    for (query, want) in rows {
        assert_eq!(
            hits_on(&parse(query), line, today, &zone),
            want,
            "{query}"
        );
    }
}

#[test]
fn a_not_utf8_byte_is_one_replacement_character() {
    let mut line = b"Send ".to_vec();
    line.extend_from_slice(&[0xe2, 0x82, b' ', b'x', b'\n']);
    assert!(hits_bytes(&parse("\"send \u{fffd}\u{fffd} x\""), &line));
    assert!(!hits_bytes(&parse("\"send \u{fffd} x\""), &line));
}

use super::{
    Kind,
    Kind::{Context, Project, Tag, Word},
    Line,
};
use crate::errs::Nr;

#[derive(Debug, Default, PartialEq, Eq)]
struct Fields {
    blank: bool,
    done: bool,
    priority: Option<u8>,
    completed: Option<String>,
    created: Option<String>,
    description: String,
}

fn fields_of(raw: &str) -> Fields {
    let line = Line::parse(raw);
    Fields {
        blank: line.blank,
        done: line.is_done(),
        priority: line.priority().map(super::Priority::letter),
        completed: line.completed().map(str::to_owned),
        created: line.created().map(str::to_owned),
        description: String::from_utf8_lossy(line.description()).into_owned(),
    }
}

fn d(s: &str) -> String {
    s.to_owned()
}

fn open(description: &'static str) -> Fields {
    Fields {
        description: description.to_owned(),
        ..Fields::default()
    }
}

#[test]
#[expect(clippy::too_many_lines, reason = "a table of rows")]
fn line_fields() {
    let rows: Vec<(&str, &str, Fields)> = vec![
        (
            "01.1 open with priority and created",
            "(A) 2026-09-20 Call client +work @phone due:2026-09-29",
            Fields {
                priority: Some(b'A'),
                created: Some(d("2026-09-20")),
                description: d("Call client +work @phone due:2026-09-29"),
                ..Fields::default()
            },
        ),
        (
            "01.2 done with both dates",
            "x 2026-09-28 2026-09-20 Fix bug +work pri:B",
            Fields {
                done: true,
                completed: Some(d("2026-09-28")),
                created: Some(d("2026-09-20")),
                description: d("Fix bug +work pri:B"),
                ..Fields::default()
            },
        ),
        (
            "01.3 done with completion date only",
            "x 2026-09-28 Fix bug",
            Fields {
                done: true,
                completed: Some(d("2026-09-28")),
                description: d("Fix bug"),
                ..Fields::default()
            },
        ),
        (
            "01.4 done with no dates",
            "x Fix bug",
            Fields {
                done: true,
                description: d("Fix bug"),
                ..Fields::default()
            },
        ),
        (
            "01.5 done with priority and both dates",
            "x (A) 2016-05-20 2016-04-30 measure space",
            Fields {
                done: true,
                priority: Some(b'A'),
                completed: Some(d("2016-05-20")),
                created: Some(d("2016-04-30")),
                description: d("measure space"),
                ..Fields::default()
            },
        ),
        ("01.6 uppercase X is open", "X Fix", open("X Fix")),
        ("01.6 x without space is open", "xFix", open("xFix")),
        (
            "01.6 word starting with x is open",
            "xylophone lesson",
            open("xylophone lesson"),
        ),
        ("01.7 lowercase priority", "(a) Buy", open("(a) Buy")),
        ("01.7 two-letter priority", "(AB) Buy", open("(AB) Buy")),
        ("01.7 priority without space", "(A)Buy", open("(A)Buy")),
        (
            "01.7 priority not in position",
            "Buy (A) milk",
            open("Buy (A) milk"),
        ),
        (
            "01.8 two spaces after priority",
            "(A)  Buy",
            Fields {
                priority: Some(b'A'),
                description: d(" Buy"),
                ..Fields::default()
            },
        ),
        (
            "01.9 impossible day",
            "2026-02-30 Buy",
            open("2026-02-30 Buy"),
        ),
        (
            "01.9 impossible month",
            "2026-13-01 Buy",
            open("2026-13-01 Buy"),
        ),
        (
            "01.9 unpadded month and day",
            "2026-9-1 Buy",
            open("2026-9-1 Buy"),
        ),
        (
            "01.10 date alone on a line",
            "2026-09-29",
            open("2026-09-29"),
        ),
        (
            "01.11 date at the end",
            "Buy milk 2026-09-29",
            open("Buy milk 2026-09-29"),
        ),
        (
            "01.15 empty line",
            "",
            Fields {
                blank: true,
                ..Fields::default()
            },
        ),
        (
            "01.15 spaces",
            "   ",
            Fields {
                blank: true,
                ..Fields::default()
            },
        ),
        (
            "01.15 tabs",
            "\t\t",
            Fields {
                blank: true,
                ..Fields::default()
            },
        ),
        ("01.15 carriage return is text", " \r", open(" \r")),
        ("01.15 no-break space is text", "\u{a0}", open("\u{a0}")),
        (
            "leap day of a year whose century digits count",
            "2104-02-29 Buy",
            Fields {
                created: Some(d("2104-02-29")),
                description: d("Buy"),
                ..Fields::default()
            },
        ),
        (
            "century is not a leap year",
            "1900-02-29 Buy",
            open("1900-02-29 Buy"),
        ),
        (
            "every fourth century is",
            "2000-02-29 Buy",
            Fields {
                created: Some(d("2000-02-29")),
                description: d("Buy"),
                ..Fields::default()
            },
        ),
        (
            "letters in a date",
            "2026-0a-01 Buy",
            open("2026-0a-01 Buy"),
        ),
        (
            "priority without dates",
            "(B) Buy milk",
            Fields {
                priority: Some(b'B'),
                description: d("Buy milk"),
                ..Fields::default()
            },
        ),
        (
            "date after a done mark without a space",
            "x2026-09-28 Buy",
            open("x2026-09-28 Buy"),
        ),
        (
            "done mark and nothing else",
            "x ",
            Fields {
                done: true,
                ..Fields::default()
            },
        ),
        ("a lone x is open", "x", open("x")),
        (
            "second date on an open line is description",
            "2026-09-20 2026-09-21 Buy",
            Fields {
                created: Some(d("2026-09-20")),
                description: d("2026-09-21 Buy"),
                ..Fields::default()
            },
        ),
        (
            "priority after the date is description",
            "2026-09-20 (A) Buy",
            Fields {
                created: Some(d("2026-09-20")),
                description: d("(A) Buy"),
                ..Fields::default()
            },
        ),
    ];
    for (name, raw, want) in rows {
        assert_eq!(fields_of(raw), want, "{name}: {raw:?}");
    }
}

type Tok = (Kind, &'static str, &'static str, &'static str);

fn tokens_of(raw: &str) -> Vec<(Kind, String, String, String)> {
    let line = Line::parse(raw);
    line.tokens()
        .map(|t| {
            let s = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
            (t.kind, s(t.text), s(t.key), s(t.value))
        })
        .collect()
}

/// Name, line, its tokens.
type TokenRow = (&'static str, &'static str, &'static [Tok]);

const TOKEN_ROWS: &[TokenRow] = &[
    (
        "01.1 words, project, context and tag",
        "(A) 2026-09-20 Call client +work @phone due:2026-09-29",
        &[
            (Word, "Call", "", ""),
            (Word, "client", "", ""),
            (Project, "+work", "", "work"),
            (Context, "@phone", "", "phone"),
            (Tag, "due:2026-09-29", "due", "2026-09-29"),
        ],
    ),
    (
        "01.2 tag on a done line",
        "x 2026-09-28 2026-09-20 Fix bug +work pri:B",
        &[
            (Word, "Fix", "", ""),
            (Word, "bug", "", ""),
            (Project, "+work", "", "work"),
            (Tag, "pri:B", "pri", "B"),
        ],
    ),
    (
        "01.10 date alone is one word",
        "2026-09-29",
        &[(Word, "2026-09-29", "", "")],
    ),
    (
        "01.11 trailing date is a word",
        "Buy milk 2026-09-29",
        &[
            (Word, "Buy", "", ""),
            (Word, "milk", "", ""),
            (Word, "2026-09-29", "", ""),
        ],
    ),
    (
        "01.12 colons",
        "See https://x a:b:c :x x: 12:30",
        &[
            (Word, "See", "", ""),
            (Word, "https://x", "", ""),
            (Word, "a:b:c", "", ""),
            (Word, ":x", "", ""),
            (Word, "x:", "", ""),
            (Tag, "12:30", "12", "30"),
        ],
    ),
    (
        "01.13 plus and at signs",
        "+work Call +a:b + @ a@b.com 1+1",
        &[
            (Project, "+work", "", "work"),
            (Word, "Call", "", ""),
            (Project, "+a:b", "", "a:b"),
            (Word, "+", "", ""),
            (Word, "@", "", ""),
            (Word, "a@b.com", "", ""),
            (Word, "1+1", "", ""),
        ],
    ),
    (
        "01.14 names are case-sensitive",
        "Call +Work +work",
        &[
            (Word, "Call", "", ""),
            (Project, "+Work", "", "Work"),
            (Project, "+work", "", "work"),
        ],
    ),
    (
        "01.8 the extra space is not a token",
        "(A)  Buy",
        &[(Word, "Buy", "", "")],
    ),
    (
        "runs of spaces and tabs separate words",
        "a \t  b",
        &[(Word, "a", "", ""), (Word, "b", "", "")],
    ),
    ("blank line has no tokens", "  ", &[]),
    (
        "a context with a colon is a context",
        "@a:b",
        &[(Context, "@a:b", "", "a:b")],
    ),
    (
        "trailing punctuation is not part of a name",
        "Ship it for +work. Then @phone, +work.acme. (+a.) +a/ @b!?",
        &[
            (Word, "Ship", "", ""),
            (Word, "it", "", ""),
            (Word, "for", "", ""),
            (Project, "+work.", "", "work"),
            (Word, "Then", "", ""),
            (Context, "@phone,", "", "phone"),
            (Project, "+work.acme.", "", "work.acme"),
            (Word, "(+a.)", "", ""),
            (Project, "+a/", "", "a"),
            (Context, "@b!?", "", "b"),
        ],
    ),
    (
        "interior punctuation stays",
        "+a-b +work/q3 +a..b +a./b. @x_y;",
        &[
            (Project, "+a-b", "", "a-b"),
            (Project, "+work/q3", "", "work/q3"),
            (Project, "+a..b", "", "a..b"),
            (Project, "+a./b.", "", "a./b"),
            (Context, "@x_y;", "", "x_y"),
        ],
    ),
    (
        "a sigil with only sentence marks after it is a plain word",
        "+. +! @- @.. +_ +c++ @c++ +c# x +.",
        &[
            (Word, "+.", "", ""),
            (Word, "+!", "", ""),
            (Context, "@-", "", "-"),
            (Word, "@..", "", ""),
            (Project, "+_", "", "_"),
            (Project, "+c++", "", "c++"),
            (Context, "@c++", "", "c++"),
            (Project, "+c#", "", "c#"),
            (Word, "x", "", ""),
            (Word, "+.", "", ""),
        ],
    ),
    (
        "non-ASCII ends a name",
        "+café. @naïve é+é. +日本。",
        &[
            (Project, "+café.", "", "café"),
            (Context, "@naïve", "", "naïve"),
            (Word, "é+é.", "", ""),
            (Project, "+日本。", "", "日本。"),
        ],
    ),
    (
        "done task",
        "x 2026-09-28 Ship +work. @phone.",
        &[
            (Word, "Ship", "", ""),
            (Project, "+work.", "", "work"),
            (Context, "@phone.", "", "phone"),
        ],
    ),
    (
        "a URL is text, whatever the scheme",
        "https://x.org http:// ftp://h/p x-y+z.1://a (https://x.org) \
         http://h:80/p https://a/+b/@c",
        &[
            (Word, "https://x.org", "", ""),
            (Word, "http://", "", ""),
            (Word, "ftp://h/p", "", ""),
            (Word, "x-y+z.1://a", "", ""),
            (Word, "(https://x.org)", "", ""),
            (Word, "http://h:80/p", "", ""),
            (Word, "https://a/+b/@c", "", ""),
        ],
    ),
    (
        "a value that is not two slashes keeps the tag",
        "mailto:a@b key:value a:/b a:b//c a://",
        &[
            (Tag, "mailto:a@b", "mailto", "a@b"),
            (Tag, "key:value", "key", "value"),
            (Tag, "a:/b", "a", "/b"),
            (Tag, "a:b//c", "a", "b//c"),
            (Word, "a://", "", ""),
        ],
    ),
    (
        "file keeps its tag in every form",
        "file:x file://x file:///a/b ://x",
        &[
            (Tag, "file:x", "file", "x"),
            (Tag, "file://x", "file", "//x"),
            (Tag, "file:///a/b", "file", "///a/b"),
            (Word, "://x", "", ""),
        ],
    ),
    (
        "a tag with a plus in the key is a tag",
        "a+b:c",
        &[(Tag, "a+b:c", "a+b", "c")],
    ),
];

#[test]
fn line_tokens() {
    for (name, raw, want) in TOKEN_ROWS {
        let want: Vec<_> = want
            .iter()
            .map(|&(k, t, a, v)| (k, t.to_owned(), a.to_owned(), v.to_owned()))
            .collect();
        assert_eq!(tokens_of(raw), want, "{name}: {raw:?}");
    }
}

#[test]
fn token_start_offsets_point_into_raw() {
    let raw = "(A) 2026-09-20 Call +work  due:2026-09-29";
    let line = Line::parse(raw);
    let starts: Vec<_> = line.tokens().map(|t| t.start).collect();
    assert_eq!(starts, [15, 20, 27]);
    for tok in line.tokens() {
        assert_eq!(
            &line.raw()[tok.start..tok.start + tok.text.len()],
            tok.text
        );
    }
}

#[test]
fn token_accessors_agree() {
    let line = Line::parse("Call +work");
    assert_eq!(line.tokens().len(), 2);
    assert_eq!(line.token(1).map(|t| t.kind), Some(Project));
    assert_eq!(line.token(2), None);
}

#[test]
fn invalid_utf8_is_parsed_as_bytes() {
    let line = Line::parse(b"(A) Buy \xff\xfe a:\xff");
    assert_eq!(line.priority().map(super::Priority::letter), Some(b'A'));
    let kinds: Vec<_> = line.tokens().map(|t| t.kind).collect();
    assert_eq!(kinds, [Word, Word, Tag]);
    assert_eq!(line.raw(), b"(A) Buy \xff\xfe a:\xff");
}

#[test]
fn blank_lines_of_a_file_share_one_allocation() {
    let file = super::File::parse(b"a\n\n\nb\n");
    let at = |n| {
        Nr::from_index(n)
            .and_then(|nr| file.line(nr))
            .map(Line::shared)
    };
    let (x, y) = (at(1), at(2));
    assert!(x.as_ref().is_some_and(|b| b.is_empty()));
    assert!(
        matches!((x, y), (Some(x), Some(y)) if std::sync::Arc::ptr_eq(&x, &y))
    );
}

/// The words of `raw` that backticks quote, `*` marked, and the kinds of
/// the others.
fn quoting(raw: impl AsRef<[u8]>) -> String {
    let line = Line::parse(raw.as_ref());
    let words: Vec<String> = line
        .tokens()
        .map(|t| {
            let kind = match t.kind {
                Word => 'w',
                Project => 'p',
                Context => 'c',
                Tag => 't',
            };
            format!("{kind}{}", if t.quoted { "*" } else { "" })
        })
        .collect();
    words.join(" ")
}

#[test]
fn backticks_quote_words() {
    let rows = [
        ("no backtick", "a +p @c k:v", "w p c t"),
        ("span with spaces", "a `+p  @c\tk:v` +q", "w w* w* w* p"),
        ("span in one word", "`due:2026-01-01` x", "w* w"),
        ("touching before", "pre`a:b` c:d", "w* t"),
        ("touching after", "`+x`: +y", "w* p"),
        ("unpaired", "a ` +p k:v", "w w p t"),
        ("unpaired after span", "`a` ` +p", "w* w p"),
        ("empty pair", "`` +p", "w p"),
        ("empty pair then word", "x`` +p", "w p"),
        ("three", "``` +p", "w p"),
        ("two spans", "`a +b` +c `d @e`", "w* w* p w* w*"),
        ("head untouched", "(A) 2026-01-01 `+x` +y", "w* p"),
        ("done head", "x `a` +y", "w* p"),
        ("span ends line", "a `b", "w w"),
    ];
    for (name, raw, want) in rows {
        assert_eq!(quoting(raw), want, "{name}");
    }
    assert_eq!(quoting(b"`\xff +p\xfe` +q"), "w* w* p");
}

#[test]
fn a_cut_inside_a_span_is_told_from_one_beside_it() {
    let desc = b"a `b c` d `e`";
    let inside: Vec<usize> = (0..=desc.len())
        .filter(|&at| super::inside_span(desc, at))
        .collect();
    assert_eq!(inside, [3, 4, 5, 6, 11, 12]);
}

#[test]
fn a_name_ends_before_trailing_punctuation_in_any_bytes() {
    let raw: &[u8] = b"a +caf\xff. @x\xc3. +w.";
    let line = Line::parse(raw);
    let names: Vec<_> = line
        .tokens()
        .map(|t| (t.kind, t.value, t.name_text()))
        .collect();
    assert_eq!(
        names,
        [
            (Word, &b""[..], &b"a"[..]),
            (Project, b"caf\xff", b"+caf\xff"),
            (Context, b"x\xc3", b"@x\xc3"),
            (Project, b"w", b"+w"),
        ]
    );
    assert_eq!(line.raw(), raw, "the raw bytes stay");
}

// A word of many names is cut in one scan: quadratic time otherwise.
#[test]
fn a_word_of_many_names_is_linear() {
    let line = Line::parse("+a".repeat(100_000).as_bytes());
    assert_eq!(line.tokens().len(), 100_000);
    let line = Line::parse(format!("{} @b", "@a+".repeat(100_000)).as_bytes());
    assert_eq!(line.tokens().len(), 100_001);
}

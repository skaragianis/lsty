use super::{Eol, Line, Priority, line::EditError};

fn text(raw: &str) -> String {
    String::from_utf8_lossy(&Line::parse(raw).text()).into_owned()
}

#[test]
fn text_drops_projects_contexts_and_tags() {
    let rows = [
        (
            "01.16 project, context and tag removed",
            "(A) 2026-09-20 Call client +work @phone due:2026-09-29",
            "Call client",
        ),
        (
            "01.17 runs of spaces collapse",
            "Add  CLI +spec02   ls id:k3f9",
            "Add CLI ls",
        ),
        ("token in the middle", "Call +work client", "Call client"),
        ("only tokens", "+work @phone due:2026-09-29", ""),
        (
            "done line drops marker and dates",
            "x 2026-09-28 2026-09-20 Fix bug +work",
            "Fix bug",
        ),
        ("words keep source order", "c b a", "c b a"),
        ("tabs become single spaces", "a\tb", "a b"),
        ("blank line", "   ", ""),
        ("leading space in description", "(A)  Buy", "Buy"),
    ];
    for (name, raw, want) in rows {
        assert_eq!(text(raw), want, "{name}");
    }
}

struct Remove {
    name: &'static str,
    raw: &'static str,
    eol: Eol,
    index: usize,
    want: &'static str,
    stuck: bool,
}

const fn remove(
    name: &'static str,
    raw: &'static str,
    index: usize,
    want: &'static str,
) -> Remove {
    Remove {
        name,
        raw,
        eol: Eol::None,
        index,
        want,
        stuck: false,
    }
}

const fn stuck(
    name: &'static str,
    raw: &'static str,
    index: usize,
    want: &'static str,
) -> Remove {
    Remove {
        name,
        raw,
        eol: Eol::None,
        index,
        want,
        stuck: true,
    }
}

fn with_eol(raw: &str, eol: Eol) -> Line {
    let mut line = Line::parse(raw);
    line.eol = eol;
    line
}

#[test]
#[expect(clippy::too_many_lines, reason = "a table of rows")]
fn remove_token() {
    let rows = [
        remove(
            "01.27 context from 01.1",
            "(A) 2026-09-20 Call client +work @phone due:2026-09-29",
            3,
            "(A) 2026-09-20 Call client +work due:2026-09-29",
        ),
        remove("01.27 first word", "Call client", 0, "client"),
        remove("last word", "Call client", 1, "Call"),
        remove(
            "first word after priority and date",
            "(A) 2026-09-20 Call client",
            0,
            "(A) 2026-09-20 client",
        ),
        remove(
            "first word of done line",
            "x 2026-09-28 Fix bug",
            0,
            "x 2026-09-28 bug",
        ),
        remove("first token is a project", "+work Call", 0, "Call"),
        remove("only word", "Buy", 0, ""),
        remove(
            "only word after priority keeps priority",
            "(A) Buy",
            0,
            "(A) ",
        ),
        remove("only word after leading space", "(A)  Buy", 0, "(A)  "),
        remove(
            "first word after leading space",
            "(A)  Buy milk",
            0,
            "(A)  milk",
        ),
        remove("run of spaces keeps the rest", "Add  CLI ls", 1, "Add  ls"),
        remove("last word after run of spaces", "Add  CLI", 1, "Add "),
        remove("tab before", "a\tb c", 1, "a c"),
        remove("tab after first word", "Call\tclient", 0, "client"),
        remove(
            "duplicate removes the one at index",
            "Call +work @home +work",
            1,
            "Call @home +work",
        ),
        Remove {
            eol: Eol::CrLf,
            ..remove("crlf ending kept", "Call client", 0, "client")
        },
        Remove {
            eol: Eol::Lf,
            ..remove("lf ending kept", "Call client", 1, "Call")
        },
        stuck(
            "first token ahead of a done mark word",
            "@home x foo",
            0,
            "x foo",
        ),
        stuck(
            "first token ahead of a done mark word after a tab",
            "Call\tx foo",
            0,
            "x foo",
        ),
        stuck(
            "first token ahead of a priority word",
            "due:2026-10-01 (B) call",
            0,
            "(B) call",
        ),
        stuck(
            "first token ahead of a creation date word",
            "(A) due:2026-10-10 2026-09-01 call",
            0,
            "(A) 2026-09-01 call",
        ),
        stuck(
            "first token ahead of a date word on a done line",
            "x 2026-09-28 Fix 2026-09-01 bug",
            0,
            "x 2026-09-28 2026-09-01 bug",
        ),
        remove(
            "a later token never exposes a word",
            "Call x foo",
            2,
            "Call x",
        ),
    ];
    for row in rows {
        let line = with_eol(row.raw, row.eol);
        let want = with_eol(row.want, row.eol);
        let got = match line.remove_token(row.index) {
            Ok(l) => {
                assert!(!row.stuck, "{}: expected a refusal", row.name);
                l
            }
            Err(EditError::Refused(l)) => {
                assert!(row.stuck, "{}: refused", row.name);
                l
            }
            Err(EditError::NoToken) => panic!("{}: no token", row.name),
        };
        assert_eq!(got, want, "{}", row.name);
    }
}

#[test]
fn remove_token_out_of_range() {
    let line = Line::parse("Call");
    assert_eq!(line.remove_token(1), Err(EditError::NoToken));
    assert_eq!(Line::parse("").remove_token(0), Err(EditError::NoToken));
}

#[test]
fn replace_token() {
    let rows = [
        (
            "tag value",
            "Call due:2026-10-01 +w",
            1,
            "due:2026-10-02",
            "Call due:2026-10-02 +w",
        ),
        (
            "first word",
            "(A) Call client",
            0,
            "Ring",
            "(A) Ring client",
        ),
        ("last word", "Call client", 1, "me", "Call me"),
        ("a token touches the next", "+@d", 0, "y", "y @d"),
        ("a token touches the previous", "@a+b`cd`", 1, "y", "@a y"),
    ];
    for (name, raw, index, new, want) in rows {
        for eol in [Eol::None, Eol::Lf, Eol::CrLf] {
            let got = with_eol(raw, eol).replace_token(index, new.as_bytes());
            assert_eq!(got, Some(with_eol(want, eol)), "{name}");
        }
    }
    assert_eq!(Line::parse("a").replace_token(1, b"b"), None);
}

#[test]
#[expect(clippy::too_many_lines, reason = "a table of rows")]
fn append_token() {
    // (name, raw, token, want, refused)
    let rows = [
        (
            "project",
            "Call client",
            "+work",
            "Call client +work",
            false,
        ),
        (
            "context",
            "Call client +work",
            "@phone",
            "Call client +work @phone",
            false,
        ),
        (
            "tag",
            "(A) 2026-09-20 Call client",
            "due:2026-09-29",
            "(A) 2026-09-20 Call client due:2026-09-29",
            false,
        ),
        ("word", "Call", "client", "Call client", false),
        (
            "done line",
            "x 2026-09-28 2026-09-20 Fix bug",
            "pri:A",
            "x 2026-09-28 2026-09-20 Fix bug pri:A",
            false,
        ),
        (
            "duplicate is appended",
            "Call +work",
            "+work",
            "Call +work +work",
            false,
        ),
        (
            "trailing spaces kept",
            "Call  ",
            "+work",
            "Call   +work",
            false,
        ),
        (
            "trailing tab kept",
            "Call\t",
            "+work",
            "Call\t +work",
            false,
        ),
        (
            "inner spacing kept",
            "Add  CLI\tls",
            "id:k3f9",
            "Add  CLI\tls id:k3f9",
            false,
        ),
        (
            "empty description after priority",
            "(A) ",
            "Buy",
            "(A) Buy",
            false,
        ),
        (
            "empty description after completion date",
            "x 2026-09-28 ",
            "Buy",
            "x 2026-09-28 Buy",
            false,
        ),
        ("empty line", "", "Buy", "Buy", false),
        (
            "whitespace-only description kept",
            "(A)  ",
            "Buy",
            "(A)  Buy",
            false,
        ),
        ("lone done mark word", "x", "id:k3f9", "x id:k3f9", true),
        ("lone priority word", "(A)", "+work", "(A) +work", true),
        (
            "lone date word",
            "2026-09-20",
            "id:k3f9",
            "2026-09-20 id:k3f9",
            true,
        ),
        (
            "priority and date word",
            "(B) 2026-09-20",
            "+work",
            "(B) 2026-09-20 +work",
            true,
        ),
        (
            "lone completion date word",
            "x 2026-09-28",
            "id:k3f9",
            "x 2026-09-28 id:k3f9",
            true,
        ),
        (
            "empty token is a no-op",
            "Call client",
            "",
            "Call client",
            false,
        ),
    ];
    for (name, raw, token, want, refused) in rows {
        for eol in [Eol::None, Eol::Lf, Eol::CrLf] {
            let line = with_eol(raw, eol);
            let got = match line.append_token(token.as_bytes()) {
                Ok(l) => {
                    assert!(!refused, "{name}: expected a refusal");
                    l
                }
                Err(EditError::Refused(l)) => {
                    assert!(refused, "{name}: refused");
                    l
                }
                Err(EditError::NoToken) => panic!("{name}: no token"),
            };
            assert_eq!(got, with_eol(want, eol), "{name}");
        }
    }
}

#[test]
fn accessors() {
    let line = Line::parse("(A) 2026-09-20 Call +work due:2026-09-29");
    assert_eq!(line.priority(), Some(Priority::A));
    assert_eq!(line.created(), Some("2026-09-20"));
    assert_eq!(line.completed(), None);
    assert_eq!(line.description(), b"Call +work due:2026-09-29");
    assert_eq!(line.tokens().len(), 3);

    let line = Line::parse("x 2026-09-28 2026-09-20 Fix bug");
    assert_eq!(line.completed(), Some("2026-09-28"));
    assert_eq!(line.created(), Some("2026-09-20"));
    assert_eq!(line.description(), b"Fix bug");
}

#[test]
fn priority_accepts_only_capital_letters() {
    assert_eq!(Priority::new(b'a'), None);
    assert_eq!(Priority::new(b'@'), None);
    assert_eq!(Priority::new(b'['), None);
    assert_eq!(Priority::new(b'Z').map(|p| p.to_string()), Some("Z".into()));
}

#[test]
fn as_ref_is_the_raw_bytes() {
    let line = Line::parse("(A) Call @phone");
    let bytes: &[u8] = line.as_ref();
    assert_eq!(bytes, line.raw());
    assert_eq!(bytes, b"(A) Call @phone");
}

#[test]
fn the_first_id_tag_counts_and_a_bare_id_is_no_tag() {
    type Row = (&'static [u8], Option<&'static [u8]>);
    let rows: [Row; 6] = [
        (b"x", None),
        (b"x id:", None),
        (b"x id:a", Some(b"a")),
        (b"x id: id:b", Some(b"b")),
        (b"x id:a id:b", Some(b"a")),
        (b"x xid:a", None),
    ];
    for (raw, want) in rows {
        assert_eq!(Line::parse(raw).tags().id, want, "{raw:?}");
    }
}

#[test]
fn removing_a_name_of_a_split_word_keeps_the_other_names() {
    let rows = [
        ("last name", "Call +work@home more", 2, "Call +work more"),
        ("first name", "Call +work@home more", 1, "Call @home more"),
        ("middle name", "Call +a@b+c", 2, "Call +a+c"),
        ("first word", "+a@b more", 0, "@b more"),
    ];
    for (name, raw, at, want) in rows {
        let line = Line::parse(raw);
        let got = line.remove_token(at).map(|l| l.raw().to_vec());
        assert_eq!(got.ok().as_deref(), Some(want.as_bytes()), "{name}");
    }
}

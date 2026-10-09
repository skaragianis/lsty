use super::test_util::{Dir, folder, parts, typed};
use super::{AddParams, Toggle, add};
use crate::errs::{Code, Nr};

const E: &str = "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01";

/// Name, file, the line number, the typed line, the file after and the
/// refusal or the messages that must be among those said.
type Row = (
    &'static str,
    &'static str,
    u32,
    &'static str,
    &'static str,
    Result<&'static [&'static str], (Code, &'static str)>,
);

const ROWS: &[Row] = &[
    (
        "38.5 text in front",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01\n",
        1,
        "(A) 2026-09-20 Re-Call client +work id:c1 due:2026-10-01",
        "(A) 2026-09-20 Re-Call client +work id:c1 due:2026-10-01\n",
        Ok(&[]),
    ),
    (
        "trailing spaces are dropped",
        "Buy\n",
        1,
        "Buy milk  ",
        "Buy milk\n",
        Ok(&[]),
    ),
    (
        "38.13 done marker",
        "Buy milk\n",
        1,
        "x Buy milk",
        "Buy milk\n",
        Err((Code::WrongState, "use lsty do 1")),
    ),
    (
        "38.15 creation date",
        "Buy milk\n",
        1,
        "2026-01-01 Buy milk",
        "Buy milk\n",
        Err((Code::WrongState, "the creation date is stamped")),
    ),
    (
        "38.16 second id",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01\n",
        1,
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01 id:zz",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01\n",
        Err((Code::WrongState, "")),
    ),
    (
        "38.17 id taken",
        "Buy\nCall id:c1\n",
        1,
        "Buy id:c1",
        "Buy\nCall id:c1\n",
        Err((Code::IdTaken, "")),
    ),
    (
        "38.18 own id",
        "Buy\n",
        1,
        "Buy id:mine",
        "Buy id:mine\n",
        Ok(&[]),
    ),
    (
        "38.19 fuzzy date",
        "Buy\n",
        1,
        "Buy due:fri",
        "Buy due:2026-10-02\n",
        Ok(&[]),
    ),
    (
        "38.20 bad date",
        "Buy\n",
        1,
        "Buy due:soon",
        "Buy due:soon\n",
        Ok(&["due:soon is not a date"]),
    ),
    (
        "bad t date",
        "Buy\n",
        1,
        "Buy t:later",
        "Buy t:later\n",
        Ok(&["t:later is not a date"]),
    ),
    (
        "a bad date already there is left alone",
        "Buy due:soon\n",
        1,
        "Buy milk due:soon",
        "Buy milk due:soon\n",
        Ok(&[]),
    ),
    (
        "bad recurrence",
        "Buy\n",
        1,
        "Buy rec:often",
        "Buy rec:often\n",
        Ok(&["rec:often is not valid"]),
    ),
    (
        "38.21 second due",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01\n",
        1,
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01 due:fri",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01 \
         due:2026-10-02\n",
        Ok(&["1 already has due:2026-10-01; the first one counts"]),
    ),
    (
        "38.22 cycle",
        "A id:a dep:b\nB id:b\n",
        2,
        "B id:b dep:a",
        "A id:a dep:b\nB id:b dep:a\n",
        Ok(&["2 is now part of a dependency cycle"]),
    ),
    (
        "38.23 nothing left",
        "Buy milk\n",
        1,
        "",
        "Buy milk\n",
        Err((Code::WrongState, "use dd to delete")),
    ),
    (
        "nothing left after the dates",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01\n",
        1,
        "(A) 2026-09-20 ",
        "(A) 2026-09-20 Call client +work id:c1 due:2026-10-01\n",
        Err((Code::WrongState, "use dd to delete")),
    ),
    (
        "a lone space after the date",
        "2026-09-20 Buy\n",
        1,
        "2026-09-20  ",
        "2026-09-20 Buy\n",
        Err((Code::WrongState, "use dd to delete")),
    ),
    (
        "only tags left",
        "Buy +home\n",
        1,
        " +home",
        " +home\n",
        Ok(&[]),
    ),
    (
        "38.46 claim",
        "Buy\n",
        1,
        "Buy started:2026-09-29T0900Z owner:agent1",
        "Buy started:2026-09-29T0900Z owner:agent1\n",
        Ok(&[]),
    ),
    (
        "38.47 blocked",
        "A id:a\nB dep:a\n",
        2,
        "B dep:a started:2026-09-29T0900Z",
        "A id:a\nB dep:a\n",
        Err((Code::Blocked, "")),
    ),
    (
        "started removed takes owner",
        "A started:2026-09-29T0800Z owner:agent1\n",
        1,
        "A",
        "A\n",
        Ok(&["1 is claimed by agent1"]),
    ),
    (
        "nothing left after the owner strip",
        "A started:2026-09-29T0800Z owner:agent1\n",
        1,
        "owner:agent1",
        "A started:2026-09-29T0800Z owner:agent1\n",
        Err((Code::WrongState, "use dd to delete")),
    ),
    (
        "38.49 done priority",
        "x 2026-09-28 Fix pri:B\n",
        1,
        "x 2026-09-28 Fix pri:A",
        "x 2026-09-28 Fix pri:B\n",
        Err((Code::WrongState, "")),
    ),
    (
        "a line break",
        "Buy\n",
        1,
        "Buy\nmilk",
        "Buy\n",
        Err((Code::InvalidValue, "")),
    ),
];

fn base_of(todo: &str, nr: u32) -> &str {
    todo.lines()
        .nth(usize::try_from(nr).unwrap_or(1) - 1)
        .unwrap_or_default()
}

fn check(dir: &Dir, row: &Row, got: crate::errs::Result<super::Outcome>) {
    let (name, _, _, _, file, want) = row;
    match (got, want) {
        (Ok(outcome), Ok(said)) => {
            let messages: Vec<String> = outcome
                .messages()
                .iter()
                .map(|m| String::from_utf8_lossy(m).into_owned())
                .collect();
            for line in *said {
                assert!(
                    messages.iter().any(|m| m == line),
                    "{name}: {messages:?} lack {line:?}"
                );
            }
        }
        (Err(err), Err((code, hint))) => {
            let (got, _, got_hint) = parts(&err);
            assert_eq!(got, *code, "{name}: {}", err.message());
            if !hint.is_empty() {
                assert_eq!(got_hint, *hint, "{name}");
            }
        }
        (Ok(_), Err((code, _))) => panic!("{name}: wanted {code:?}"),
        (Err(err), Ok(_)) => panic!("{name}: {}", err.message()),
    }
    assert_eq!(dir.todo(), *file, "{name}");
}

#[test]
fn a_typed_line_is_written_with_warnings() {
    for row in ROWS {
        let (_, todo, nr, draft, _, _) = row;
        let dir = folder(todo, None);
        let at = Nr::new(*nr).unwrap();
        let base = base_of(todo, *nr);
        let got = typed(&dir, at, base.as_bytes(), draft.as_bytes(), &[]);
        check(&dir, row, got);
    }
}

#[test]
fn a_changed_base_is_a_conflict() {
    let dir = folder("Phone client\n", None);
    let at = Nr::new(1).unwrap();
    let err =
        typed(&dir, at, b"Call client", b"Ring client", &[]).unwrap_err();
    assert_eq!(err.code(), Code::EditConflict);
    assert_eq!(dir.todo(), "Phone client\n");
}

#[test]
fn an_edit_keeps_the_record_for_undo() {
    let dir = folder(&format!("{E}\n"), None);
    let at = Nr::new(1).unwrap();
    let draft = format!("{E} more");
    let out = typed(&dir, at, E.as_bytes(), draft.as_bytes(), &[]).unwrap();
    assert!(out.undo().is_some());
}

/// The text of the new item, the file and what is said.
type Soft = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const SOFT: &[Soft] = &[
    ("fuzzy date", "Buy due:fri", "Buy due:2026-10-02\n", &[]),
    (
        "bad date",
        "Buy due:soon",
        "Buy due:soon\n",
        &["due:soon is not a date"],
    ),
    (
        "bad recurrence",
        "Buy rec:often",
        "Buy rec:often\n",
        &["rec:often is not valid"],
    ),
];

#[test]
fn a_new_item_is_written_with_warnings_when_asked() {
    for (name, text, file, said) in SOFT {
        let dir = folder("", None);
        let mut env = dir.env();
        env.cycle_warn_only = true;
        let params = AddParams {
            texts: vec![(*text).to_owned()],
            date: Toggle::Off,
            ..AddParams::default()
        };
        let out = add(&env, &params).unwrap();
        assert_eq!(dir.todo(), *file, "{name}");
        let messages: Vec<String> = out
            .messages()
            .iter()
            .map(|m| String::from_utf8_lossy(m).into_owned())
            .collect();
        for line in *said {
            assert!(
                messages.iter().any(|m| m == line),
                "{name}: {messages:?}"
            );
        }
    }
}

#[test]
fn a_new_item_with_a_bad_date_is_refused_otherwise() {
    let dir = folder("", None);
    let params = AddParams {
        texts: vec!["Buy due:soon".to_owned()],
        date: Toggle::Off,
        ..AddParams::default()
    };
    assert_eq!(
        add(&dir.env(), &params).unwrap_err().code(),
        Code::InvalidValue
    );
}

fn day() -> crate::dates::Date {
    crate::dates::Date::new(2026, 9, 29).unwrap()
}

/// The text after the fuzzy dates are written out, and the warnings.
fn soft(text: &str, old: &str) -> (String, Vec<String>) {
    let old = crate::todotxt::Line::parse(old);
    let (out, warns) = super::typed::soft_tags(text.as_bytes(), &old, day());
    let warns = warns
        .iter()
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect();
    (String::from_utf8(out).unwrap(), warns)
}

#[test]
fn typed_dates_are_written_out_and_bad_ones_are_kept_with_a_warning() {
    type Row = (
        &'static str,
        &'static str,
        &'static str,
        &'static [&'static str],
    );
    let rows: [Row; 13] = [
        ("x due:tomorrow", "x", "x due:2026-09-30", &[]),
        ("x t:+3d y", "x", "x t:2026-10-02 y", &[]),
        ("x due:zzz", "x", "x due:zzz", &["due:zzz is not a date"]),
        (
            "due:tomorrow due:zzz x",
            "x",
            "due:2026-09-30 due:zzz x",
            &["due:zzz is not a date"],
        ),
        ("x due:tomorrow", "x due:tomorrow", "x due:tomorrow", &[]),
        ("x due:zzz", "x due:zzz", "x due:zzz", &[]),
        ("x foo:tomorrow", "x", "x foo:tomorrow", &[]),
        ("x t:zzz", "x", "x t:zzz", &["t:zzz is not a date"]),
        ("x rec:zz", "x", "x rec:zz", &["rec:zz is not valid"]),
        ("x rec:+1w", "x", "x rec:+1w", &[]),
        ("x rec:1d", "x", "x rec:1d", &[]),
        ("x pri:zzz", "x", "x pri:zzz", &[]),
        ("x foo:zzz", "x", "x foo:zzz", &[]),
    ];
    for (text, old, want, warns) in rows {
        let (out, got) = soft(text, old);
        assert_eq!(out, want, "{text}");
        assert_eq!(got, warns, "{text}");
    }
}

#[test]
fn a_key_typed_twice_is_warned_about_once_for_the_first_value() {
    type Row = (&'static str, &'static str, &'static [&'static str]);
    let rows: [Row; 7] = [
        (
            "a due:1 due:2",
            "a",
            &["#3 already has due:1; the first one counts"],
        ),
        ("a due:1", "a", &[]),
        (
            "a due:1 due:2",
            "a due:1",
            &["#3 already has due:1; the first one counts"],
        ),
        ("a due:1 due:2", "a due:1 due:2", &[]),
        (
            "a due:1 due:2 due:3",
            "a due:1 due:2",
            &["#3 already has due:1; the first one counts"],
        ),
        ("a foo:1 foo:2", "a", &[]),
        (
            "a pri:A t:1 t:2",
            "a",
            &["#3 already has t:1; the first one counts"],
        ),
    ];
    for (typed, old, want) in rows {
        let old = crate::todotxt::Line::parse(old);
        let got = super::typed::repeated_keys("#3", &old, typed.as_bytes());
        let got: Vec<String> = got
            .iter()
            .map(|w| String::from_utf8_lossy(w).into_owned())
            .collect();
        assert_eq!(got, want, "{typed}");
    }
}

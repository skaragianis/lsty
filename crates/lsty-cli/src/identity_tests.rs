use std::ffi::OsString;

use lsty_core::errs::{Code, Field};

use super::commands::find;
use super::identity::identify;
use super::parse::parse;

/// Command line, `$LSTY_OWNER`, whether stdin is a terminal, and the
/// actor's name or the error's code and message.
type Row = (
    &'static str,
    Option<&'static str>,
    bool,
    Result<&'static str, (Code, &'static str)>,
);

const NO_ID: &str = "a write without a terminal must say who is acting";
const START: &str = "next --start needs an identity";

const ROWS: &[Row] = &[
    ("pri 1 A", None, false, Err((Code::NoIdentity, NO_ID))),
    ("pri 1 A --human", None, false, Ok("")),
    ("pri 1 A", None, true, Ok("")),
    ("pri 1 A", Some(""), false, Err((Code::NoIdentity, NO_ID))),
    ("ls", None, false, Ok("")),
    ("show 1", None, false, Ok("")),
    ("pri 1 A", Some("agent1"), false, Ok("agent1")),
    ("pri 1 A --owner agent1", None, false, Ok("agent1")),
    (
        "pri 1 A --owner agent2",
        Some("agent1"),
        false,
        Ok("agent2"),
    ),
    ("pri 1 A --human", Some("agent1"), false, Ok("")),
    ("pri 1 A --human", Some("agent1"), true, Ok("")),
    ("ls", Some("agent1"), true, Ok("agent1")),
    (
        "pri 1 A --owner a:b",
        None,
        true,
        Err((
            Code::InvalidValue,
            "an agent name has no whitespace or colon",
        )),
    ),
    (
        "ls",
        Some("my agent"),
        true,
        Err((
            Code::InvalidValue,
            "an agent name has no whitespace or colon",
        )),
    ),
    (
        "ls --owner=",
        None,
        true,
        Err((Code::InvalidValue, "an agent name is not empty")),
    ),
    ("next --start", None, true, Err((Code::NoIdentity, START))),
    (
        "next --start --human",
        None,
        true,
        Err((Code::NoIdentity, START)),
    ),
    ("next --start", Some("w1"), false, Ok("w1")),
    ("next", None, false, Ok("")),
];

#[test]
fn the_actor_and_the_refusal_follow_the_rules() {
    for (line, owner, tty, want) in ROWS {
        let tokens: Vec<String> = line.split(' ').map(str::to_owned).collect();
        let (parsed, result) = parse(&tokens);
        result.unwrap();
        let command = parsed.command.unwrap();
        let vars = |name: &str| {
            owner.filter(|_| name == "LSTY_OWNER").map(OsString::from)
        };
        let got = identify(command, &parsed.opts, &vars, *tty);
        match (got, want) {
            (Ok(actor), Ok(name)) => assert_eq!(actor.name(), *name, "{line}"),
            (Err(err), Err((code, message))) => {
                assert_eq!((err.code(), err.message()), (*code, *message));
            }
            (got, want) => panic!("{line}: {got:?} against {want:?}"),
        }
    }
}

#[test]
fn a_refused_write_says_how_to_fix_it() {
    let (parsed, _) = parse(&["pri".to_owned()]);
    let command = find("pri").unwrap();
    let err = identify(command, &parsed.opts, &|_| None, false).unwrap_err();
    assert_eq!(err.hint(), Some("set LSTY_OWNER, or pass --human"));
    assert_eq!(err.exit(), 2);
}

#[test]
fn next_start_asks_for_one_name_per_worker() {
    let tokens = ["next".to_owned(), "--start".to_owned()];
    let (parsed, _) = parse(&tokens);
    let command = parsed.command.unwrap();
    let err = identify(command, &parsed.opts, &|_| None, true).unwrap_err();
    assert_eq!(err.hint(), Some("set LSTY_OWNER, one name per worker"));
}

#[test]
fn a_name_that_is_not_utf8_is_refused() {
    use std::os::unix::ffi::OsStringExt as _;
    let (parsed, _) = parse(&["ls".to_owned()]);
    let command = parsed.command.unwrap();
    let vars = |_: &str| Some(OsString::from_vec(vec![b'a', 0xff]));
    let err = identify(command, &parsed.opts, &vars, true).unwrap_err();
    assert_eq!(
        (err.code(), err.message()),
        (Code::InvalidValue, "an agent name must be valid UTF-8")
    );
    let field = err.fields().find(|(key, _)| *key == "field");
    assert_eq!(field.map(|(_, v)| v), Some(&Field::Str("owner".to_owned())));
}

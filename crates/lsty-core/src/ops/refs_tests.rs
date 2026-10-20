use super::refs::{no_task, parse_one};
use crate::errs::{Code, Field, Source};

#[test]
fn reads_a_number_and_a_done_number() {
    let todo = parse_one("open", "012").unwrap();
    assert_eq!(
        (todo.line(), todo.given()),
        (Some((Source::Todo, 12)), Some("012"))
    );
    let done = parse_one("open", "done:3").unwrap();
    assert_eq!(done.line(), Some((Source::Done, 3)));
}

/// The argument and the code and message it fails with.
const BAD: &[(&str, Code, &str)] = &[
    ("x", Code::Usage, "\"x\" is not a reference"),
    ("", Code::Usage, "\"\" is not a reference"),
    ("a\x1bb", Code::Usage, "\"a\\x1bb\" is not a reference"),
    ("done:", Code::Usage, "\"done:\" is not a reference"),
    ("-1", Code::Usage, "\"-1\" is not a reference"),
    ("1,x", Code::Usage, "\"x\" is not a reference"),
    ("1,2", Code::Usage, "open takes one reference"),
    (
        "99999999999999999999",
        Code::Usage,
        "\"99999999999999999999\" is not a reference",
    ),
];

#[test]
fn refuses_anything_else() {
    for (arg, code, message) in BAD {
        let err = parse_one("open", arg).unwrap_err();
        assert_eq!((err.code(), err.message()), (*code, *message), "{arg:?}");
    }
}

#[test]
fn no_task_names_the_reference_it_was_given() {
    let by_id = parse_one("open", "id:abc").unwrap();
    let err = no_task(&by_id);
    assert_eq!(err.to_string(), "no task id:abc");
    assert_eq!(err.text("id"), Some("abc"));
    let line = no_task(&parse_one("open", "done:7").unwrap());
    assert_eq!(line.field("nr"), Some(&Field::Int(7)));
}

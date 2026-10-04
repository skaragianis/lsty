use crate::errs::{Code, Source};

use super::parse_one;

#[test]
fn reads_a_number_and_a_done_number() {
    let todo = parse_one("show", "012").unwrap();
    assert_eq!(
        (todo.line(), todo.given()),
        (Some((Source::Todo, 12)), "012")
    );
    let done = parse_one("show", "done:3").unwrap();
    assert_eq!(done.line(), Some((Source::Done, 3)));
}

/// The argument and the code and message it fails with.
const BAD: &[(&str, Code, &str)] = &[
    ("x", Code::Usage, "\"x\" is not a reference"),
    ("", Code::Usage, "\"\" is not a reference"),
    ("done:", Code::Usage, "\"done:\" is not a reference"),
    ("-1", Code::Usage, "\"-1\" is not a reference"),
    ("1,x", Code::Usage, "\"x\" is not a reference"),
    ("1,2", Code::Usage, "show takes one reference"),
    (
        "99999999999999999999",
        Code::Usage,
        "\"99999999999999999999\" is not a reference",
    ),
];

#[test]
fn refuses_anything_else() {
    for (arg, code, message) in BAD {
        let err = parse_one("show", arg).unwrap_err();
        assert_eq!((err.code(), err.message()), (*code, *message), "{arg:?}");
    }
}

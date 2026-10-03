use super::Query;
use super::test_util::{hits, parse};
use crate::errs::{Code, Error, Field};

/// Arguments, line, expected.
type ArgRow<'a> = (&'a [&'a str], &'a str, bool);

const BAD: &[&str] = &[
    "a|-b",
    "-",
    "a||b",
    "a|",
    "\"open",
    "a\"b",
    "\"\"",
    "\"a\"b\"c\"",
    "x \"",
    "due:soon",
    "due:<",
    "pri:b",
    "pri:C-A",
    "pri:1",
    "is:ready",
    "is:",
    "-|a",
    "due:<=",
    "pri:<=",
    "pri:AB",
];

fn field<'a>(err: &'a Error, key: &str) -> Option<&'a Field> {
    err.fields().find(|(k, _)| *k == key).map(|(_, v)| v)
}

#[test]
fn bad_filters_are_invalid_values_naming_the_term() {
    for input in BAD {
        let err = Query::parse(input).expect_err(input);
        assert_eq!(err.code(), Code::InvalidValue, "{input}");
        assert_eq!(
            field(&err, "field"),
            Some(&Field::Str("filter".into())),
            "{input}"
        );
        let Some(Field::Str(value)) = field(&err, "value") else {
            panic!("{input}: no value");
        };
        assert!(!value.is_empty(), "{input}");
        let quoted = format!("{value:?}");
        assert!(
            err.message().contains(&quoted),
            "{input}: {}",
            err.message()
        );
        assert!(
            matches!(field(&err, "accepted"), Some(Field::Strs(a)) if !a.is_empty()),
            "{input}"
        );
    }
}

#[test]
fn the_message_and_value_of_one_error() {
    let err = Query::parse("x due:soon").expect_err("bad");
    assert_eq!(err.message(), "filter: \"due:soon\" is not a date");
    assert_eq!(field(&err, "value"), Some(&Field::Str("due:soon".into())));
    let err = Query::parse("a \"b c").expect_err("bad");
    assert_eq!(
        err.message(),
        "filter: \"\\\"b c\" has a quote that is not closed"
    );
}

#[test]
fn arguments_are_one_term_each() {
    let rows: &[ArgRow<'_>] = &[
        (&["two words", "+work"], "two words +work", true),
        (&["two words", "+work"], "two +work words", false),
        (&["-@waiting"], "A @waiting", false),
        (&["pri:A|pri:B"], "(B) X", true),
        (&["-two words"], "two words", false),
        (&["a \"b\" | c"], "a \"b\" | c", true),
        (&[], "X", true),
    ];
    for &(args, line, want) in rows {
        let q = Query::parse_args(args).expect("args");
        assert_eq!(hits(&q, line), want, "{args:?} on {line:?}");
    }
}

#[test]
fn an_argument_parses_as_the_same_term_in_a_filter() {
    let lines = ["(A) X @waiting", "(B) X", "X"];
    for arg in ["-@waiting", "pri:A|pri:B"] {
        let from_args = Query::parse_args(&[arg]).expect("args");
        let from_parse = parse(arg);
        for line in lines {
            assert_eq!(
                hits(&from_args, line),
                hits(&from_parse, line),
                "{arg} on {line}"
            );
        }
    }
}

#[test]
fn a_bad_argument_is_an_error() {
    for arg in ["", "-", "is:x"] {
        let err = Query::parse_args(&[arg]).expect_err(arg);
        assert_eq!(err.code(), Code::InvalidValue, "{arg}");
    }
}

#[test]
fn quoting_follows_the_escapes_of_a_quoted_string() {
    let err = Query::parse("a\"b\"\u{1}").expect_err("bad");
    assert_eq!(
        err.message(),
        "filter: \"a\\\"b\\\"\\x01\" has a quote inside a word"
    );
}

#[test]
fn format_and_private_characters_are_escaped_in_messages() {
    let err = Query::parse("\"a\u{200d}b").expect_err("bad");
    assert_eq!(
        err.message(),
        "filter: \"\\\"a\\u200db\" has a quote that is not closed"
    );
    let err = Query::parse("is:\u{e000}\u{378}").expect_err("bad");
    assert_eq!(
        err.message(),
        "filter: \"is:\\ue000\\u0378\" is not a state"
    );
}

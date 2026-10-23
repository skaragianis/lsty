use super::{
    Query,
    test_util::{hits, parse},
};
use crate::errs::{Code, Field};

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

#[test]
fn bad_filters_are_invalid_values_naming_the_term() {
    for input in BAD {
        let err = Query::parse(input).expect_err(input);
        assert_eq!(err.code(), Code::InvalidValue, "{input}");
        assert_eq!(
            err.field("field"),
            Some(&Field::Str("filter".into())),
            "{input}"
        );
        let Some(Field::Str(value)) = err.field("value") else {
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
            matches!(
                err.field("accepted"),
                Some(Field::Strs(a)) if !a.is_empty()
            ),
            "{input}"
        );
    }
}

#[test]
fn the_message_and_value_of_one_error() {
    let err = Query::parse("x due:soon").expect_err("bad");
    assert_eq!(err.message(), "filter: \"due:soon\" is not a date");
    assert_eq!(err.field("value"), Some(&Field::Str("due:soon".into())));
    let err = Query::parse("a \"b c").expect_err("bad");
    assert_eq!(
        err.message(),
        "filter: \"\\\"b c\" has a quote that is not closed"
    );
}

#[test]
fn text_after_a_closed_phrase_is_named() {
    let after = "has text after a phrase";
    let rows = [
        ("\"ab\"c", after),
        ("\"a\"b\"c\"", after),
        ("\"\"", "has an empty phrase"),
    ];
    for (input, problem) in rows {
        let err = Query::parse(input).expect_err(input);
        assert!(err.message().ends_with(problem), "{input}");
    }
}

#[test]
fn a_bar_at_the_edge_of_a_term_hints_at_the_spaces() {
    let hint = Some("write a|b, without spaces around the bar");
    let rows = [
        ("a | b", "|", hint),
        ("a |b", "|b", hint),
        ("a| b", "a|", hint),
        ("-|a", "-|a", hint),
        ("+a|\"b\"|", "+a|\"b\"|", hint),
        ("a||b|", "a||b|", hint),
        ("\"|a\"||b", "\"|a\"||b", None),
        ("a||b", "a||b", None),
        ("a|b||c", "a|b||c", None),
    ];
    for (input, term, want) in rows {
        let err = Query::parse(input).expect_err(input);
        let message = format!("filter: {term:?} has an empty alternative");
        assert_eq!(err.message(), message, "{input}");
        assert_eq!(err.hint(), want, "{input}");
    }
    let err = Query::parse_args(&["a", "|", "b"]).expect_err("bar");
    assert_eq!(err.hint(), hint);
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

#[test]
fn operator_words_warn() {
    let rows: [(&str, &[&str]); 12] = [
        ("or", &["or"]),
        ("OR", &["OR"]),
        ("a Not b", &["Not"]),
        ("and", &["and"]),
        ("a or b and c", &["or", "and"]),
        ("\"or\"", &[]),
        ("-or", &[]),
        ("or|and", &[]),
        ("order", &[]),
        ("+or", &[]),
        ("or:x", &[]),
        ("", &[]),
    ];
    for (text, words) in rows {
        let found = Query::parse(text).expect(text);
        assert_eq!(found.warnings().len(), words.len(), "{text}");
        for (warning, word) in found.warnings().iter().zip(words) {
            let want = format!("filter: \"{word}\" is read as a word;");
            assert!(warning.starts_with(&want), "{text}: {warning}");
        }
    }
    let args = Query::parse_args(&["or", "x", "-and"]).expect("args");
    assert_eq!(args.warnings().len(), 1);
    let both = Query::and([Query::parse("not").expect("p"), args]);
    assert_eq!(both.warnings().len(), 2);
    let twice = Query::and([Query::parse("or or").expect("p"), both]);
    assert_eq!(twice.warnings().len(), 2);
    assert_eq!(
        Query::parse("or").expect("p").warnings(),
        [
            "filter: \"or\" is read as a word; use a|b for either, --filter -a for not"
        ]
    );
}

#[test]
fn terms_come_as_written_each_with_its_own_query() {
    let terms = Query::parse_terms(r#"  +a|+b  "fix login" -due:<=fri "#)
        .expect("terms");
    let texts: Vec<&str> = terms.iter().map(|(t, _)| *t).collect();
    assert_eq!(texts, ["+a|+b", "\"fix login\"", "-due:<=fri"]);
    assert!(hits(&terms[1].1, "Fix login now"));
    assert!(!hits(&terms[0].1, "X +c"));
    for input in BAD {
        assert!(Query::parse_terms(input).is_err(), "{input}");
    }
}

#[test]
fn a_term_of_only_projects_or_only_contexts_reads_as_names() {
    use crate::todotxt::Kind;
    let names = |text: &str, kind| {
        let query = parse(text);
        query.names_of(kind).map(|(negated, names)| {
            let names: Vec<String> = names
                .iter()
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .collect();
            (negated, names.join(" "))
        })
    };
    assert_eq!(names("+a", Kind::Project), Some((false, "a".into())));
    assert_eq!(names("-@a|@b", Kind::Context), Some((true, "a b".into())));
    assert_eq!(names("+a", Kind::Context), None);
    let mixed = ["+a|@b", "+a|milk", "+", "+a +b", "milk", "is:done", ""];
    for text in mixed {
        assert_eq!(names(text, Kind::Project), None, "{text}");
    }
}

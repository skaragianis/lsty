use super::split_count;
use crate::run_tests::Fake;

fn words(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

/// The arguments, the arguments left and the count taken from them.
type Split = (&'static str, &'static str, Option<i64>);

const COUNTS: &[Split] = &[
    ("", "", None),
    ("1", "1", None),
    ("1 2", "1", Some(2)),
    ("1 2 3", "1 2", Some(3)),
    ("1, 2", "1, 2", None),
    ("1 x", "1 x", None),
    ("1 +5", "1 +5", None),
    ("1 -5", "1 -5", None),
    ("1 99999999999999999999", "1 99999999999999999999", None),
];

#[test]
fn the_last_argument_is_a_count_unless_a_comma_binds_it_to_the_list() {
    for (args, rest, count) in COUNTS {
        let args = words(args);
        let (left, got) = split_count(&args);
        assert_eq!((left, got), (words(rest).as_slice(), *count), "{args:?}");
    }
}

#[test]
fn reorder_with_no_place_misses_an_argument() {
    for args in [&["reorder", "1"][..], &["reorder"]] {
        let fake = Fake::new()
            .file("todo.txt", "a\nb\n")
            .var("TODO_FILE", "todo.txt");
        let ran = fake.human(args);
        assert_eq!(ran.code, 2, "{args:?}");
        assert!(
            ran.err.starts_with("lsty: missing argument\n"),
            "{}",
            ran.err
        );
    }
}

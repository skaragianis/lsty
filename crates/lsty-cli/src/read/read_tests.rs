use super::{Priorities, is_priorities};
use crate::run_tests::Fake;

/// The argument and the letters it stands for.
const SETS: &[(&str, &str)] = &[
    ("A", "A"),
    ("A-C", "ABC"),
    ("AC", "AC"),
    ("A-CF", "ABCF"),
    ("C-A", ""),
    ("ADG", "ADG"),
];

#[test]
fn priority_sets() {
    for (letters, want) in SETS {
        let set = Priorities::parse(letters);
        let got: String = (b'A'..=b'Z')
            .filter(|&b| set.holds(b))
            .map(char::from)
            .collect();
        assert_eq!(&got, want, "{letters}");
    }
}

/// The argument and whether it has the shape of a priorities argument.
const SHAPES: &[(&str, bool)] = &[
    ("A", true),
    ("a", true),
    ("A-C", true),
    ("a-c", true),
    ("AB", true),
    ("A-CF", true),
    ("ab", false),
    ("client", false),
    ("", false),
    ("A-", false),
    ("-A", false),
    ("1", false),
    ("A1", false),
    ("1-A", false),
];

#[test]
fn priorities_shape() {
    for (arg, want) in SHAPES {
        assert_eq!(is_priorities(arg), *want, "{arg:?}");
    }
}

const HIDING: &str = "(A) Call @phone +work now\nBuy milk\n";

/// The options before the command and the lines the list shows.
const HIDDEN: &[(&str, &str)] = &[
    ("ls", "1 (A) Call @phone +work now\n2 Buy milk\n"),
    ("-+ ls", "1 (A) Call @phone now\n2 Buy milk\n"),
    ("-@ ls", "1 (A) Call +work now\n2 Buy milk\n"),
    ("-P ls", "1 Call @phone +work now\n2 Buy milk\n"),
    ("-@+P ls", "1 Call now\n2 Buy milk\n"),
    ("-@ -@ ls", "1 (A) Call @phone +work now\n2 Buy milk\n"),
];

#[test]
fn the_parts_a_listing_hides_are_cut_from_its_lines() {
    for (line, want) in HIDDEN {
        let fake = Fake::new()
            .file("todo.txt", HIDING)
            .var("TODO_FILE", "todo.txt");
        let words: Vec<&str> =
            line.split(' ').chain(["--no-config"]).collect();
        let ran = fake.run(&words);
        assert_eq!((ran.code, ran.err.as_str()), (0, ""), "{line}");
        assert_eq!(ran.out, format!("{want}-- 2 of 2 shown\n"), "{line}");
    }
}

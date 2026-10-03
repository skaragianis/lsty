use super::read::{Priorities, is_priorities};

/// The argument and the letters it stands for.
const SETS: &[(&str, &str)] = &[
    ("A", "A"),
    ("A-C", "ABC"),
    ("AC", "AC"),
    ("A-CF", "ABCF"),
    ("C-A", ""),
];

#[test]
fn priority_sets() {
    for (letters, want) in SETS {
        let set = Priorities::parse(letters);
        let got: String = ('A'..='Z')
            .filter(|c| set.holds(u8::try_from(*c).unwrap_or(0)))
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
];

#[test]
fn priorities_shape() {
    for (arg, want) in SHAPES {
        assert_eq!(is_priorities(arg), *want, "{arg:?}");
    }
}

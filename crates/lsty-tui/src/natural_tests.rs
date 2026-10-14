use std::cmp::Ordering::{self, Equal, Greater, Less};

use crate::natural::natural;

#[test]
fn names_compare_in_natural_order() {
    const CASES: [(&str, &str, Ordering); 17] = [
        // digits by value, not by text
        ("v2", "v10", Less),
        ("v10", "v2", Greater),
        ("v9", "v9", Equal),
        ("a1b2", "a1b10", Less),
        // a run longer than any integer still compares by value
        ("n99999999999999999999", "n100000000000000000000", Less),
        // leading zeros: same value, then the bytes decide
        ("v02", "v2", Less),
        ("v002", "v10", Less),
        ("v2", "v02", Greater),
        // a name that is only digits
        ("10", "9", Greater),
        ("007", "7", Less),
        ("2", "a", Less),
        // letters without regard to case, then the bytes
        ("Beta", "alpha", Greater),
        ("alpha", "Beta", Less),
        ("abc", "ABC", Greater),
        ("ABD", "abc", Greater),
        // a start before the whole
        ("v", "v1", Less),
        // non-ASCII letters compare by their bytes, after ASCII
        ("é", "z", Greater),
    ];
    for (a, b, want) in CASES {
        let got = natural(a.as_bytes(), b.as_bytes());
        assert_eq!(got, want, "{a:?} {b:?}");
        assert_eq!(natural(b.as_bytes(), a.as_bytes()), want.reverse());
    }
}

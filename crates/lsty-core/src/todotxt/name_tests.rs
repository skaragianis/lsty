use super::{name_has_prefix, name_parent};
use crate::todotxt::{Kind, Line};

#[test]
fn prefix() {
    let rows = [
        ("02.29 equal", "work", true),
        ("02.29 dot child", "work.acme", true),
        ("02.29 slash child", "work/q3", true),
        ("02.30 case", "Work", false),
        ("02.30 longer word", "workshop", false),
        ("02.30 dash", "work-q3", false),
        ("shorter", "wor", false),
        ("flat doubled separator", "work..x", false),
    ];
    for (name, n, want) in rows {
        assert_eq!(name_has_prefix(n.as_bytes(), b"work"), want, "{name}");
    }
}

#[test]
fn parent() {
    let rows = [
        ("top level", "work", ""),
        ("dot child", "work.acme", "work"),
        ("slash child", "work/q3", "work"),
        ("grandchild", "a.b/c", "a.b"),
        ("dash", "home-improvement", ""),
        ("leading dot", ".x", ""),
        ("doubled dot is flat", "a..b", ""),
        ("doubled mixed is flat", "a./b", ""),
        ("doubled anywhere is flat", "a.b/./c", ""),
        ("dash before the dot", "a-.b", "a-"),
    ];
    for (name, n, want) in rows {
        assert_eq!(name_parent(n.as_bytes()), want.as_bytes(), "{name}");
    }
}

#[test]
fn levels_run_from_the_name_up() {
    let levels = |name: &str| -> Vec<String> {
        super::name_levels(name.as_bytes())
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect()
    };
    assert_eq!(levels("a.b/c"), ["a.b/c", "a.b", "a"]);
    assert_eq!(levels("a..b"), ["a..b"]);
    assert_eq!(levels("a./b"), ["a./b"]);
    assert_eq!(levels("a"), ["a"]);
    assert_eq!(levels(""), Vec::<String>::new());
}

// A deep name is walked once: this takes quadratic time otherwise.
#[test]
fn a_deep_name_is_linear() {
    let deep = "a.".repeat(50_000) + "a";
    assert_eq!(super::name_levels(deep.as_bytes()).count(), 50_001);
    assert!(name_has_prefix(deep.as_bytes(), b"a"));
    let flat = "a..".repeat(50_000);
    assert_eq!(super::name_levels(flat.as_bytes()).count(), 1);
    assert!(!name_has_prefix(flat.as_bytes(), b"a"));
}

/// Every word is one row: the names it holds, each as its sigil and the
/// levels from the name up, joined by `>`; the rows of a word are spaced.
#[test]
fn names_of_a_word() {
    let rows = [
        ("+c++", "+c++"),
        ("@c++", "@c++"),
        ("+c#", "+c#"),
        ("+a-", "+a-"),
        ("+a_", "+a_"),
        ("+work.", "+work"),
        ("+work,", "+work"),
        ("+work).", "+work"),
        ("@a.b.", "@a.b>a"),
        ("+.", ""),
        ("+!", ""),
        ("@..", ""),
        ("@feat@s", "@feat @s"),
        ("+work@home", "+work @home"),
        ("+a+b", "+a +b"),
        ("+a.+b,", "+a +b"),
        ("+a.b@c.d", "+a.b>a @c.d>c"),
        ("+a@é", "+a @é"),
        ("me@example.com", ""),
        ("a+b", ""),
        ("a..b", ""),
        ("+a..b", "+a..b"),
        ("@a..b", "@a..b"),
        ("+a.b..c", "+a.b..c"),
    ];
    for (word, want) in rows {
        let raw = format!("x {word}");
        let line = Line::parse(raw.as_bytes());
        let got: Vec<String> = line
            .tokens()
            .filter(|t| matches!(t.kind, Kind::Project | Kind::Context))
            .map(|t| {
                let sigil = if t.kind == Kind::Project { "+" } else { "@" };
                let levels: Vec<_> = super::name_levels(t.value)
                    .map(String::from_utf8_lossy)
                    .collect();
                format!("{sigil}{}", levels.join(">"))
            })
            .collect();
        assert_eq!(got.join(" "), want, "{word}");
        assert_eq!(line.raw(), raw.as_bytes(), "{word}");
    }
}

#[test]
fn a_leading_separator_is_part_of_a_top_level_name() {
    let line = Line::parse(b"x +.a @/b");
    let names: Vec<_> = line.tokens().map(|t| t.value).collect();
    assert_eq!(names, [&b".a"[..], b"/b"]);
    assert_eq!(name_parent(b".a"), b"");
}

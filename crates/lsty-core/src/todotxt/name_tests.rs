use super::{name_has_prefix, name_parent, name_segments};

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
    ];
    for (name, n, want) in rows {
        assert_eq!(name_has_prefix(n.as_bytes(), b"work"), want, "{name}");
    }
}

/// Name, line, its name segments.
type SegmentRow = (&'static str, &'static str, &'static [&'static str]);

#[test]
fn segments() {
    let rows: [SegmentRow; 3] = [
        ("02.31 three", "work.acme/q3", &["work", "acme", "q3"]),
        ("02.31 one", "home-improvement", &["home-improvement"]),
        ("empty segment", "work..acme", &["work", "", "acme"]),
    ];
    for (name, n, want) in rows {
        let got: Vec<&[u8]> = name_segments(n.as_bytes());
        let want: Vec<&[u8]> = want.iter().map(|s| s.as_bytes()).collect();
        assert_eq!(got, want, "{name}");
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
    ];
    for (name, n, want) in rows {
        assert_eq!(name_parent(n.as_bytes()), want.as_bytes(), "{name}");
    }
}

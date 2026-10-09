use super::{name_has_prefix, name_parent};

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

use super::Owner;

const NAMES: [(&str, bool); 8] = [
    ("agent1", true),
    ("a-b_c.d", true),
    ("", false),
    ("a b", false),
    ("a\tb", false),
    ("a:b", false),
    (":", false),
    ("a\u{a0}b", false),
];

#[test]
fn owner_names() {
    for (name, ok) in NAMES {
        assert_eq!(Owner::new(name).is_some(), ok, "{name:?}");
    }
}

#[test]
fn owner_keeps_its_name() {
    assert_eq!(
        Owner::new("agent1").map(|o| o.as_str().to_owned()),
        Some("agent1".to_owned())
    );
}

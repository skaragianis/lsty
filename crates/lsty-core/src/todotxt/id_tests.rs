use super::Id;

#[test]
fn valid_chosen_ids() {
    let long = "a".repeat(65);
    let rows: [(&str, &str, bool); 14] = [
        ("02.17 word", "parser", true),
        ("02.17 short", "k3f9", true),
        ("02.17 one char", "a", true),
        ("02.17 punctuation", "v1.2_x-y", true),
        ("02.17 64 chars", &long[..64], true),
        ("02.18 empty", "", false),
        ("02.18 dash start", "-a", false),
        ("02.18 dot start", ".a", false),
        ("02.18 under start", "_a", false),
        ("02.18 comma", "a,b", false),
        ("02.18 pipe", "a|b", false),
        ("02.18 space", "a b", false),
        ("02.18 non ASCII", "é", false),
        ("02.18 65 chars", &long, false),
    ];
    for (name, id, want) in rows {
        assert_eq!(Id::new(id).is_some(), want, "{name}");
        assert_eq!(Id::is_valid(id.as_bytes()), want, "{name} is_valid");
    }
}

#[test]
fn id_keeps_its_text() {
    let id = Id::new("v1.2_x-y");
    assert_eq!(id.as_ref().map(Id::as_str), Some("v1.2_x-y"));
    assert_eq!(id.map(|i| i.to_string()).as_deref(), Some("v1.2_x-y"));
}

#[test]
fn every_short_generated_id_is_a_valid_chosen_id() {
    for a in super::ID_ALPHABET {
        for b in super::ID_ALPHABET {
            let id = [*a, *b, *a, *b];
            assert!(Id::new(id).is_some());
        }
    }
}

#[test]
fn is_valid_takes_bytes_that_are_not_text() {
    assert!(!Id::is_valid(b"a\xff"));
    assert!(!Id::is_valid(b"\xffa"));
    assert!(Id::is_valid(b"a-b_c.d"));
}

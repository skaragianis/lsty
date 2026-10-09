use super::quote::quote;

/// Text, quoted.
const ROWS: &[(&str, &str)] = &[
    ("plain", "\"plain\""),
    ("a b", "\"a b\""),
    ("a\"b\\c", "\"a\\\"b\\\\c\""),
    ("\u{7}\u{8}\u{c}\n\r\t\u{b}", "\"\\a\\b\\f\\n\\r\\t\\v\""),
    ("\u{1}\u{7f}", "\"\\x01\\x7f\""),
    ("\u{80}\u{9f}", "\"\\u0080\\u009f\""),
    ("é日本😀", "\"é日本😀\""),
    // No-break space and other separators.
    (
        "\u{a0}\u{2003}\u{3000}\u{2028}",
        "\"\\u00a0\\u2003\\u3000\\u2028\"",
    ),
    // Soft hyphen, zero-width space and joiner, marks, BOM, bidi controls.
    ("\u{ad}", "\"\\u00ad\""),
    ("\u{200b}\u{200d}\u{200e}", "\"\\u200b\\u200d\\u200e\""),
    ("\u{feff}\u{2066}\u{202e}", "\"\\ufeff\\u2066\\u202e\""),
    // Private use: the three planes.
    ("\u{e000}\u{f8ff}", "\"\\ue000\\uf8ff\""),
    ("\u{f0000}\u{10fffd}", "\"\\U000f0000\\U0010fffd\""),
    // Unassigned and noncharacters.
    ("\u{378}\u{fdd0}\u{ffff}", "\"\\u0378\\ufdd0\\uffff\""),
    ("\u{e0000}\u{10ffff}", "\"\\U000e0000\\U0010ffff\""),
    // Tag characters.
    ("\u{e0001}", "\"\\U000e0001\""),
    // Printable neighbours of the escaped ones.
    ("\u{a1}\u{ac}\u{ae}", "\"¡¬®\""),
];

#[test]
fn quotes_as_a_quoted_string_does() {
    for &(text, want) in ROWS {
        assert_eq!(quote(text), want, "{text:?}");
    }
}

#[test]
fn the_table_is_sorted_and_apart() {
    use super::quote_table::NOT_PRINTABLE;
    for pair in NOT_PRINTABLE.windows(2) {
        let [(_, last), (first, _)] = pair else {
            continue;
        };
        assert!(last + 1 < *first, "{last:x} {first:x}");
    }
    assert!(NOT_PRINTABLE.iter().all(|(first, last)| first <= last));
}

#[test]
fn invalid_bytes_are_escaped_one_at_a_time() {
    let rows: &[(&[u8], &str)] = &[
        (b"Caf\xe9 x", "\"Caf\\xe9 x\""),
        (b"\xe2\x82", "\"\\xe2\\x82\""),
        (b"\xff\xfe\"", "\"\\xff\\xfe\\\"\""),
    ];
    for &(bytes, want) in rows {
        assert_eq!(quote(bytes), want, "{bytes:?}");
    }
}

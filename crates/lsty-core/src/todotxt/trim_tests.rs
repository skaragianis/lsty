use super::{trim, trim_range};

/// Bytes, and the range left after trimming white space.
type Row = (&'static [u8], (usize, usize));

const ROWS: [Row; 12] = [
    (b"", (0, 0)),
    (b"   ", (3, 3)),
    (b"  a b  ", (2, 5)),
    (b"a", (0, 1)),
    (b"\xc2\xa0a\xc2\xa0", (2, 3)),
    (b"\xe9 ", (0, 1)),
    (b" \xe9", (1, 2)),
    (b"\t\xe2\x80\x83x", (4, 5)),
    (b" \xffa\xff ", (1, 4)),
    (b"\xff  ", (0, 1)),
    (b"  \xff", (2, 3)),
    (b"\n a \r\n", (2, 3)),
];

#[test]
fn trimming_follows_unicode_white_space() {
    for (bytes, (start, end)) in ROWS {
        assert_eq!(trim_range(bytes), start..end, "{bytes:?}");
        assert_eq!(trim(bytes), &bytes[start..end], "{bytes:?}");
    }
}

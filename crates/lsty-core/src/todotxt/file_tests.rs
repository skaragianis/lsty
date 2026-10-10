use super::{File, Line};
use crate::errs::Nr;

/// Raw text and ending of every line, with the byte order mark.
fn summary(f: &File) -> (Vec<String>, Vec<&'static str>, bool) {
    let raws = f
        .lines
        .iter()
        .map(|l| String::from_utf8_lossy(l.raw()).into_owned())
        .collect();
    let eols = f
        .lines
        .iter()
        .map(|l| match l.eol.as_bytes() {
            b"\n" => "\n",
            b"\r\n" => "\r\n",
            _ => "",
        })
        .collect();
    (raws, eols, f.bom)
}

type Row = (
    &'static str,
    &'static [u8],
    &'static [&'static str],
    &'static [&'static str],
    bool,
);

const PARSE_ROWS: &[Row] = &[
    (
        "01.18 two lines with trailing newline",
        b"a\nb\n",
        &["a", "b"],
        &["\n", "\n"],
        false,
    ),
    (
        "01.19 CRLF and no trailing newline",
        b"a\r\nb",
        &["a", "b"],
        &["\r\n", ""],
        false,
    ),
    (
        "01.20 blank lines count",
        b"a\n\n\nb\n",
        &["a", "", "", "b"],
        &["\n", "\n", "\n", "\n"],
        false,
    ),
    ("01.21 empty file", b"", &[], &[], false),
    ("01.21 single newline", b"\n", &[""], &["\n"], false),
    (
        "01.22 byte order mark",
        b"\xef\xbb\xbf(A) Buy\n",
        &["(A) Buy"],
        &["\n"],
        true,
    ),
    (
        "01.23 invalid UTF-8",
        b"Buy \xff\xfe milk\n",
        &["Buy \u{fffd}\u{fffd} milk"],
        &["\n"],
        false,
    ),
    (
        "lone CR stays in the line",
        b"a\rb\n",
        &["a\rb"],
        &["\n"],
        false,
    ),
    (
        "mixed line endings",
        b"a\r\nb\nc",
        &["a", "b", "c"],
        &["\r\n", "\n", ""],
        false,
    ),
    ("no newline at all", b"a", &["a"], &[""], false),
    ("byte order mark only", b"\xef\xbb\xbf", &[], &[], true),
    (
        "CR before the last line without LF",
        b"a\r",
        &["a\r"],
        &[""],
        false,
    ),
    (
        "two byte order marks",
        b"\xef\xbb\xbf\xef\xbb\xbfa",
        &["\u{feff}a"],
        &[""],
        true,
    ),
];

#[test]
fn parse_rows() {
    for (name, data, raws, eols, bom) in PARSE_ROWS {
        let file = File::parse(data);
        let (got_raws, got_eols, got_bom) = summary(&file);
        assert_eq!(&got_raws, raws, "{name}");
        assert_eq!(&got_eols, eols, "{name}");
        assert_eq!(got_bom, *bom, "{name}");
        assert_eq!(file.to_bytes(), *data, "{name}: round trip");
    }
}

#[test]
fn nr_counts_blank_lines() {
    let file = File::parse(b"a\n\n\nb\n");
    assert_eq!(file.lines.len(), 4);
    assert!(file.lines[1].blank && file.lines[2].blank);
    let nr = Nr::new(4).unwrap();
    assert_eq!(file.line(nr).map(Line::raw), Some(&b"b"[..]));
    assert_eq!(file.line(Nr::new(5).unwrap()), None);
}

#[test]
fn bom_is_not_part_of_the_first_line() {
    let file = File::parse(b"\xef\xbb\xbf(A) Buy\n");
    let line = &file.lines[0];
    assert_eq!(line.priority().map(super::Priority::letter), Some(b'A'));
    assert_eq!(line.description(), b"Buy");
}

#[test]
fn to_bytes_writes_raw_not_the_fields() {
    let mut file = File::parse(b"(A)  Buy\n");
    assert!(file.lines[0].priority().is_some());
    file.bom = true;
    assert_eq!(file.to_bytes(), b"\xef\xbb\xbf(A)  Buy\n");
    assert_eq!(File::default().to_bytes(), b"");
}

#[test]
fn round_trip_of_pseudo_random_bytes() {
    // 01.30: any input comes back unchanged and no line panics.
    let alphabet = b"x ()AB-0129:+@\t\r\n\xef\xbb\xbf\xff";
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        seed
    };
    for _ in 0..2000 {
        let len = usize::try_from(next() >> 59).unwrap() * 3;
        let data: Vec<u8> = (0..len)
            .map(|_| {
                alphabet
                    [usize::try_from(next() >> 33).unwrap() % alphabet.len()]
            })
            .collect();
        let file = File::parse(&data);
        assert_eq!(file.to_bytes(), data, "{data:?}");
        drop(Line::parse(data));
    }
}

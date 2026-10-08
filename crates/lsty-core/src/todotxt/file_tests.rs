use super::{Eol, File, Line};
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
        .map(|l| match l.eol().as_bytes() {
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
    assert!(file.lines[1].is_blank() && file.lines[2].is_blank());
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

/// Name, file before, line appended, file after.
type AppendRow<'a> = (&'a str, &'a [u8], &'a str, &'a [u8]);

/// Name, file before, line number blanked, file after.
type BlankRow<'a> = (&'a str, &'a [u8], u32, &'a [u8]);

#[test]
fn append_line() {
    let rows: &[AppendRow<'_>] = &[
        (
            "01.28 no trailing newline and crlf kept",
            b"a\r\nb",
            "c",
            b"a\r\nb\r\nc",
        ),
        ("trailing newline kept", b"a\nb\n", "c", b"a\nb\nc\n"),
        ("crlf with trailing newline", b"a\r\n", "b", b"a\r\nb\r\n"),
        (
            "first line's ending wins",
            b"a\r\nb\n",
            "c",
            b"a\r\nb\nc\r\n",
        ),
        ("empty file", b"", "a", b"a\n"),
        ("single line without newline", b"a", "b", b"a\nb"),
        ("after a blank last line", b"a\n\n", "b", b"a\n\nb\n"),
        ("BOM kept", b"\xef\xbb\xbfa\n", "b", b"\xef\xbb\xbfa\nb\n"),
        (
            "new line is parsed",
            b"a\n",
            "(A) 2026-09-20 Call +work",
            b"a\n(A) 2026-09-20 Call +work\n",
        ),
    ];
    for (name, data, raw, want) in rows {
        let original = File::parse(data);
        let mut file = original.clone();
        file.append_line(*raw);
        assert_eq!(file.to_bytes(), *want, "{name}");
        assert_eq!(file, File::parse(want), "{name}: lines");
        assert_eq!(original.to_bytes(), *data, "{name}: original");
    }
}

#[test]
fn blank_line() {
    let rows: &[BlankRow<'_>] = &[
        (
            "01.29 first line keeps crlf",
            b"a\r\nb\r\n",
            1,
            b"\r\nb\r\n",
        ),
        ("middle line", b"a\nb\nc\n", 2, b"a\n\nc\n"),
        ("last line with newline", b"a\nb\n", 2, b"a\n\n"),
        ("last line without newline", b"a\nb", 2, b"a\n"),
        ("already blank", b"a\n\nc\n", 2, b"a\n\nc\n"),
        ("whitespace-only line", b"a\n  \t\nc\n", 2, b"a\n\nc\n"),
        ("BOM kept", b"\xef\xbb\xbfa\nb\n", 1, b"\xef\xbb\xbf\nb\n"),
    ];
    for (name, data, nr, want) in rows {
        let nr = Nr::new(*nr).unwrap();
        let mut file = File::parse(data);
        let before = file.lines.len();
        assert!(file.blank_line(nr), "{name}");
        assert_eq!(file.to_bytes(), *want, "{name}");
        assert_eq!(file.lines.len(), before, "{name}: count");
        assert!(file.line(nr).is_some_and(Line::is_blank), "{name}");
    }
    assert!(!File::parse(b"a\n").blank_line(Nr::new(2).unwrap()));
    assert!(!File::parse(b"a\n").blank_line(Nr::new(9).unwrap()));
}

#[test]
fn round_trip_of_pseudo_random_bytes() {
    // 01.30: any input comes back unchanged and no line panics.
    let alphabet = b"x ()AB-0129:+@\t\r\n\xef\xbb\xbf\xff";
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    for _ in 0..2000 {
        let mut data = Vec::new();
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let len = usize::try_from(seed >> 59).unwrap() * 3;
        for _ in 0..len {
            seed =
                seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let at = usize::try_from(seed >> 33).unwrap() % alphabet.len();
            data.push(alphabet[at]);
        }
        let file = File::parse(&data);
        assert_eq!(file.to_bytes(), data, "{data:?}");
        drop(Line::parse(data));
    }
}

/// A line of `raw` ending in `eol`.
fn ended(raw: &[u8], eol: Eol) -> Line {
    let mut line = Line::parse(raw);
    line.set_eol(eol);
    line
}

#[test]
fn same_bytes_agrees_with_the_written_bytes() {
    let alphabet = b"ab \r\n\xef\xbb\xbf";
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |below: usize| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        usize::try_from(seed >> 33).unwrap() % below
    };
    for _ in 0..4000 {
        let a: Vec<u8> = (0..next(10)).map(|_| alphabet[next(7)]).collect();
        let mut b = a.clone();
        match next(4) {
            0 => {}
            1 => b.push(alphabet[next(7)]),
            2 if !b.is_empty() => b[next(a.len())] = alphabet[next(7)],
            _ => b = (0..next(10)).map(|_| alphabet[next(7)]).collect(),
        }
        let (fa, fb) = (File::parse(&a), File::parse(&b));
        assert_eq!(fa.same_bytes(&fb), a == b, "{a:?} {b:?}");
        assert_eq!(fb.same_bytes(&fa), a == b, "{a:?} {b:?}");
    }
}

#[test]
fn same_bytes_looks_at_the_bytes_not_at_how_lines_are_cut() {
    let bom = File {
        lines: vec![ended(b"a", Eol::Lf)],
        bom: true,
    };
    let in_text = File {
        lines: vec![ended(b"\xef\xbb\xbfa", Eol::Lf)],
        bom: false,
    };
    assert!(bom.same_bytes(&in_text));
    let one = File {
        lines: vec![ended(b"ab", Eol::None)],
        bom: false,
    };
    let two = File {
        lines: vec![ended(b"a", Eol::None), ended(b"b", Eol::None)],
        bom: false,
    };
    assert!(one.same_bytes(&two));
    let blanks = File {
        lines: vec![ended(b"", Eol::None), ended(b"ab", Eol::None)],
        bom: false,
    };
    assert!(blanks.same_bytes(&one));
    assert!(File::default().same_bytes(&File::parse(b"")));
    assert!(!File::default().same_bytes(&File::parse(b"\n")));
    assert!(!one.same_bytes(&File::parse(b"ab\n")));
    assert!(!File::parse(b"ab\n").same_bytes(&File::parse(b"ab\r\n")));
}

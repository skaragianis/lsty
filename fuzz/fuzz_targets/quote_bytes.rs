//! Any bytes quote to text between double quotes with nothing unprintable
//! left raw, and reading the escapes back gives the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::errs::quote_bytes;

fuzz_target!(|data: &[u8]| {
    let quoted = quote_bytes(data);
    let inner = quoted.strip_prefix('"').and_then(|q| q.strip_suffix('"'));
    let Some(inner) = inner else {
        panic!("not in double quotes: {quoted}");
    };
    assert_eq!(unquote(inner), data);
});

/// The bytes that the escapes of `inner` stand for.
fn unquote(inner: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            assert!(c != '"' && c != '\\', "raw {c:?}");
            assert!(c == ' ' || printable(c), "raw {c:?}");
            let mut buf = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        let Some(e) = chars.next() else {
            panic!("a lone backslash at the end");
        };
        match e {
            '"' | '\\' => out.push(e as u8),
            'a' => out.push(7),
            'b' => out.push(8),
            'f' => out.push(12),
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            'v' => out.push(11),
            'x' => out.push(hex(&mut chars, 2) as u8),
            'u' | 'U' => {
                let n = hex(&mut chars, if e == 'u' { 4 } else { 8 });
                let Some(c) = char::from_u32(n) else {
                    panic!("no character U+{n:X}");
                };
                let mut buf = [0; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            other => panic!("unknown escape \\{other}"),
        }
    }
    out
}

/// The next `digits` lower-case hex digits as a number.
fn hex(chars: &mut std::str::Chars<'_>, digits: usize) -> u32 {
    (0..digits).fold(0, |n, _| {
        let d = chars
            .next()
            .filter(|c| !c.is_ascii_uppercase())
            .and_then(|c| c.to_digit(16));
        let Some(d) = d else {
            panic!("a short hex escape");
        };
        n * 16 + d
    })
}

/// Whether `c` may stand as itself, roughly: not a control character,
/// not a space other than U+0020 and not one of the common invisible
/// format characters.
fn printable(c: char) -> bool {
    !c.is_control()
        && !(c.is_whitespace() && c != ' ')
        && !matches!(
            c,
            '\u{ad}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2060}'..='\u{2064}'
                | '\u{feff}'
        )
}

use std::fmt::Write as _;

use super::quote_table::NOT_PRINTABLE;

/// Whether `c` is printed as itself inside a quoted string.
fn is_printable(c: char) -> bool {
    let n = u32::from(c);
    let at = NOT_PRINTABLE.partition_point(|&(_, last)| last < n);
    NOT_PRINTABLE.get(at).is_none_or(|&(first, _)| n < first)
}

/// `text` in double quotes, with C-style escapes for quotes, backslashes,
/// control characters and every character that is not printable. Each byte
/// that is not part of valid UTF-8 becomes `\xNN`, one byte at a time.
pub fn quote(text: impl AsRef<[u8]>) -> String {
    let mut out = String::from("\"");
    for chunk in text.as_ref().utf8_chunks() {
        chunk.valid().chars().for_each(|c| push_char(&mut out, c));
        for b in chunk.invalid() {
            write!(out, "\\x{b:02x}").ok();
        }
    }
    out.push('"');
    out
}

/// A config key as a message prints it: escaped like a quoted string,
/// without the quotes.
pub fn shown(key: &str) -> String {
    let quoted = quote(key);
    quoted[1..quoted.len() - 1].to_owned()
}

fn push_char(out: &mut String, c: char) {
    match c {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\u{7}' => out.push_str("\\a"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\u{b}' => out.push_str("\\v"),
        c if is_printable(c) => out.push(c),
        c => escape(out, c),
    }
}

fn escape(out: &mut String, c: char) {
    let n = u32::from(c);
    match n {
        ..0x80 => write!(out, "\\x{n:02x}"),
        0x80..0x1_0000 => write!(out, "\\u{n:04x}"),
        _ => write!(out, "\\U{n:08x}"),
    }
    .ok();
}

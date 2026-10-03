use super::quote_table::NOT_PRINTABLE;

/// Whether `c` is printed as itself inside a quoted string.
fn is_printable(c: char) -> bool {
    let n = u32::from(c);
    let at = NOT_PRINTABLE.partition_point(|&(_, last)| last < n);
    match NOT_PRINTABLE.get(at) {
        Some(&(first, _)) => n < first,
        None => true,
    }
}

/// `text` in double quotes, with C-style escapes for quotes, backslashes,
/// control characters and every character that is not printable.
pub fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
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
            c if c == ' ' || is_printable(c) => out.push(c),
            c => out.push_str(&escape(c)),
        }
    }
    out.push('"');
    out
}

fn escape(c: char) -> String {
    let n = u32::from(c);
    if n < 0x80 {
        format!("\\x{n:02x}")
    } else if n < 0x1_0000 {
        format!("\\u{n:04x}")
    } else {
        format!("\\U{n:08x}")
    }
}

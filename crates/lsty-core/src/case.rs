//! Upper and lower case one character to one character, the way the
//! keys, the query words and the completion of names compare text.

/// The simple lower case of `c`: its lower case when that is one
/// character, and `c` when there is none. `\u{130}` (capital dotted I)
/// has a lower case of two characters but a simple one that is a plain
/// `i`.
pub(crate) fn simple_lower(c: char) -> char {
    if c == '\u{130}' {
        return 'i';
    }
    let mut low = c.to_lowercase();
    match (low.next(), low.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// The simple upper case of `c`: its upper case when that is one
/// character, and `c` when there is none. The letters with an iota
/// subscript have an upper case of two characters but a simple one that
/// is their title-case letter.
pub fn simple_upper(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => title_case(c),
    }
}

/// `s` lower-cased one character at a time.
pub fn lower(s: &str) -> String {
    s.chars().map(simple_lower).collect()
}

fn title_case(c: char) -> char {
    let simple = match c {
        '\u{1F80}'..='\u{1F87}'
        | '\u{1F90}'..='\u{1F97}'
        | '\u{1FA0}'..='\u{1FA7}' => 8,
        '\u{1FB3}' | '\u{1FC3}' | '\u{1FF3}' => 9,
        _ => return c,
    };
    char::from_u32(u32::from(c) + simple).unwrap_or(c)
}

#[cfg(test)]
mod tests;

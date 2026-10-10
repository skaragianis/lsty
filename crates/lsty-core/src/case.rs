//! Upper and lower case one character to one character, as text is
//! compared everywhere.

/// The simple lower case of `c`: the first character of its lower case,
/// which is its only one except for `\u{130}` (capital dotted I), whose
/// simple lower case is a plain `i`.
pub(crate) fn simple_lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The simple upper case of `c`: its upper case when that is one
/// character, and `c` when there is none. The letters with an iota
/// subscript have an upper case of two characters but a simple one that
/// is their title-case letter (`iota_subscript_upper`).
pub fn simple_upper(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => iota_subscript_upper(c),
    }
}

/// `s` lower-cased one character at a time.
pub fn lower(s: &str) -> String {
    s.chars().map(simple_lower).collect()
}

/// The title-case letter of an iota-subscript letter: it sits 8 code
/// points above (9 for the three single ones); `c` otherwise.
fn iota_subscript_upper(c: char) -> char {
    let offset = match c {
        '\u{1F80}'..='\u{1F87}'
        | '\u{1F90}'..='\u{1F97}'
        | '\u{1FA0}'..='\u{1FA7}' => 8,
        '\u{1FB3}' | '\u{1FC3}' | '\u{1FF3}' => 9,
        _ => return c,
    };
    char::from_u32(u32::from(c) + offset).unwrap_or(c)
}

#[cfg(test)]
mod tests;

//! Text that is safe to show on a terminal: every control character is
//! replaced by a one-cell picture, so that nothing in a file, an
//! argument or the environment can move the cursor, set the title or
//! recolour the screen.

use std::borrow::Cow;

/// The picture of a control character that the terminal would act on:
/// U+2400 plus its code for C0 (U+2421 for DEL), U+FFFD for C1.
fn picture(c: char) -> char {
    let code = u32::from(c);
    let shown = match code {
        0..=0x1f => char::from_u32(0x2400 + code),
        0x7f => Some('\u{2421}'),
        _ => None,
    };
    shown.unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// `c`, or its picture when it is a control character.
fn shown(c: char) -> char {
    if c.is_control() { picture(c) } else { c }
}

/// `text` with every control character drawn as a one-cell picture, so
/// that widths agree with drawing. Text without control characters is
/// returned as it is.
pub fn visible(text: Cow<'_, str>) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return text;
    }
    Cow::Owned(text.chars().map(shown).collect())
}

/// Like [`visible`] for bytes, each byte that is not UTF-8 drawn as
/// U+FFFD. Bytes that need nothing are returned as they are, after one
/// scan. A line break is drawn too: text of several lines is drawn line
/// by line.
pub fn visible_bytes(text: &[u8]) -> Cow<'_, [u8]> {
    // `fold`, with no branch in the loop, so that the compiler runs a
    // block of bytes at a time
    let c0 = text
        .iter()
        .fold(0, |c0, &b| c0 | (u8::from(b < 0x20) | u8::from(b == 0x7f)));
    // `C2 80` to `C2 9F` is a C1 character
    let c1 =
        |t: &[u8]| t.windows(2).any(|w| w[0] == 0xc2 && w[1] & 0xe0 == 0x80);
    if c0 == 0
        && (text.is_ascii() || str::from_utf8(text).is_ok() && !c1(text))
    {
        return Cow::Borrowed(text);
    }
    let drawn: String =
        String::from_utf8_lossy(text).chars().map(shown).collect();
    Cow::Owned(drawn.into_bytes())
}

#[cfg(test)]
mod tests;

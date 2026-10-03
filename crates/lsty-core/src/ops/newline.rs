//! The text of a new item before it becomes a line.

use crate::errs::{Code, Error, Field, Result};

/// The message of an add without text.
pub(super) const NEEDS_TEXT: &str = "add needs text";

/// The text without surrounding whitespace. Empty text, a line break and
/// a leading done mark are refused.
pub(super) fn clean_text(raw: &str) -> Result<&str> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(Error::new(Code::Usage, NEEDS_TEXT));
    }
    if text.contains(['\r', '\n']) {
        return Err(invalid_text(
            "a task is one line",
            "use lsty addm for several tasks",
        ));
    }
    if text.starts_with("x ") {
        return Err(invalid_text(
            "a new task can't start as done",
            "add it, then use lsty do",
        ));
    }
    Ok(text)
}

/// An `invalid_value` error about the text of a new item. An empty hint
/// is left out.
pub(super) fn invalid_text(message: &str, hint: &str) -> Error {
    let err = Error::new(Code::InvalidValue, message)
        .with_field("field", Field::Str("text".to_owned()));
    if hint.is_empty() {
        err
    } else {
        err.with_hint(hint)
    }
}

//! The text of a new task before it becomes a line.

use crate::errs::{Code, Error, Field, Result, lossy};

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

/// The refusal of a copy that would start with a stamp-like word.
pub(super) fn copy_error(raw: &[u8]) -> Error {
    invalid_text(
        &format!("can't copy {}", lossy(raw)),
        "the text would start with a stamp-like word; reword it first",
    )
}

/// An `invalid_value` error about the text of a new task.
pub(super) fn invalid_text(message: &str, hint: &str) -> Error {
    Error::new(Code::InvalidValue, message)
        .with_field("field", Field::Str("text".to_owned()))
        .with_hint(hint)
}

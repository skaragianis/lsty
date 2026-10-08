use super::line::Priority;

/// The fields [`build`] writes a line from.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Parts<'a> {
    /// Write the completion mark.
    pub done: bool,
    /// Written on a done line too, after the mark.
    pub priority: Option<Priority>,
    /// `YYYY-MM-DD`, written as given.
    pub completed: Option<&'a str>,
    /// `YYYY-MM-DD`, written as given.
    pub created: Option<&'a str>,
    /// Written as it is, after one space that follows the parts above.
    pub description: &'a [u8],
}

/// Writes the line for `parts` in grammar order, `x`, priority,
/// completion date, creation date, description, with one space after each
/// part that is set.
pub(crate) fn build(parts: &Parts<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    if parts.done {
        out.extend_from_slice(b"x ");
    }
    if let Some(p) = parts.priority {
        out.extend_from_slice(&[b'(', p.letter(), b')', b' ']);
    }
    for date in [parts.completed, parts.created].into_iter().flatten() {
        out.extend_from_slice(date.as_bytes());
        out.push(b' ');
    }
    out.extend_from_slice(parts.description);
    out
}

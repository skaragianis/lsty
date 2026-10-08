/// Splits a project or context name at `.` and `/`, keeping empty
/// segments.
#[cfg(test)]
pub(crate) fn name_segments(name: &[u8]) -> Vec<&[u8]> {
    name.split(|b| matches!(b, b'.' | b'/')).collect()
}

/// Whether `name` is `parent` or lies below it.
pub(crate) fn name_has_prefix(name: &[u8], parent: &[u8]) -> bool {
    name.strip_prefix(parent)
        .is_some_and(|rest| matches!(rest.first(), None | Some(b'.' | b'/')))
}

/// The name of the level above `name`, or empty for a top-level name.
pub fn name_parent(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|b| matches!(b, b'.' | b'/')) {
        Some(i) => name.get(..i).unwrap_or_default(),
        None => b"",
    }
}

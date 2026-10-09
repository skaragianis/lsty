/// What separates the parts of a name.
pub const SEPARATORS: [char; 2] = ['.', '/'];

fn is_separator(b: u8) -> bool {
    SEPARATORS.contains(&char::from(b))
}

/// Whether `name` is `parent` or lies below it.
pub(crate) fn name_has_prefix(name: &[u8], parent: &[u8]) -> bool {
    name.strip_prefix(parent)
        .is_some_and(|rest| rest.first().is_none_or(|&b| is_separator(b)))
}

/// The name of the level above `name`, or empty for a top-level name.
pub fn name_parent(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|&b| is_separator(b))
        .and_then(|i| name.get(..i))
        .unwrap_or_default()
}

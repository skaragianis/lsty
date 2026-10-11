/// What separates the parts of a name.
pub const SEPARATORS: [char; 2] = ['.', '/'];

/// `name` up to its last ASCII letter or digit or non-ASCII byte: what a
/// project or context name is once the punctuation that ends a sentence
/// or a list is off it.
pub(crate) fn trim_name(name: &[u8]) -> &[u8] {
    let end = name
        .iter()
        .rposition(|b| !b.is_ascii() || b.is_ascii_alphanumeric())
        .map_or(0, |i| i + 1);
    name.get(..end).unwrap_or_default()
}

fn is_separator(b: u8) -> bool {
    SEPARATORS.contains(&char::from(b))
}

/// `name` and every level above it.
pub fn name_levels(name: &[u8]) -> impl Iterator<Item = &[u8]> {
    std::iter::successors(Some(name), |n| Some(name_parent(n)))
        .take_while(|n| !n.is_empty())
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

/// What separates the parts of a name.
pub const SEPARATORS: [char; 2] = ['.', '/'];

/// `name` without the sentence marks that end it: a project or context
/// name once the punctuation that ends a sentence or a list is off it.
pub(crate) fn trim_name(name: &[u8]) -> &[u8] {
    let end = name
        .iter()
        .rposition(|b| !b".,;:!?)]}'\"/".contains(b))
        .map_or(0, |i| i + 1);
    name.get(..end).unwrap_or_default()
}

fn is_separator(b: u8) -> bool {
    SEPARATORS.contains(&char::from(b))
}

/// Whether `name` has two separators in a row: it is then one flat
/// top-level name.
fn is_flat(name: &[u8]) -> bool {
    name.windows(2).any(|w| w.iter().all(|&b| is_separator(b)))
}

/// `name` up to its last separator, or empty.
fn cut(name: &[u8]) -> &[u8] {
    let at = name.iter().rposition(|&b| is_separator(b)).unwrap_or(0);
    name.get(..at).unwrap_or_default()
}

/// `name` and every level above it.
pub fn name_levels(name: &[u8]) -> impl Iterator<Item = &[u8]> {
    let flat = is_flat(name);
    std::iter::successors(Some(name), move |n| (!flat).then(|| cut(n)))
        .take_while(|n| !n.is_empty())
}

/// Whether `name` is `parent` or lies below it.
pub(crate) fn name_has_prefix(name: &[u8], parent: &[u8]) -> bool {
    if is_flat(name) {
        return name == parent;
    }
    name.strip_prefix(parent)
        .is_some_and(|rest| rest.first().is_none_or(|&b| is_separator(b)))
}

/// The name of the level above `name`, or empty for a top-level name. A
/// name with two separators in a row is one flat top-level name.
pub fn name_parent(name: &[u8]) -> &[u8] {
    if is_flat(name) { &[] } else { cut(name) }
}

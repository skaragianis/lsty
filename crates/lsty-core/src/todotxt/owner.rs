/// The name of an agent: the value of its `owner:` tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner(String);

impl Owner {
    /// The owner called `name`; `None` when it is empty or has a colon or
    /// whitespace, which a tag value can't hold.
    pub(crate) fn new(name: &str) -> Option<Self> {
        let bad = |c: char| c == ':' || c.is_whitespace();
        (!name.is_empty() && !name.contains(bad))
            .then(|| Self(name.to_owned()))
    }

    /// The name.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

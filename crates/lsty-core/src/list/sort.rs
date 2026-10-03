use super::snapshot::{Item, Snapshot};
use crate::errs::{self, Nr, Source};
use crate::todotxt::Priority;

/// What `sort` orders items by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortField {
    /// Line number; `manual` is the same.
    File,
    /// Effective priority A to Z, then items without one.
    Priority,
    /// Earliest `due:` first, then items without a valid one.
    Due,
    /// Oldest creation date first, then items without one.
    Created,
    /// The description without regard to case, then by bytes.
    Alpha,
}

impl SortField {
    /// The values `--sort` accepts, in the order they are listed.
    pub const NAMES: [&'static str; 6] =
        ["manual", "file", "priority", "due", "created", "alpha"];

    /// The field a `--sort` value names, in lower case.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "file" | "manual" => Some(Self::File),
            "priority" => Some(Self::Priority),
            "due" => Some(Self::Due),
            "created" => Some(Self::Created),
            "alpha" => Some(Self::Alpha),
            _ => None,
        }
    }
}

/// The value one item is ordered by. Every key of one sort is of the same
/// variant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key<'a> {
    Nr(Nr),
    Letter(bool, Option<u8>),
    Date(bool, Option<&'a str>),
    Text(Vec<u8>, &'a [u8]),
}

impl Snapshot {
    /// Orders `items`, which come from this snapshot, by `field`. Items of
    /// the todo file stay before those of the done file, and ties keep
    /// their order.
    pub fn sort<'a>(&'a self, items: &mut [&'a Item], field: SortField) {
        items.sort_by_cached_key(|item| {
            let rank = u8::from(item.source() == Source::Done);
            (rank, self.sort_key(item, field))
        });
    }

    fn sort_key<'a>(&'a self, item: &'a Item, field: SortField) -> Key<'a> {
        let line = self.line(item);
        match field {
            SortField::File => Key::Nr(item.nr()),
            SortField::Priority => {
                let letter = line.effective_priority().map(Priority::letter);
                Key::Letter(letter.is_none(), letter)
            }
            SortField::Due => {
                let due = line.tags().due;
                Key::Date(due.is_none(), due)
            }
            SortField::Created => {
                let created = line.created();
                Key::Date(created.is_none(), created)
            }
            SortField::Alpha => {
                let text = line.description();
                Key::Text(fold(text).into_bytes(), text)
            }
        }
    }
}

/// The text in lower case, one character at a time, so that the result
/// does not depend on the neighbours of a letter.
fn fold(text: &[u8]) -> String {
    errs::lossy(text)
        .chars()
        .flat_map(|c| {
            // The simple mapping: one character each (`İ` gives `i`).
            c.to_lowercase().take(1)
        })
        .collect()
}

//! References to items on the command line: `12`, `id:k3f9` and
//! `done:12`, alone or in a comma list.

use super::expect::{self, Expect};
use crate::{
    errs::{Code, Error, Field, HELP_HINT, Nr, Result, Source, quote},
    list::{self, Item, Snapshot},
    todotxt::Id,
};

/// What a reference names.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    /// A line of a file; the number can be zero or past the end. Only a
    /// line can be guarded.
    Line {
        source: Source,
        nr: i64,
        expect: Option<Expect>,
    },
    /// The item that carries an id, in whichever file it is.
    Id(Id),
}

/// One reference as given: a line or an id, and the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    key: Key,
    given: Option<String>,
    /// Whether an item of the done file is accepted.
    pub(super) archived: bool,
}

impl Target {
    /// Line `nr` of the todo file, with no reference as written.
    pub const fn todo(nr: i64) -> Self {
        Self::at(Source::Todo, nr)
    }

    /// The todo line at row `index` (counting from zero).
    pub(super) fn todo_index(index: usize) -> Self {
        Self::todo(i64::try_from(index + 1).unwrap_or(i64::MAX))
    }

    /// The item that carries `id`, in whichever file it is.
    pub(crate) const fn by_id(id: Id) -> Self {
        Self::new(Key::Id(id))
    }

    /// Line `nr` of the file `source`, with no reference as written.
    pub const fn at(source: Source, nr: i64) -> Self {
        Self::new(Key::Line {
            source,
            nr,
            expect: None,
        })
    }

    const fn new(key: Key) -> Self {
        Self {
            key,
            given: None,
            archived: false,
        }
    }

    /// Records the reference as written, which messages use.
    #[must_use]
    pub(super) fn written(mut self, given: &str) -> Self {
        self.given = Some(given.to_owned());
        self
    }

    /// Accepts an item of the done file too.
    #[must_use]
    pub(crate) const fn archived(mut self) -> Self {
        self.archived = true;
        self
    }

    /// Guards the target: the line it names must still have `text`. When
    /// `exact`, the item's text must equal it; otherwise it must be a
    /// fragment of the raw line or of the text. Runs of white space count
    /// as one space either way.
    /// An `id:` reference takes no guard: this leaves it as it is.
    pub fn set_expect(&mut self, text: impl Into<Vec<u8>>, exact: bool) {
        if let Key::Line { expect, .. } = &mut self.key {
            *expect = Some(Expect::new(text, exact));
        }
    }

    /// The guard, when the target has one.
    pub(crate) const fn expect(&self) -> Option<&Expect> {
        match &self.key {
            Key::Line { expect, .. } => expect.as_ref(),
            Key::Id(_) => None,
        }
    }

    /// The target, refusing an item of the done file again.
    #[must_use]
    pub(super) const fn current_only(mut self) -> Self {
        self.archived = false;
        self
    }

    /// The file and the line number of a reference by number; the number
    /// can be zero or past the end of the file. `None` for an `id:`
    /// reference.
    pub const fn line(&self) -> Option<(Source, i64)> {
        match self.key {
            Key::Line { source, nr, .. } => Some((source, nr)),
            Key::Id(_) => None,
        }
    }

    /// The id the target names, when it is an `id:` reference.
    pub const fn id(&self) -> Option<&Id> {
        match &self.key {
            Key::Id(id) => Some(id),
            Key::Line { .. } => None,
        }
    }

    /// The reference as written, none when it was built in code.
    pub fn given(&self) -> Option<&str> {
        self.given.as_deref()
    }

    /// How messages name the target: the reference as written without
    /// `id:`, or its id, or its number, with `done:` before a number in
    /// the done file.
    pub fn handle(&self) -> String {
        if let Some(given) = &self.given {
            return given.strip_prefix("id:").unwrap_or(given).to_owned();
        }
        match &self.key {
            Key::Id(id) => id.to_string(),
            Key::Line { source, nr, .. } => source.numbered(nr),
        }
    }
}

/// Reads a comma list of references: `12`, `id:k3f9` or `done:12`.
pub fn parse_refs(arg: &str) -> Result<Vec<Target>> {
    arg.split(',').map(parse_ref).collect()
}

/// Reads the one reference a command takes; a comma list is a usage
/// error.
pub fn parse_one(command: &str, arg: &str) -> Result<Target> {
    let [only] = <[Target; 1]>::try_from(parse_refs(arg)?).map_err(|_| {
        Error::new(Code::Usage, format!("{command} takes one reference"))
            .with_hint(HELP_HINT)
    })?;
    Ok(only)
}

fn parse_ref(text: &str) -> Result<Target> {
    if let Some(id) = text.strip_prefix("id:").filter(|id| !id.is_empty()) {
        let id = Id::new(id).ok_or_else(|| invalid_id(id))?;
        return Ok(Target::by_id(id).written(text));
    }
    let (source, digits) = match text.strip_prefix("done:") {
        Some(rest) => (Source::Done, rest),
        None => (Source::Todo, text),
    };
    let all_digits = digits.bytes().all(|b| b.is_ascii_digit());
    match digits.parse::<i64>() {
        Ok(nr) if all_digits => Ok(Target::at(source, nr).written(text)),
        _ => Err(Error::new(
            Code::Usage,
            format!("{} is not a reference", quote(text)),
        )
        .with_hint("use 12, id:k3f9 or done:12")),
    }
}

/// The error for an id that lsty may not write.
pub(super) fn invalid_id(id: &str) -> Error {
    let accepted = ["letters, digits, - _ . (up to 64), starting with a \
                     letter or digit"];
    Error::invalid_value(format!("id {id} is not a valid id"), id, &accepted)
        .with_field("field", Field::Str("id".to_owned()))
}

/// The item a target names; a blank line or a line past the end is
/// `no_item`.
pub fn resolve<'s>(snap: &'s Snapshot, target: &Target) -> Result<&'s Item> {
    find(snap, target)?.ok_or_else(|| no_item(target))
}

/// The item a target names: `None` for a blank line, `no_item` for a
/// line that is not in the file.
pub(super) fn find<'s>(
    snap: &'s Snapshot,
    target: &Target,
) -> Result<Option<&'s Item>> {
    let (source, number) = match &target.key {
        Key::Id(id) => return snap.by_id(id.as_str()).map(Some),
        Key::Line { source, nr, .. } => (*source, *nr),
    };
    let nr = u32::try_from(number)
        .ok()
        .and_then(Nr::new)
        .filter(|nr| nr.index() < snap.file(source).lines.len());
    let item = nr.and_then(|nr| snap.at(source, nr));
    expect::check(snap, item, target, (source, number))?;
    nr.map(|_| item).ok_or_else(|| no_item(target))
}

pub(super) fn no_item(target: &Target) -> Error {
    match &target.key {
        Key::Line { nr, .. } => {
            Error::new(Code::NoItem, format!("no item {}", target.handle()))
                .with_field("nr", Field::Int(*nr))
        }
        Key::Id(id) => list::no_item(id.as_str().as_bytes()),
    }
}

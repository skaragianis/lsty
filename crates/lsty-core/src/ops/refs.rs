//! References to items on the command line: `12`, `id:k3f9` and
//! `done:12`, alone or in a comma list.

use crate::errs::{Code, Error, Field, Nr, Result, Source};
use crate::list::{Item, Snapshot};
use crate::todotxt::Id;

/// What a reference names.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    /// A line of a file; the number can be zero or past the end.
    Line { source: Source, nr: i64 },
    /// The item that carries an id, in whichever file it is.
    Id(Id),
}

/// One reference as given: a line or an id, and the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    key: Key,
    given: String,
    archived: bool,
}

impl Target {
    /// Line `nr` of the todo file, with no reference as written.
    pub const fn todo(nr: i64) -> Self {
        Self::at(Source::Todo, nr)
    }

    /// Line `nr` of the done file, with no reference as written.
    pub const fn done(nr: i64) -> Self {
        Self::at(Source::Done, nr)
    }

    /// The item that carries `id`, in whichever file it is.
    pub const fn by_id(id: Id) -> Self {
        Self::new(Key::Id(id))
    }

    const fn at(source: Source, nr: i64) -> Self {
        Self::new(Key::Line { source, nr })
    }

    const fn new(key: Key) -> Self {
        Self {
            key,
            given: String::new(),
            archived: false,
        }
    }

    /// Records the reference as written, which messages use.
    #[must_use]
    fn written(mut self, given: &str) -> Self {
        given.clone_into(&mut self.given);
        self
    }

    /// Accepts an item of the done file too.
    #[must_use]
    pub const fn archived(mut self) -> Self {
        self.archived = true;
        self
    }

    /// The target, refusing an item of the done file again.
    #[must_use]
    pub(super) const fn current_only(mut self) -> Self {
        self.archived = false;
        self
    }

    /// Whether an item of the done file is accepted.
    pub const fn is_archived(&self) -> bool {
        self.archived
    }

    /// The file the reference names; the todo file for an id.
    pub const fn source(&self) -> Source {
        match self.key {
            Key::Line { source, .. } => source,
            Key::Id(_) => Source::Todo,
        }
    }

    /// The line number, which can be zero or past the end of the file;
    /// zero for an id.
    pub const fn nr(&self) -> i64 {
        match self.key {
            Key::Line { nr, .. } => nr,
            Key::Id(_) => 0,
        }
    }

    /// The id the target names, when it is an `id:` reference.
    pub const fn id(&self) -> Option<&Id> {
        match &self.key {
            Key::Id(id) => Some(id),
            Key::Line { .. } => None,
        }
    }

    /// The reference as written, empty when it was built in code.
    pub fn given(&self) -> &str {
        &self.given
    }

    /// How messages name the target: the reference as written without
    /// `id:`, or its id, or its number, with `done:` before a number in
    /// the done file.
    pub fn handle(&self) -> String {
        if !self.given.is_empty() {
            let given = self.given.strip_prefix("id:");
            return given.unwrap_or(&self.given).to_owned();
        }
        match &self.key {
            Key::Id(id) => id.to_string(),
            Key::Line {
                source: Source::Todo,
                nr,
            } => nr.to_string(),
            Key::Line {
                source: Source::Done,
                nr,
            } => format!("done:{nr}"),
        }
    }
}

/// The hint of a usage error that has no synopsis to give.
const HELP_HINT: &str = "run \"lsty help\"";

/// Reads a comma list of references: `12`, `id:k3f9` or `done:12`.
pub fn parse_refs(arg: &str) -> Result<Vec<Target>> {
    arg.split(',').map(parse_ref).collect()
}

/// Reads the one reference a command takes; a comma list is a usage
/// error.
pub fn parse_one(command: &str, arg: &str) -> Result<Target> {
    let mut targets = parse_refs(arg)?;
    match (targets.pop(), targets.is_empty()) {
        (Some(only), true) => Ok(only),
        _ => Err(Error::new(
            Code::Usage,
            format!("{command} takes one reference"),
        )
        .with_hint(HELP_HINT)),
    }
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
            format!("\"{text}\" is not a reference"),
        )
        .with_hint("use 12, id:k3f9 or done:12")),
    }
}

/// The error for an id that lsty may not write.
pub(super) fn invalid_id(id: &str) -> Error {
    Error::new(Code::InvalidValue, format!("id {id} is not a valid id"))
        .with_field("field", Field::Str("id".to_owned()))
        .with_field("value", Field::Str(id.to_owned()))
        .with_field(
            "accepted",
            Field::Strs(vec![
                "letters, digits, - _ . (up to 64), starting with a \
                 letter or digit"
                    .to_owned(),
            ]),
        )
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
    if let Some(id) = target.id() {
        return snap.by_id(id.as_str()).map(Some);
    }
    let source = target.source();
    let nr = u32::try_from(target.nr()).ok().and_then(Nr::new);
    let in_file =
        nr.is_some_and(|nr| nr.index() < snap.file(source).lines.len());
    match nr {
        Some(nr) if in_file => Ok(snap.at(source, nr)),
        _ => Err(no_item(target)),
    }
}

pub(super) fn no_item(target: &Target) -> Error {
    Error::new(Code::NoItem, format!("no item {}", target.handle()))
        .with_field("nr", Field::Int(target.nr()))
}

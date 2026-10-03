//! References to items on the command line: `12` and `done:12`.

use crate::errs::{Code, Error, Field, Nr, Result, Source};
use crate::list::{Item, Snapshot};

/// One reference as given: the file, the line number and the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The file the reference names.
    pub source: Source,
    /// The line number; it can be zero or past the end of the file.
    pub nr: i64,
    /// The reference as written, which messages use.
    pub given: String,
    /// Whether an item of the done file is accepted.
    pub archived: bool,
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

    const fn at(source: Source, nr: i64) -> Self {
        Self {
            source,
            nr,
            given: String::new(),
            archived: false,
        }
    }

    /// Accepts an item of the done file too.
    #[must_use]
    pub const fn archived(mut self) -> Self {
        self.archived = true;
        self
    }

    /// How messages name the target: the reference as written, or its
    /// number, with `done:` before a number in the done file.
    pub fn handle(&self) -> String {
        if !self.given.is_empty() {
            return self.given.clone();
        }
        match self.source {
            Source::Todo => self.nr.to_string(),
            Source::Done => format!("done:{}", self.nr),
        }
    }
}

/// The hint of a usage error that has no synopsis to give.
const HELP_HINT: &str = "run \"lsty help\"";

/// Reads the one reference a command takes; a comma list is a usage
/// error.
pub fn parse_one(command: &str, arg: &str) -> Result<Target> {
    let mut targets = arg
        .split(',')
        .map(parse_ref)
        .collect::<Result<Vec<Target>>>()?;
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
    let (source, digits) = match text.strip_prefix("done:") {
        Some(rest) => (Source::Done, rest),
        None => (Source::Todo, text),
    };
    let all_digits = digits.bytes().all(|b| b.is_ascii_digit());
    match digits.parse::<i64>() {
        Ok(nr) if all_digits => Ok(Target {
            source,
            nr,
            given: text.to_owned(),
            archived: false,
        }),
        _ => Err(Error::new(
            Code::Usage,
            format!("\"{text}\" is not a reference"),
        )
        .with_hint("use 12, id:k3f9 or done:12")),
    }
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
    let nr = u32::try_from(target.nr).ok().and_then(Nr::new);
    let in_file =
        nr.is_some_and(|nr| nr.index() < snap.file(target.source).lines.len());
    match nr {
        Some(nr) if in_file => Ok(snap.at(target.source, nr)),
        _ => Err(no_item(target)),
    }
}

pub(super) fn no_item(target: &Target) -> Error {
    Error::new(Code::NoItem, format!("no item {}", target.handle()))
        .with_field("nr", Field::Int(target.nr))
}

//! A message as data: plain text and the tasks it names.

use super::{Nr, Source};

/// The list a name's line number belongs to: `Written` is the list the
/// write leaves, `Read` the list the operation read under the lock (a task
/// the write removed, a refusal); a line that list does not have (one a
/// refused write would only have added, an edit's task gone) has no place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The line in the list the write left.
    Written(Nr),
    /// The line in the list the operation read: a task the write
    /// removed, and every name in a refusal.
    Read(Nr),
}

/// How a message names a task no row shows: by its `id:`, else by its
/// line, `line 3` for a todo line and `done:3` for the done file's.
pub fn unplaced(source: Source, nr: Nr, id: Option<&str>) -> String {
    id.map_or_else(
        || match source {
            Source::Todo => format!("line {nr}"),
            Source::Done => source.numbered(nr),
        },
        |id| format!("id:{id}"),
    )
}

/// A task a message names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    /// What the command line prints for it: the reference the caller
    /// wrote, or the line as the operation read it.
    pub typed: String,
    /// The task's `id:`, if it has one.
    pub id: Option<String>,
    /// The file the line is in.
    pub source: Source,
    /// The line, if it has one.
    pub place: Option<Place>,
    /// The staged row it is, while the operation runs.
    pub(crate) row: Option<(Source, usize)>,
    /// The line the task had before the write, for a name that says where
    /// the task went.
    pub from: Option<(Source, Place)>,
}

impl Name {
    /// How the command line names the task.
    pub fn cli(&self) -> String {
        self.typed.clone()
    }

    /// A name that is not a row of a staged file.
    pub(crate) fn new(
        typed: String,
        id: Option<String>,
        source: Source,
        place: Option<Place>,
    ) -> Self {
        Self {
            typed,
            id,
            source,
            place,
            row: None,
            from: None,
        }
    }

    /// This name for the line `row` of the list the write leaves, which is
    /// line `nr` of it.
    pub(crate) fn written(mut self, nr: Nr, row: (Source, usize)) -> Self {
        self.place = Some(Place::Written(nr));
        self.row = Some(row);
        self
    }

    /// This name for the line the operation read, even when the task is
    /// still in the list the write leaves: the one a message that says
    /// where it went starts with.
    #[must_use]
    pub(crate) fn read(mut self) -> Self {
        self.row = None;
        self
    }

    /// This name for the task that was `from` before the write.
    #[must_use]
    pub(crate) fn moved_from(mut self, from: &Self) -> Self {
        self.from = from.place.map(|place| (from.source, place));
        self
    }

    /// This name as the command line prints `typed`.
    #[must_use]
    pub fn typed(mut self, typed: impl Into<String>) -> Self {
        self.typed = typed.into();
        self
    }

    /// A message that starts with this name and goes on with `text`.
    pub fn then(self, text: impl AsRef<[u8]>) -> Msg {
        Msg::from(self).then(text)
    }
}

/// A piece of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Text, which may be the text of a task and is never read again.
    Text(Vec<u8>),
    /// A task.
    Name(Name),
}

/// A message: a template of text and the tasks it names, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Msg(Vec<Part>);

impl Msg {
    /// The parts, in order.
    pub fn parts(&self) -> &[Part] {
        &self.0
    }

    /// The parts, to place the names.
    pub(crate) fn parts_mut(&mut self) -> &mut [Part] {
        &mut self.0
    }

    /// This message without its last `n` parts.
    #[must_use]
    pub fn without_last(&self, n: usize) -> Self {
        Self(self.0[..self.0.len().saturating_sub(n)].to_vec())
    }

    /// This message and `text`.
    #[must_use]
    pub fn then(mut self, text: impl AsRef<[u8]>) -> Self {
        self.0.push(Part::Text(text.as_ref().to_vec()));
        self
    }

    /// This message and a task.
    #[must_use]
    pub fn and(mut self, name: Name) -> Self {
        self.0.push(Part::Name(name));
        self
    }

    /// This message and the tasks `names`, set apart by commas.
    #[must_use]
    pub fn and_list(mut self, names: impl IntoIterator<Item = Name>) -> Self {
        for (i, name) in names.into_iter().enumerate() {
            if i > 0 {
                self = self.then(", ");
            }
            self = self.and(name);
        }
        self
    }

    /// The bytes of the message, with `name` writing each task.
    pub fn render(&self, name: impl Fn(&Name) -> String) -> Vec<u8> {
        let mut out = Vec::new();
        for part in &self.0 {
            match part {
                Part::Text(text) => out.extend_from_slice(text),
                Part::Name(n) => out.extend_from_slice(name(n).as_bytes()),
            }
        }
        out
    }

    /// The bytes the command line prints.
    pub fn cli(&self) -> Vec<u8> {
        self.render(Name::cli)
    }
}

impl From<Name> for Msg {
    fn from(name: Name) -> Self {
        Self(vec![Part::Name(name)])
    }
}

impl From<&str> for Msg {
    fn from(text: &str) -> Self {
        Self(vec![Part::Text(text.as_bytes().to_vec())])
    }
}

impl From<String> for Msg {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}

impl From<Vec<u8>> for Msg {
    fn from(text: Vec<u8>) -> Self {
        Self(vec![Part::Text(text)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(typed: &str, place: Place) -> Name {
        Name::new(typed.to_owned(), None, Source::Done, Some(place))
    }

    #[test]
    fn renders_names_by_the_function_and_text_as_it_is() {
        let nr = Nr::new(4).unwrap();
        let said = name("done:4", Place::Read(nr))
            .then(" and ")
            .and(name("k3f9", Place::Written(nr)))
            .then(" \u{1}%n{}");
        assert_eq!(said.cli(), b"done:4 and k3f9 \x01%n{}");
        assert_eq!(said.render(|_| "X".into()), b"X and X \x01%n{}");
    }
}

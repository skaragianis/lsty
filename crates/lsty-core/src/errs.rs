//! The error type and the code table.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::num::NonZeroU32;
use std::path::Path;

mod quote;
mod quote_table;

pub use quote::quote;

#[cfg(test)]
mod quote_tests;

/// Exit code of an I/O or internal error.
pub const EXIT_IO: u8 = 1;
/// Exit code of a wrong call.
pub const EXIT_USAGE: u8 = 2;
/// Exit code when the list disagrees with the call.
pub const EXIT_CONFLICT: u8 = 3;
/// Exit code of a lock timeout.
pub const EXIT_LOCK_TIMEOUT: u8 = 4;

/// A stable error code, written as `error.code` in the JSON envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Code {
    /// The call is malformed.
    Usage,
    /// The command name is not known.
    UnknownCommand,
    /// An option or argument has a value outside the accepted forms.
    InvalidValue,
    /// The command needs a terminal and has none.
    NoTty,
    /// The write needs an identity and has none.
    NoIdentity,
    /// No todo file was found.
    NoFile,
    /// The reference names no item.
    NoItem,
    /// `--expect` does not match the item's text.
    ExpectMismatch,
    /// The item has an open prerequisite.
    Blocked,
    /// The change would create a dependency cycle.
    Cycle,
    /// Another actor is working on the item.
    Claimed,
    /// The item is in a state the command does not accept.
    WrongState,
    /// The id is already used by another item.
    IdTaken,
    /// The id is held by more than one item.
    Ambiguous,
    /// The file changed while it was being edited.
    EditConflict,
    /// The lock was not acquired in time.
    LockTimeout,
    /// An I/O or internal error.
    Io,
}

impl Code {
    /// Every code, in table order.
    pub const ALL: [Self; 17] = [
        Self::Usage,
        Self::UnknownCommand,
        Self::InvalidValue,
        Self::NoTty,
        Self::NoIdentity,
        Self::NoFile,
        Self::NoItem,
        Self::ExpectMismatch,
        Self::Blocked,
        Self::Cycle,
        Self::Claimed,
        Self::WrongState,
        Self::IdTaken,
        Self::Ambiguous,
        Self::EditConflict,
        Self::LockTimeout,
        Self::Io,
    ];

    /// The code as written in output, such as `unknown_command`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::UnknownCommand => "unknown_command",
            Self::InvalidValue => "invalid_value",
            Self::NoTty => "no_tty",
            Self::NoIdentity => "no_identity",
            Self::NoFile => "no_file",
            Self::NoItem => "no_item",
            Self::ExpectMismatch => "expect_mismatch",
            Self::Blocked => "blocked",
            Self::Cycle => "cycle",
            Self::Claimed => "claimed",
            Self::WrongState => "wrong_state",
            Self::IdTaken => "id_taken",
            Self::Ambiguous => "ambiguous",
            Self::EditConflict => "edit_conflict",
            Self::LockTimeout => "lock_timeout",
            Self::Io => "io",
        }
    }

    /// The process exit code for this error.
    pub const fn exit(self) -> u8 {
        match self {
            Self::Usage
            | Self::UnknownCommand
            | Self::InvalidValue
            | Self::NoTty
            | Self::NoIdentity
            | Self::NoFile => EXIT_USAGE,
            Self::NoItem
            | Self::ExpectMismatch
            | Self::Blocked
            | Self::Cycle
            | Self::Claimed
            | Self::WrongState
            | Self::IdTaken
            | Self::Ambiguous
            | Self::EditConflict => EXIT_CONFLICT,
            Self::LockTimeout => EXIT_LOCK_TIMEOUT,
            Self::Io => EXIT_IO,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An item number: the 1-based physical line number in its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nr(NonZeroU32);

impl Nr {
    /// The NR of line `n`, or `None` for 0.
    pub const fn new(n: u32) -> Option<Self> {
        match NonZeroU32::new(n) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// The line number.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for Nr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The file an item lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// The todo file.
    Todo,
    /// The done file.
    Done,
}

impl Source {
    /// The source as written in output: `todo` or `done`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Done => "done",
        }
    }
}

/// A reference to an item, written `{"nr", "source", "id"}` in JSON.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ref {
    /// The item's line number in its file.
    pub nr: Nr,
    /// The file the item lives in.
    pub source: Source,
    /// The item's `id:`, if it has one.
    pub id: Option<String>,
}

/// The value of an extra field of an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// JSON `null`.
    Null,
    /// A number.
    Int(i64),
    /// A string.
    Str(String),
    /// A list of strings.
    Strs(Vec<String>),
    /// A reference to an item.
    Ref(Ref),
    /// A list of references to items.
    Refs(Vec<Ref>),
}

/// An lsty error: a code, a human message and the details of the code.
///
/// The fields live behind one box so that `Result<T, Error>` stays small.
#[derive(Debug)]
pub struct Error(Box<Inner>);

#[derive(Debug)]
struct Inner {
    code: Code,
    message: String,
    hint: Option<String>,
    item: Option<Ref>,
    fields: BTreeMap<&'static str, Field>,
    source: Option<io::Error>,
}

/// A result whose error is an lsty [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// An error with `code` and the human `message`, which has no `lsty:`
    /// prefix and no newline.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self(Box::new(Inner {
            code,
            message: message.into(),
            hint: None,
            item: None,
            fields: BTreeMap::new(),
            source: None,
        }))
    }

    /// Sets the hint printed after the message.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.0.hint = Some(hint.into());
        self
    }

    /// Sets the item the error is about.
    #[must_use]
    pub fn item(mut self, item: Ref) -> Self {
        self.0.item = Some(item);
        self
    }

    /// Sets an extra field of the code's row in the table.
    #[must_use]
    pub fn field(mut self, key: &'static str, value: Field) -> Self {
        self.0.fields.insert(key, value);
        self
    }

    /// The error's code.
    pub fn code(&self) -> Code {
        self.0.code
    }

    /// The process exit code for this error.
    pub fn exit(&self) -> u8 {
        self.0.code.exit()
    }

    /// The human message.
    pub fn message(&self) -> &str {
        &self.0.message
    }

    /// The hint, if there is one.
    pub fn hint_text(&self) -> Option<&str> {
        self.0.hint.as_deref()
    }

    /// The item the error is about, if any.
    pub fn item_ref(&self) -> Option<&Ref> {
        self.0.item.as_ref()
    }

    /// The extra fields, in key order.
    pub fn fields(&self) -> impl Iterator<Item = (&'static str, &Field)> {
        self.0.fields.iter().map(|(key, value)| (*key, value))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0
            .source
            .as_ref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

/// An `io` error for `op` on `path`, worded
/// `open /x/todo.txt: no such file or directory`.
pub fn io(op: &str, path: &Path, err: io::Error) -> Error {
    let message = format!("{op} {}: {}", path.display(), io_text(&err));
    let mut e = Error::new(Code::Io, message);
    e.0.source = Some(err);
    e
}

/// An `io` error with two layers, worded
/// `read /x/todo.txt: open /x/todo.txt: permission denied`: the step
/// `op path` that failed, and the system call `inner_op inner_path` under
/// it.
pub fn io_at(
    op: &str,
    path: &Path,
    inner_op: &str,
    inner_path: &Path,
    err: io::Error,
) -> Error {
    let message = format!(
        "{op} {}: {inner_op} {}: {}",
        path.display(),
        inner_path.display(),
        io_text(&err)
    );
    with_source(message, err)
}

/// An `io` error for a failed rename of `from` to `to`, worded
/// `rename a: rename a b: file exists`.
pub fn io_rename(from: &Path, to: &Path, err: io::Error) -> Error {
    let message = format!(
        "rename {0}: rename {0} {1}: {2}",
        from.display(),
        to.display(),
        io_text(&err)
    );
    with_source(message, err)
}

fn with_source(message: String, err: io::Error) -> Error {
    let mut e = Error::new(Code::Io, message);
    e.0.source = Some(err);
    e
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        let mut e = Self::new(Code::Io, io_text(&err));
        e.0.source = Some(err);
        e
    }
}

/// The text of an I/O error without Rust's ` (os error N)` suffix and
/// and with a lower-case first letter.
fn io_text(err: &io::Error) -> String {
    let text = err.to_string();
    let text = match err.raw_os_error() {
        Some(n) => text
            .strip_suffix(&format!(" (os error {n})"))
            .unwrap_or(&text)
            .to_owned(),
        None => text,
    };
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_codes() {
        let rows = [
            (Code::Usage, 2),
            (Code::UnknownCommand, 2),
            (Code::InvalidValue, 2),
            (Code::NoTty, 2),
            (Code::NoIdentity, 2),
            (Code::NoFile, 2),
            (Code::NoItem, 3),
            (Code::ExpectMismatch, 3),
            (Code::Blocked, 3),
            (Code::Cycle, 3),
            (Code::Claimed, 3),
            (Code::WrongState, 3),
            (Code::IdTaken, 3),
            (Code::Ambiguous, 3),
            (Code::EditConflict, 3),
            (Code::LockTimeout, 4),
            (Code::Io, 1),
        ];
        assert_eq!(rows.len(), Code::ALL.len());
        for (code, want) in rows {
            assert_eq!(code.exit(), want, "{code}");
        }
    }
    #[test]
    fn codes() {
        let mut got: Vec<_> = Code::ALL.iter().map(|c| c.as_str()).collect();
        got.sort_unstable();
        let mut want = vec![
            "usage",
            "unknown_command",
            "invalid_value",
            "no_tty",
            "no_identity",
            "no_file",
            "no_item",
            "expect_mismatch",
            "blocked",
            "cycle",
            "claimed",
            "wrong_state",
            "id_taken",
            "ambiguous",
            "edit_conflict",
            "lock_timeout",
            "io",
        ];
        want.sort_unstable();
        assert_eq!(got, want);
    }
    #[test]
    fn two_layer_io_errors() {
        let err = io::Error::from_raw_os_error(13);
        let at = io_at(
            "read",
            Path::new("/x/t.txt"),
            "open",
            Path::new("/x/t.txt"),
            err,
        );
        assert_eq!(
            at.message(),
            "read /x/t.txt: open /x/t.txt: permission denied"
        );
        assert_eq!(at.code(), Code::Io);
        assert!(std::error::Error::source(&at).is_some());
        let err = io::Error::from_raw_os_error(17);
        let rn = io_rename(Path::new("/x/.t"), Path::new("/x/t"), err);
        assert_eq!(
            rn.message(),
            "rename /x/.t: rename /x/.t /x/t: file exists"
        );
    }

    #[test]
    fn plain_io_error() {
        let err = Error::from(io::Error::other("disk on fire"));
        assert_eq!(err.code(), Code::Io);
        assert_eq!(err.exit(), 1);
        assert_eq!(err.message(), "disk on fire");
    }

    #[test]
    fn io_error_reads_like_go() {
        let err = io(
            "open",
            Path::new("/x/todo.txt"),
            io::Error::from_raw_os_error(2),
        );
        assert_eq!(
            err.message(),
            "open /x/todo.txt: no such file or directory"
        );
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn builder() {
        let item = Ref {
            nr: Nr::new(5).unwrap(),
            source: Source::Todo,
            id: Some("k3f9".to_owned()),
        };
        let err = Error::new(Code::Claimed, "5 is claimed by agent2")
            .hint("use --takeover")
            .item(item.clone())
            .field("started", Field::Str("2026-09-29T0900Z".to_owned()))
            .field("owner", Field::Str("agent2".to_owned()));
        assert_eq!(err.exit(), 3);
        assert_eq!(err.hint_text(), Some("use --takeover"));
        assert_eq!(err.item_ref(), Some(&item));
        let keys: Vec<_> = err.fields().map(|(k, _)| k).collect();
        assert_eq!(keys, ["owner", "started"]);
    }

    #[test]
    fn result_is_small() {
        assert_eq!(
            size_of::<Result<()>>(),
            size_of::<usize>(),
            "Error must stay one pointer wide"
        );
    }

    #[test]
    fn nr_is_never_zero() {
        assert!(Nr::new(0).is_none());
        assert_eq!(Nr::new(12).map(Nr::get), Some(12));
    }
}

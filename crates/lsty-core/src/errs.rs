//! The error type and the code table.

use std::{
    collections::BTreeMap, ffi::OsString, fmt, io, num::NonZeroU32,
    os::unix::ffi::OsStrExt as _, path::Path,
};

mod quote;
mod quote_table;

pub use quote::quote;

#[cfg(test)]
mod quote_tests;

/// Looks up an environment variable; the caller supplies it, nothing below
/// the binary reads the process's environment.
pub type Vars<'a> = dyn Fn(&str) -> Option<OsString> + 'a;

/// `path` as text, as [`lossy`] reads its bytes.
pub fn lossy_path(path: impl AsRef<Path>) -> String {
    lossy(path.as_ref().as_os_str().as_bytes())
}

/// `bytes` as text, with one U+FFFD for each byte that is not part of a
/// valid UTF-8 sequence.
pub fn lossy(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(chunk.invalid().iter().map(|_| '\u{fffd}'));
    }
    out
}

/// Exit code of an I/O or internal error.
pub const EXIT_IO: u8 = 1;
/// Exit code of a wrong call.
pub const EXIT_USAGE: u8 = 2;
/// Exit code when the list disagrees with the call.
pub const EXIT_CONFLICT: u8 = 3;
/// Exit code of a lock timeout.
pub const EXIT_LOCK_TIMEOUT: u8 = 4;

/// The hint of a usage error that has no synopsis to give.
pub const HELP_HINT: &str = "run \"lsty help\"";

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

    /// The NR of the line at 0-based `index`, or `None` when it does not
    /// fit.
    pub fn from_index(index: usize) -> Option<Self> {
        let n = u32::try_from(index).ok()?.checked_add(1)?;
        Self::new(n)
    }

    /// The 0-based position of the line: its number minus one.
    pub fn index(self) -> usize {
        self.0.get() as usize - 1
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

    /// How a line number of this file is written in a reference and in
    /// messages: the number, with `done:` before a number of the done
    /// file.
    pub fn numbered(self, nr: impl fmt::Display) -> String {
        match self {
            Self::Todo => nr.to_string(),
            Self::Done => format!("done:{nr}"),
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

/// What an error is about: an item, or a line that has none. One at
/// most.
#[derive(Debug)]
enum About {
    Item(Ref),
    Line(i64, Source),
}

#[derive(Debug)]
struct Inner {
    code: Code,
    message: String,
    hint: Option<String>,
    about: Option<About>,
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
            about: None,
            fields: BTreeMap::new(),
            source: None,
        }))
    }

    /// Sets the hint printed after the message.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.0.hint = Some(hint.into());
        self
    }

    /// Sets the item the error is about.
    #[must_use]
    pub fn with_item(mut self, item: Ref) -> Self {
        self.0.about = Some(About::Item(item));
        self
    }

    /// Sets the place of a line that has no item (replacing any item),
    /// written where the item goes with a null `id`. Unlike a [`Ref`],
    /// the number can be any integer: a reference may name line 0 or a
    /// line beyond any file.
    #[must_use]
    pub(crate) fn with_place(mut self, nr: i64, source: Source) -> Self {
        self.0.about = Some(About::Line(nr, source));
        self
    }

    /// Sets an extra field of the code's row in the table.
    #[must_use]
    pub fn with_field(mut self, key: &'static str, value: Field) -> Self {
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
    pub fn hint(&self) -> Option<&str> {
        self.0.hint.as_deref()
    }

    /// The item the error is about, if any.
    pub fn item(&self) -> Option<&Ref> {
        match self.0.about.as_ref() {
            Some(About::Item(item)) => Some(item),
            Some(About::Line(..)) | None => None,
        }
    }

    /// The place of a line that has no item, if one was set.
    pub fn place(&self) -> Option<(i64, Source)> {
        match self.0.about {
            Some(About::Line(nr, source)) => Some((nr, source)),
            Some(About::Item(_)) | None => None,
        }
    }

    /// The extra field `key`, if the error has one.
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.0.fields.get(key)
    }

    /// The text of the extra field `key`, if it is a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        if let Field::Str(s) = self.field(key)? {
            Some(s)
        } else {
            None
        }
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
    with_source(format!("{op} {}: {}", lossy_path(path), io_text(&err)), err)
}

/// An `io` error with two layers, worded
/// `read /x/todo.txt: open /x/todo.txt: permission denied`: the step
/// `op path` that failed, and the system call `inner_op inner_path` under
/// it.
pub(crate) fn io_at(
    op: &str,
    path: &Path,
    inner_op: &str,
    inner_path: &Path,
    err: io::Error,
) -> Error {
    let message = format!(
        "{op} {}: {inner_op} {}: {}",
        lossy_path(path),
        lossy_path(inner_path),
        io_text(&err)
    );
    with_source(message, err)
}

/// An `io` error for a failed rename of `from` to `to`, worded
/// `rename a: rename a b: file exists`.
pub(crate) fn io_rename(from: &Path, to: &Path, err: io::Error) -> Error {
    let message = format!(
        "rename {0}: rename {0} {1}: {2}",
        lossy_path(from),
        lossy_path(to),
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
        with_source(io_text(&err), err)
    }
}

/// The text of an I/O error without Rust's ` (os error N)` suffix
/// and with a lower-case first letter.
#[must_use]
pub fn io_text(err: &io::Error) -> String {
    let full = err.to_string();
    let text = err
        .raw_os_error()
        .and_then(|n| full.strip_suffix(&format!(" (os error {n})")))
        .unwrap_or(&full);
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_written_as_the_json_does() {
        for code in Code::ALL {
            assert_eq!(code.to_string(), code.as_str());
        }
        assert_eq!(Code::NoItem.to_string(), "no_item");
        assert_eq!(Source::Todo.as_str(), "todo");
        assert_eq!(Source::Done.as_str(), "done");
    }

    #[test]
    fn line_numbers_are_written_with_the_file() {
        assert_eq!(Source::Todo.numbered(7), "7");
        assert_eq!(Source::Done.numbered(7), "done:7");
        assert_eq!(
            Source::Done.numbered(format_args!("{:03}", 7)),
            "done:007"
        );
    }

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
    fn io_error_names_the_operation_and_the_path() {
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
            .with_hint("use --takeover")
            .with_item(item.clone())
            .with_field("started", Field::Str("2026-09-29T0900Z".to_owned()))
            .with_field("owner", Field::Str("agent2".to_owned()));
        assert_eq!(err.exit(), 3);
        assert_eq!(err.hint(), Some("use --takeover"));
        assert_eq!(err.item(), Some(&item));
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
    fn lossy_replaces_each_invalid_byte() {
        let rows: [(&[u8], &str); 5] = [
            (b"abc", "abc"),
            (b"caf\xe9", "caf\u{fffd}"),
            (b"a\xe2\x80b", "a\u{fffd}\u{fffd}b"),
            (b"\xff\xfe", "\u{fffd}\u{fffd}"),
            ("é☃".as_bytes(), "é☃"),
        ];
        for (bytes, want) in rows {
            assert_eq!(lossy(bytes), want, "{bytes:?}");
        }
    }

    #[test]
    fn nr_is_never_zero() {
        assert!(Nr::new(0).is_none());
        assert_eq!(Nr::new(12).map(Nr::get), Some(12));
        assert_eq!(Nr::from_index(0), Nr::new(1));
        assert_eq!(Nr::from_index(11).map(Nr::index), Some(11));
        assert_eq!(Nr::from_index(u32::MAX as usize), None);
    }
}

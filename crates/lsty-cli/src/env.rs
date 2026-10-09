//! What [`run`](crate::run) takes from the process.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::config::Config;
use lsty_core::discover::Found;
use lsty_core::errs::Vars;
use lsty_core::links::{self, HookResult};

/// One of the three standard streams, in the order of [`Env::is_tty`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stream {
    /// Standard input.
    Stdin,
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

impl Stream {
    /// Every stream, in the order of [`Env::is_tty`].
    #[cfg(test)]
    pub(crate) const ALL: [Self; 3] =
        [Self::Stdin, Self::Stdout, Self::Stderr];

    /// The device that names the stream in the message of a failed write.
    pub(crate) const fn device(self) -> &'static str {
        match self {
            Self::Stdin => "/dev/stdin",
            Self::Stdout => "/dev/stdout",
            Self::Stderr => "/dev/stderr",
        }
    }
}

/// What the TUI is opened with.
#[derive(Debug)]
pub struct TuiArgs {
    /// The todo and done files.
    pub found: Found,
    /// The configuration, with its warnings already printed.
    pub config: Config,
}

/// A hook that opens a file.
pub(crate) type FileHook<'a> = links::Hook<'a>;

/// Runs the editor and waits for it: the editor's program and arguments,
/// the file last. An error says why it could not run or did not succeed.
pub(crate) type EditorHook<'a> = dyn Fn(&[OsString]) -> HookResult + 'a;

/// Shows text in a pager: the pager's program and arguments, and the text
/// on its stdin. An error means the pager could not be started; how it
/// ended does not matter.
pub(crate) type PagerHook<'a> =
    dyn Fn(&[OsString], &[u8]) -> io::Result<()> + 'a;

/// Everything [`run`](crate::run) takes from the process, so that tests
/// can fake it.
pub struct Env<'a> {
    /// The arguments without the program name. A non-UTF-8 argument is a
    /// usage error.
    pub args: Vec<OsString>,
    /// Standard input.
    pub stdin: &'a mut dyn Read,
    /// Standard output.
    pub stdout: &'a mut dyn Write,
    /// Standard error.
    pub stderr: &'a mut dyn Write,
    /// Looks up an environment variable.
    pub vars: &'a Vars<'a>,
    /// The current folder: absolute, or `.` when the process cannot tell.
    pub cwd: PathBuf,
    /// The home folder.
    pub home: Option<PathBuf>,
    /// Whether stdin, stdout and stderr are terminals.
    pub is_tty: [bool; 3],
    /// The real time, read once per process.
    pub now: Timestamp,
    /// The local time zone.
    pub zone: TimeZone,
    /// The version `lsty -V` prints.
    pub version: &'static str,
    /// Runs the editor on a file and waits for it.
    pub run_editor: &'a EditorHook<'a>,
    /// Opens a file with the desktop's opener.
    pub open_path: &'a FileHook<'a>,
    /// Shows the help through a pager.
    pub run_pager: &'a PagerHook<'a>,
    /// Runs the TUI and returns its exit code; `None` when there is no
    /// TUI to run.
    pub run_tui: Option<&'a dyn Fn(TuiArgs) -> u8>,
}

impl Env<'_> {
    /// Opens `path` in the editor of `EDITOR` (or `vi`) and waits for it.
    ///
    /// # Errors
    ///
    /// The editor's failure, as the message of an error.
    pub(crate) fn edit(&self, path: &Path) -> Result<(), String> {
        let editor = (self.vars)("EDITOR");
        (self.run_editor)(&links::editor_command(editor.as_deref(), path))
    }
}

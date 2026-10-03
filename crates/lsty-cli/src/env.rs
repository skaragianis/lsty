//! What [`run`](crate::run) takes from the process.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::config::Config;
use lsty_core::discover::Found;
use lsty_core::errs::Vars;

/// The terminal streams, as indexes into [`Env::is_tty`].
pub const STDIN: usize = 0;
/// Standard output.
pub const STDOUT: usize = 1;
/// Standard error.
pub const STDERR: usize = 2;

/// What the TUI is opened with.
#[derive(Debug)]
pub struct TuiArgs {
    /// The todo and done files.
    pub found: Found,
    /// The configuration, with its warnings already printed.
    pub config: Config,
}

/// A hook that opens a file.
pub type FileHook<'a> = lsty_core::links::Hook<'a>;

/// Shows text in a pager: the pager's program and arguments, and the text
/// on its stdin. An error means the pager could not be started; how it
/// ended does not matter.
pub type PagerHook<'a> = dyn Fn(&[OsString], &[u8]) -> io::Result<()> + 'a;

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
    /// The current folder, absolute.
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
    /// Opens a file in the editor and waits for it.
    pub run_editor: &'a FileHook<'a>,
    /// Opens a file with the desktop's opener.
    pub open_path: &'a FileHook<'a>,
    /// Shows the help through a pager.
    pub run_pager: &'a PagerHook<'a>,
    /// Runs the TUI and returns its exit code; `None` when there is no
    /// TUI to run.
    pub run_tui: Option<&'a dyn Fn(TuiArgs) -> u8>,
}

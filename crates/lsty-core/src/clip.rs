//! The system clipboard: read with a helper program, written with an
//! OSC 52 escape sequence.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::path::PathBuf;
use std::time::Duration;

use crate::errs::{Code, Error, Result, lossy};
use crate::links::split_words;

mod osc52;
mod system;

pub use osc52::osc52;
pub use system::SystemRunner;

#[cfg(test)]
mod tests;

/// How long a reader may run before it is stopped.
pub(crate) const READ_LIMIT: Duration = Duration::from_secs(2);

/// The programs tried when `paste_command` is not set, in order.
const KNOWN: [(&str, &[&str]); 3] = [
    ("wl-paste", &["--no-newline"]),
    ("xclip", &["-selection", "clipboard", "-o"]),
    ("pbpaste", &[]),
];

/// A program and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// The program, as a path or a bare name.
    pub name: OsString,
    /// Its arguments.
    pub args: Vec<OsString>,
}

/// Where the text on the clipboard comes from.
pub trait Reader {
    /// The text on the clipboard.
    ///
    /// # Errors
    ///
    /// There is no reader, or it failed, ran out of time or did not
    /// finish.
    fn read(&self) -> Result<Vec<u8>>;
}

/// Runs a program and gives back what it printed.
pub trait Runner {
    /// Runs `program` for at most `limit` and returns its standard
    /// output. A program that is still running at the limit is killed.
    ///
    /// # Errors
    ///
    /// The program cannot be started, exits with an error or runs out of
    /// time.
    fn run(&self, program: &Program, limit: Duration) -> io::Result<Vec<u8>>;
}

/// Looks for a program by name, as the shell does.
pub(crate) type Finder<'a> = dyn Fn(&OsStr) -> Option<PathBuf> + 'a;

/// The program that reads the clipboard: the one `paste_command` names,
/// or else the first known one that `find` finds.
pub(crate) fn choose(
    paste_command: &str,
    find: &Finder<'_>,
) -> Option<Program> {
    let words = split_words(OsStr::new(paste_command));
    if let Some((first, rest)) = words.split_first() {
        let name =
            find(first).map_or_else(|| first.clone(), PathBuf::into_os_string);
        return Some(Program {
            name,
            args: rest.to_vec(),
        });
    }
    KNOWN.iter().find_map(|(name, args)| {
        Some(Program {
            name: find(OsStr::new(name))?.into_os_string(),
            args: args.iter().map(OsString::from).collect(),
        })
    })
}

/// A clipboard read by running a program, or by nothing when none was
/// found.
#[derive(Debug)]
pub struct Clipboard<R> {
    program: Option<Program>,
    runner: R,
}

impl<R: Runner> Clipboard<R> {
    /// A reader that runs `program` with `runner`.
    pub(crate) const fn new(program: Option<Program>, runner: R) -> Self {
        Self { program, runner }
    }

    /// The program this reader runs.
    #[cfg(test)]
    pub(crate) const fn program(&self) -> Option<&Program> {
        self.program.as_ref()
    }
}

impl Clipboard<SystemRunner> {
    /// The reader that `paste_command` or the programs found by `find`
    /// give, running real programs.
    pub fn system(paste_command: &str, find: &Finder<'_>) -> Self {
        Self::new(choose(paste_command, find), SystemRunner)
    }
}

impl<R: Runner> Reader for Clipboard<R> {
    fn read(&self) -> Result<Vec<u8>> {
        let Some(program) = &self.program else {
            return Err(Error::new(Code::Io, "no clipboard reader found"));
        };
        let name = lossy(program.name.as_bytes());
        let mut out = self.runner.run(program, READ_LIMIT).map_err(|e| {
            Error::new(
                Code::Io,
                format!("read the clipboard with {name}: run {name}: {e}"),
            )
        })?;
        if out.last() == Some(&b'\n') {
            out.pop();
        }
        Ok(out)
    }
}

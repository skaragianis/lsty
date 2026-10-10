//! The system clipboard: read with a helper program, written with an
//! OSC 52 escape sequence.

use std::{
    ffi::{OsStr, OsString},
    io,
    os::unix::ffi::OsStrExt as _,
    path::PathBuf,
    time::Duration,
};

use crate::{
    errs::{Code, Error, Result, lossy},
    links::split_words,
};

mod osc52;
mod system;

pub use osc52::osc52;
pub use system::OsRunner;

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
    /// The file to run: what the search found.
    pub file: PathBuf,
    /// The program as named, which it sees as its own name.
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
///
/// An error is why the program may not be run.
pub(crate) type Finder<'a> = dyn Fn(&OsStr) -> Result<PathBuf, String> + 'a;

/// The program that reads the clipboard: the one `paste_command` names,
/// or else the first known one that `find` finds. The error is the
/// message of a read that cannot start: no program, or one `find`
/// refused.
pub(crate) fn choose(
    paste_command: &str,
    find: &Finder<'_>,
) -> Result<Program, String> {
    let words = split_words(OsStr::new(paste_command));
    if let Some((first, rest)) = words.split_first() {
        let file = find(first).map_err(|why| read_failure(first, &why))?;
        return Ok(Program {
            file,
            name: first.clone(),
            args: rest.to_vec(),
        });
    }
    KNOWN
        .iter()
        .find_map(|(name, args)| {
            Some(Program {
                file: find(OsStr::new(name)).ok()?,
                name: name.into(),
                args: args.iter().map(OsString::from).collect(),
            })
        })
        .ok_or_else(|| "no clipboard reader found".to_owned())
}

/// The message of a read with `name` that failed with `why`.
fn read_failure(name: &OsStr, why: &dyn std::fmt::Display) -> String {
    let name = lossy(name.as_bytes());
    format!("read the clipboard with {name}: run {name}: {why}")
}

/// A clipboard read by running a program, or by nothing when none was
/// found.
#[derive(Debug)]
pub struct Clipboard<R> {
    program: Result<Program, String>,
    runner: R,
}

impl<R: Runner> Clipboard<R> {
    /// A reader that runs `program` with `runner`.
    pub(crate) const fn new(
        program: Result<Program, String>,
        runner: R,
    ) -> Self {
        Self { program, runner }
    }

    /// The program this reader runs.
    #[cfg(test)]
    pub(crate) fn program(&self) -> Option<&Program> {
        self.program.as_ref().ok()
    }
}

impl Clipboard<OsRunner> {
    /// The reader that `paste_command` or the programs found by `find`
    /// give, running real programs.
    pub fn system(paste_command: &str, find: &Finder<'_>) -> Self {
        Self::new(choose(paste_command, find), OsRunner)
    }
}

impl<R: Runner> Reader for Clipboard<R> {
    fn read(&self) -> Result<Vec<u8>> {
        let program = self
            .program
            .as_ref()
            .map_err(|why| Error::new(Code::Io, why.clone()))?;
        let mut out = self.runner.run(program, READ_LIMIT).map_err(|e| {
            Error::new(Code::Io, read_failure(&program.name, &e))
        })?;
        if out.last() == Some(&b'\n') {
            out.pop();
        }
        Ok(out)
    }
}

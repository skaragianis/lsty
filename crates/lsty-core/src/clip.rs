//! The system clipboard: read and written with helper programs, and
//! written with an OSC 52 escape sequence.

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
const READ_LIMIT: Duration = Duration::from_secs(2);

/// How long a writer may run before it is stopped: less, because the
/// screen waits for it.
const WRITE_LIMIT: Duration = Duration::from_millis(500);

/// One direction of the clipboard: how its failures are worded and the
/// programs tried when no command is set, in order.
struct Way {
    verb: &'static str,
    noun: &'static str,
    known: &'static [(&'static str, &'static [&'static str])],
}

const READ: Way = Way {
    verb: "read",
    noun: "reader",
    known: &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("pbpaste", &[]),
    ],
};

const WRITE: Way = Way {
    verb: "write",
    noun: "writer",
    known: &[
        ("pbcopy", &[]),
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
    ],
};

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

/// Where the text on the clipboard comes from and goes to.
pub trait Pasteboard {
    /// The text on the clipboard.
    ///
    /// # Errors
    ///
    /// There is no reader, or every one failed, ran out of time or did
    /// not finish.
    fn read(&self) -> Result<Vec<u8>>;

    /// Puts `text` on the clipboard with the programs that do it.
    ///
    /// # Errors
    ///
    /// The program that `copy_command` names failed. A program that
    /// was found by itself may fail without a word, because the
    /// terminal may have taken the text.
    fn write(&self, text: &[u8]) -> Result<()>;
}

/// Runs a program and gives back what it printed.
pub trait Runner {
    /// Runs `program` for at most `limit` and returns its standard
    /// output. With `input` the program is a writer: it is given the
    /// text on its standard input and its output is not kept. A program
    /// that is still running at the limit is killed.
    ///
    /// # Errors
    ///
    /// The program cannot be started, exits with an error or runs out of
    /// time.
    fn run(
        &self,
        program: &Program,
        input: Option<&[u8]>,
        limit: Duration,
    ) -> io::Result<Vec<u8>>;
}

/// Looks for a program by name, as the shell does.
///
/// An error is why the program may not be run.
type Finder<'a> = dyn Fn(&OsStr) -> Result<PathBuf, String> + 'a;

/// The programs for `way`: the one `command` names, or else every
/// known one that `find` finds, in order. A program that cannot be run
/// is the message of the run that cannot start: no program, or one
/// `find` refused.
fn choose(
    command: &str,
    way: &Way,
    find: &Finder<'_>,
) -> Vec<Result<Program, String>> {
    let words = split_words(OsStr::new(command));
    if let Some((first, rest)) = words.split_first() {
        let found = find(first).map_err(|why| failure(way, first, &why));
        return vec![found.map(|file| Program {
            file,
            name: first.clone(),
            args: rest.to_vec(),
        })];
    }
    let found: Vec<_> = way
        .known
        .iter()
        .filter_map(|(name, args)| {
            Some(Ok(Program {
                file: find(OsStr::new(name)).ok()?,
                name: name.into(),
                args: args.iter().map(OsString::from).collect(),
            }))
        })
        .collect();
    if found.is_empty() {
        vec![Err(format!("no clipboard {} found", way.noun))]
    } else {
        found
    }
}

/// The message of a run of `name` for `way` that failed with `why`.
fn failure(way: &Way, name: &OsStr, why: &dyn std::fmt::Display) -> String {
    let (verb, name) = (way.verb, lossy(name.as_bytes()));
    format!("{verb} the clipboard with {name}: run {name}: {why}")
}

/// A clipboard read and written by running programs.
#[derive(Debug)]
pub struct Clipboard<R> {
    /// Tried in order until one reads.
    readers: Vec<Result<Program, String>>,
    /// Tried in order until one writes.
    writers: Vec<Result<Program, String>>,
    /// Written after the writers, whether they did or not.
    mirror: Option<Result<Program, String>>,
    /// A failed writer is an error: `copy_command` named it.
    loud: bool,
    runner: R,
}

impl<R: Runner> Clipboard<R> {
    /// The clipboard that `paste_command`, `copy_command` or the programs
    /// found by `find` give, running programs with `runner`. Inside tmux
    /// (`tmux`) its buffer is written too, and read when the system
    /// clipboard cannot be.
    pub fn with(
        runner: R,
        paste_command: &str,
        copy_command: &str,
        tmux: bool,
        find: &Finder<'_>,
    ) -> Self {
        let unset = |c: &str| split_words(OsStr::new(c)).is_empty();
        let in_tmux = |args: &[&str]| {
            let file = find(OsStr::new("tmux")).ok().filter(|_| tmux)?;
            let args = args.iter().map(OsString::from).collect();
            Some(Ok(Program {
                file,
                name: "tmux".into(),
                args,
            }))
        };
        let loud = !unset(copy_command);
        let mut readers = choose(paste_command, &READ, find);
        readers.extend(
            in_tmux(&["save-buffer", "-"]).filter(|_| unset(paste_command)),
        );
        Self {
            readers,
            writers: choose(copy_command, &WRITE, find),
            mirror: in_tmux(&["load-buffer", "-"]).filter(|_| !loud),
            loud,
            runner,
        }
    }

    /// Runs `programs` in order until one succeeds, to read, or with
    /// `text` to write: what it printed, else the first error.
    fn run(
        &self,
        programs: &[Result<Program, String>],
        text: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        let (way, limit) = match text {
            Some(_) => (&WRITE, WRITE_LIMIT),
            None => (&READ, READ_LIMIT),
        };
        let mut tries = programs.iter().map(|program| {
            let program = program
                .as_ref()
                .map_err(|why| Error::new(Code::Io, why.clone()))?;
            self.runner.run(program, text, limit).map_err(|e| {
                Error::new(Code::Io, failure(way, &program.name, &e))
            })
        });
        let first = tries.next().unwrap_or_else(|| Ok(Vec::new()));
        first.or_else(|e| tries.find_map(Result::ok).ok_or(e))
    }
}

impl<R: Runner> Pasteboard for Clipboard<R> {
    fn read(&self) -> Result<Vec<u8>> {
        let mut out = self.run(&self.readers, None)?;
        if out.last() == Some(&b'\n') {
            out.pop();
        }
        Ok(out)
    }

    fn write(&self, text: &[u8]) -> Result<()> {
        let done = self.run(&self.writers, Some(text));
        drop(self.run(self.mirror.as_slice(), Some(text)));
        if self.loud { done.map(drop) } else { Ok(()) }
    }
}

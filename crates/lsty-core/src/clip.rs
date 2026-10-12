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
    /// There is no reader, or it failed, ran out of time or did not
    /// finish.
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

/// The program for `way`: the one `command` names, or else the first
/// known one that `find` finds. The error is the message of a run that
/// cannot start: no program, or one `find` refused.
fn choose(
    command: &str,
    way: &Way,
    find: &Finder<'_>,
) -> Result<Program, String> {
    let words = split_words(OsStr::new(command));
    if let Some((first, rest)) = words.split_first() {
        let file = find(first).map_err(|why| failure(way, first, &why))?;
        return Ok(Program {
            file,
            name: first.clone(),
            args: rest.to_vec(),
        });
    }
    way.known
        .iter()
        .find_map(|(name, args)| {
            Some(Program {
                file: find(OsStr::new(name)).ok()?,
                name: name.into(),
                args: args.iter().map(OsString::from).collect(),
            })
        })
        .ok_or_else(|| format!("no clipboard {} found", way.noun))
}

/// The message of a run of `name` for `way` that failed with `why`.
fn failure(way: &Way, name: &OsStr, why: &dyn std::fmt::Display) -> String {
    let (verb, name) = (way.verb, lossy(name.as_bytes()));
    format!("{verb} the clipboard with {name}: run {name}: {why}")
}

/// A clipboard read and written by running programs.
#[derive(Debug)]
pub struct Clipboard<R> {
    program: Result<Program, String>,
    /// What reads when `program` fails.
    fallback: Option<Program>,
    writers: Result<Vec<Program>, String>,
    /// A failed writer is an error: `copy_command` named it.
    loud: bool,
    runner: R,
}

impl<R: Runner> Clipboard<R> {
    /// A clipboard that reads with `program` run by `runner`, and
    /// writes with nothing.
    pub(crate) const fn new(
        program: Result<Program, String>,
        runner: R,
    ) -> Self {
        Self {
            program,
            fallback: None,
            writers: Ok(Vec::new()),
            loud: false,
            runner,
        }
    }

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
            Some(Program {
                file,
                name: "tmux".into(),
                args,
            })
        };
        let loud = !unset(copy_command);
        let writers = if loud {
            choose(copy_command, &WRITE, find).map(|p| vec![p])
        } else {
            let known = choose("", &WRITE, find).ok();
            Ok(known
                .into_iter()
                .chain(in_tmux(&["load-buffer", "-"]))
                .collect())
        };
        Self {
            fallback: in_tmux(&["save-buffer", "-"])
                .filter(|_| unset(paste_command)),
            writers,
            loud,
            ..Self::new(choose(paste_command, &READ, find), runner)
        }
    }
}

impl<R: Runner> Pasteboard for Clipboard<R> {
    fn read(&self) -> Result<Vec<u8>> {
        let read = |program: &Program| {
            let mut out =
                self.runner.run(program, None, READ_LIMIT).map_err(|e| {
                    Error::new(Code::Io, failure(&READ, &program.name, &e))
                })?;
            if out.last() == Some(&b'\n') {
                out.pop();
            }
            Ok(out)
        };
        let first = self
            .program
            .as_ref()
            .map_err(|why| Error::new(Code::Io, why.clone()))
            .and_then(read);
        match (&first, &self.fallback) {
            (Err(_), Some(tmux)) => read(tmux).or(first),
            _ => first,
        }
    }

    fn write(&self, text: &[u8]) -> Result<()> {
        let writers = self
            .writers
            .as_ref()
            .map_err(|why| Error::new(Code::Io, why.clone()))?;
        let mut done = Ok(());
        for program in writers {
            let ran = self
                .runner
                .run(program, Some(text), WRITE_LIMIT)
                .map_err(|e| {
                    Error::new(Code::Io, failure(&WRITE, &program.name, &e))
                });
            done = done.and_then(|()| ran.map(drop));
        }
        if self.loud { done } else { Ok(()) }
    }
}

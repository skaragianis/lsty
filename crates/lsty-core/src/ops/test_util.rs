//! Helpers shared by the tests of this module.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::{
    Edited, Env, Grant, Outcome, Pin, Seen, Settle, Tracked, edit_pinned,
};
use crate::config::Config;
use crate::dates::Clock;
use crate::discover::{FileState, Found, FoundBy};
use crate::errs::{Code, Error, Nr, Result};
use crate::list::Snapshot;
use crate::todotxt::File;

/// A temp folder with a todo file and, maybe, a done file, and what an
/// [`Env`] borrows.
pub(super) struct Dir {
    _tmp: TempDir,
    pub(super) found: Found,
    pub(super) config: Config,
    pub(super) clock: Clock,
}

/// Today is 2026-09-29 and now is 09:00 UTC.
fn clock() -> Clock {
    clock_at("2026-09-29T0900Z")
}

/// A clock that reads `now` (the `LSTY_NOW` form).
pub(super) fn clock_at(now: &str) -> Clock {
    let var = |name: &str| (name == "LSTY_NOW").then(|| now.into());
    Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, jiff::tz::TimeZone::UTC)
        .expect("clock")
}

pub(super) fn folder(todo: &str, done: Option<&str>) -> Dir {
    let tmp = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(tmp.path()).expect("canonicalize");
    let found = Found {
        todo: real.join("todo.txt"),
        done: real.join("done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    };
    fs::write(&found.todo, todo).expect("write todo");
    if let Some(done) = done {
        fs::write(&found.done, done).expect("write done");
    }
    Dir {
        _tmp: tmp,
        found,
        config: Config::default(),
        clock: clock(),
    }
}

impl Dir {
    pub(super) fn env(&self) -> Env<'_> {
        let mut env = Env::new(&self.found, &self.config, &self.clock);
        env.record_undo = true;
        env
    }

    pub(super) fn todo(&self) -> String {
        read(&self.found.todo)
    }

    pub(super) fn done(&self) -> String {
        read(&self.found.done)
    }

    pub(super) fn dir(&self) -> PathBuf {
        self.found
            .todo
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default()
    }
}

fn read(path: &std::path::Path) -> String {
    String::from_utf8(fs::read(path).expect("read")).expect("utf8")
}

/// The code, message and hint of an error.
pub(super) fn parts(err: &Error) -> (Code, &str, &str) {
    (err.code(), err.message(), err.hint().unwrap_or_default())
}

/// Messages as bytes, to compare with what an operation said.
pub(super) fn said(messages: &[&str]) -> Vec<Vec<u8>> {
    messages.iter().map(|m| m.as_bytes().to_vec()).collect()
}

/// A source of ids for tests.
pub(super) struct Sequence {
    given: Option<Vec<String>>,
    at: std::cell::Cell<usize>,
}

impl Sequence {
    /// `aaaa`, `bbbb`, `cccc`, ...: the next letter, repeated as many
    /// times as the length asked for.
    pub(super) fn letters() -> Self {
        Self {
            given: None,
            at: std::cell::Cell::new(0),
        }
    }

    /// The given ids in order; asking for more fails.
    pub(super) fn list<S: Into<String>>(
        ids: impl IntoIterator<Item = S>,
    ) -> Self {
        Self {
            given: Some(ids.into_iter().map(Into::into).collect()),
            at: std::cell::Cell::new(0),
        }
    }
}

impl super::IdSource for Sequence {
    fn next(&self, length: usize) -> Result<String> {
        let at = self.at.get();
        self.at.set(at + 1);
        if let Some(ids) = &self.given {
            return ids.get(at).cloned().ok_or_else(|| {
                Error::new(Code::Io, "the test id source ran out")
            });
        }
        let letter =
            u8::try_from(at % 26).map_or('z', |n| char::from(b'a' + n));
        Ok(letter.to_string().repeat(length))
    }
}

/// The reading of the files of `dir` that held `todo` and `done`.
pub(super) fn snapshot(dir: &Dir, todo: &[u8], done: &[u8]) -> Snapshot {
    Snapshot::build(
        dir.found.clone(),
        File::parse(todo),
        File::parse(done),
        &dir.env().params(),
    )
}

/// Writes `draft` over line `nr`, which read `base` in the file as it
/// is. A conflict is an error.
pub(super) fn typed(
    dir: &Dir,
    nr: Nr,
    base: &[u8],
    draft: &[u8],
    grants: &[Grant],
) -> Result<Outcome> {
    let pin = Pin::new(nr, base, Settle::LineOrBlank);
    let seen = Seen::of(&snapshot(dir, dir.todo().as_bytes(), b""));
    let tracked = Tracked::new(pin, seen);
    match edit_pinned(&dir.env(), &tracked, draft, grants)? {
        Edited::Written(out) => Ok(*out),
        Edited::Conflict(c) => Err(c.error()),
    }
}

/// Owned copies of string literals.
pub(super) fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| (*t).to_owned()).collect()
}

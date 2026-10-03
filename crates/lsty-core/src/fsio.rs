//! Reading and writing the todo and done files without losing a byte.
//!
//! [`write()`] goes through a synced temp file in the target's folder, a
//! rename and a folder sync, so the target holds either the old content or
//! the new, never half of either. [`resolve`] follows symlinks first, so a
//! link stays a link and the temp file lands beside the real file.

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::errs;

mod hooks;
mod names;
mod read;
mod resolve;
mod write;

pub use hooks::{Hooks, Os};
pub(crate) use names::{base_name, folder_of};
pub use read::{hash, read};
pub(crate) use resolve::clean;
pub use resolve::resolve;
pub use write::{clean_orphans, write};

#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
pub(crate) mod test_util;
#[cfg(test)]
mod write_tests;

/// The cause of the error a hook returns to stand for a process that died
/// at that point.
#[derive(Debug)]
pub struct Crash;

impl fmt::Display for Crash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("fsio: simulated crash")
    }
}

impl StdError for Crash {}

/// The cause of the error [`resolve`] returns when the folder of a path
/// does not exist or is not a folder.
#[derive(Debug)]
pub struct NoDir(pub PathBuf);

impl fmt::Display for NoDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fsio: folder does not exist: {}", self.0.display())
    }
}

impl StdError for NoDir {}

/// The error a hook returns to simulate a crash.
pub fn crash() -> io::Error {
    io::Error::other(Crash)
}

/// Whether `err` is the simulated crash of a hook.
pub fn is_crash(err: &errs::Error) -> bool {
    caused_by::<Crash>(err)
}

/// Whether `err` says a folder does not exist.
pub fn is_no_dir(err: &errs::Error) -> bool {
    caused_by::<NoDir>(err)
}

fn caused_by<T: StdError + 'static>(err: &errs::Error) -> bool {
    StdError::source(err)
        .and_then(|s| s.downcast_ref::<io::Error>())
        .and_then(io::Error::get_ref)
        .is_some_and(<dyn StdError + Send + Sync>::is::<T>)
}

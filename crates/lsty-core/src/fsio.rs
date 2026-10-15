//! Reading and writing the todo and done files without losing a byte.
//!
//! [`write()`] goes through a synced temp file in the target's folder, a
//! rename and a folder sync, so the target holds either the old content or
//! the new, never half of either. A path is resolved through symlinks
//! first, so a link stays a link and the temp file lands beside the real
//! file.

use std::{error::Error as StdError, fmt, io, path::PathBuf};

use crate::{Vars, errs};

/// `name`, when it is set to something: an empty value counts as unset.
#[must_use]
pub fn non_empty(vars: &Vars<'_>, name: &str) -> Option<std::ffi::OsString> {
    vars(name).filter(|v| !v.is_empty())
}

mod hooks;
mod names;
mod read;
mod resolve;
mod write;

pub use hooks::{Hooks, OsHooks};
pub(crate) use names::{base_name, folder_of};
pub use read::read;
pub(crate) use read::read_in;
pub(crate) use resolve::resolve;
pub(crate) use write::clean_orphans;
pub use write::write;

pub(crate) use crate::todotxt::clean;

#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
pub(crate) mod test_util;

/// The cause of the error a hook returns to stand for a process that died
/// at that point.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct Crash;

#[cfg(test)]
impl fmt::Display for Crash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("fsio: simulated crash")
    }
}

#[cfg(test)]
impl StdError for Crash {}

/// The cause of the error [`resolve()`] returns when the folder of a path
/// does not exist or is not a folder.
#[derive(Debug)]
struct NoDir(pub PathBuf);

impl fmt::Display for NoDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "fsio: folder does not exist: {}",
            errs::lossy_path(&self.0)
        )
    }
}

impl StdError for NoDir {}

/// The error a hook returns to simulate a crash.
#[cfg(test)]
pub(crate) fn crash() -> io::Error {
    io::Error::other(Crash)
}

/// Whether `err` is the simulated crash of a hook.
#[cfg(test)]
pub(crate) fn is_crash(err: &errs::Error) -> bool {
    caused_by::<Crash>(err)
}

/// Whether `err` says a folder does not exist.
pub(crate) fn is_no_dir(err: &errs::Error) -> bool {
    caused_by::<NoDir>(err)
}

fn caused_by<T: StdError + 'static>(err: &errs::Error) -> bool {
    StdError::source(err)
        .and_then(|s| s.downcast_ref::<io::Error>())
        .and_then(io::Error::get_ref)
        .is_some_and(<dyn StdError + Send + Sync>::is::<T>)
}

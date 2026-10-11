//! The advisory lock every read and write of a list holds.
//!
//! The lock of a file is a `flock` on the folder that holds it, opened
//! read-only: the file itself is never locked, because every write renames
//! a new file over it, and no file is created for the lock. Folders are
//! locked in byte order of their path with a timeout for the whole list,
//! so two processes asking for the same folders cannot deadlock. A
//! filesystem that refuses folder locks gets a sidecar file per list file
//! instead.

use std::{
    fs::File,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::errs;

mod flock;
mod folders;
mod sidecar;
mod wait;

pub use flock::Flock;
pub(crate) use flock::OsFlock;
use folders::Folder;
pub use wait::Cancel;
use wait::{Failure, Waiter};

#[cfg(test)]
pub(crate) mod tests;

/// How a lock is held. A lock is never upgraded: a caller that will write
/// asks for [`Mode::Exclusive`] from the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    /// Any number of holders at once; reads use it.
    Shared,
    /// One holder and no shared holders; writes use it.
    Exclusive,
}

/// A request for the lock of the folder of `file`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Want {
    /// An absolute, clean path with no `..` components. Its folder is
    /// taken as written, so two spellings of one folder lock it twice.
    pub file: PathBuf,
    /// How the lock is held.
    pub mode: Mode,
}

/// A set of locks taken together by [`acquire`]. Dropping it releases them.
#[derive(Debug, Default)]
pub struct Held {
    /// Each lock with the path it was taken on.
    locks: Vec<(File, PathBuf)>,
}

impl Held {
    /// Unlocks in reverse order and closes the descriptors. Calling it
    /// again does nothing.
    pub fn release(&mut self) -> errs::Result<()> {
        let mut failed = None;
        while let Some((file, at)) = self.locks.pop() {
            if let Err(e) = file.unlock() {
                failed.get_or_insert_with(|| errs::io("unlock", &at, e));
            }
        }
        failed.map_or(Ok(()), Err)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // The kernel drops a lock with its descriptor, so a failed unlock
        // has nothing left to report.
        drop(self.release());
    }
}

/// Takes every lock in `wants`, in path order, within `timeout` for the
/// whole list. It holds all of them or none.
pub fn acquire(
    wants: &[Want],
    timeout: Duration,
    cancel: &Cancel,
) -> errs::Result<Held> {
    acquire_with(&OsFlock, wants, timeout, cancel)
}

/// [`acquire`] with the `flock` call replaced by `flock`.
pub fn acquire_with<F: Flock + ?Sized>(
    flock: &F,
    wants: &[Want],
    timeout: Duration,
    cancel: &Cancel,
) -> errs::Result<Held> {
    let wait = Waiter::new(cancel, Instant::now().checked_add(timeout));
    let mut held = Held::default();
    for folder in folders::group(wants) {
        lock_folder(flock, &wait, &mut held, &folder)?;
    }
    Ok(held)
}

fn lock_folder<F: Flock + ?Sized>(
    flock: &F,
    wait: &Waiter<'_>,
    held: &mut Held,
    folder: &Folder,
) -> errs::Result<()> {
    let dir = &folder.dir;
    let handle = File::open(dir)
        .map_err(|e| errs::io_at("lock", dir, "open", dir, e))?;
    match wait.lock(flock, &handle, folder.mode()) {
        Ok(()) => held.locks.push((handle, dir.clone())),
        Err(Failure::Io(e)) if flock::unsupported(&e) => {
            return sidecar::lock_all(flock, wait, held, folder);
        }
        Err(other) => return Err(other.into_error(dir, &folder.first_name())),
    }
    Ok(())
}

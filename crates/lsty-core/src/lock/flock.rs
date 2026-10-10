//! The `flock` call, replaceable by tests.

use std::{fs::File, io};

use rustix::io::Errno;

use super::Mode;

/// The system call that takes a lock without waiting.
pub trait Flock {
    /// Takes the lock on `file`, or fails with
    /// [`io::ErrorKind::WouldBlock`] when another holder has it.
    fn try_lock(&self, file: &File, mode: Mode) -> io::Result<()>;
}

/// The production call: `flock(2)` through [`File`].
pub(crate) struct OsFlock;

impl Flock for OsFlock {
    fn try_lock(&self, file: &File, mode: Mode) -> io::Result<()> {
        match mode {
            Mode::Shared => file.try_lock_shared(),
            Mode::Exclusive => file.try_lock(),
        }
        .map_err(Into::into)
    }
}

/// Whether the filesystem cannot lock a file (`EINVAL`, `ENOTSUP`,
/// `EOPNOTSUPP`, `ENOLCK`).
pub(super) fn unsupported(err: &io::Error) -> bool {
    let Some(raw) = err.raw_os_error() else {
        return false;
    };
    [Errno::INVAL, Errno::NOTSUP, Errno::OPNOTSUPP, Errno::NOLCK]
        .iter()
        .any(|e| e.raw_os_error() == raw)
}

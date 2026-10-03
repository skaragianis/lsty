//! The `flock` call, replaceable by tests.

use std::fs::{File, TryLockError};
use std::io;

use rustix::io::Errno;

use super::Mode;

/// The system call that takes a lock without waiting.
pub trait Flock {
    /// Takes the lock on `file`, or fails with
    /// [`io::ErrorKind::WouldBlock`] when another holder has it.
    fn try_lock(&self, file: &File, mode: Mode) -> io::Result<()>;
}

/// The production call: `flock(2)` through [`File`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Os;

impl Flock for Os {
    fn try_lock(&self, file: &File, mode: Mode) -> io::Result<()> {
        let result = match mode {
            Mode::Shared => file.try_lock_shared(),
            Mode::Exclusive => file.try_lock(),
        };
        match result {
            Ok(()) => Ok(()),
            Err(TryLockError::WouldBlock) => {
                Err(io::ErrorKind::WouldBlock.into())
            }
            Err(TryLockError::Error(e)) => Err(e),
        }
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

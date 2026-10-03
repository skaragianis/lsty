//! The points of a write that tests observe or replace.

use std::fs::File;
use std::io;
use std::path::Path;

use rustix::io::Errno;

/// Hooks run at fixed points of [`write`](super::write). An error stops the
/// write there and leaves the disk as it is. The sync methods stand for the
/// system calls, so a test can record or fail them.
pub trait Hooks {
    /// Runs after the temp file is synced and before the rename.
    fn before_rename(&self, _target: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Runs after the rename and before the folder sync.
    fn after_rename(&self, _target: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Runs after the folder sync.
    fn after_dir_sync(&self, _target: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Flushes the temp file to disk.
    fn sync_file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    /// Flushes the open folder to disk. A filesystem that cannot is
    /// skipped.
    fn sync_dir(&self, dir: &File) -> io::Result<()> {
        match dir.sync_all() {
            Err(e) if unsupported_sync(&e) => Ok(()),
            other => other,
        }
    }
}

/// The production hooks: real syncs and nothing else.
#[derive(Debug, Clone, Copy, Default)]
pub struct Os;

impl Hooks for Os {}

/// Whether the filesystem cannot sync a folder (`EINVAL`, `ENOTSUP`,
/// `ENOTTY`).
pub(super) fn unsupported_sync(err: &io::Error) -> bool {
    let Some(raw) = err.raw_os_error() else {
        return false;
    };
    [Errno::INVAL, Errno::NOTSUP, Errno::NOTTY]
        .iter()
        .any(|e| e.raw_os_error() == raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [(Errno, bool); 4] = [
        (Errno::INVAL, true),
        (Errno::NOTSUP, true),
        (Errno::NOTTY, true),
        (Errno::IO, false),
    ];

    #[test]
    fn unsupported_sync_errors() {
        for (errno, want) in ROWS {
            let raw = errno.raw_os_error();
            let err = io::Error::from_raw_os_error(raw);
            assert_eq!(unsupported_sync(&err), want, "errno {raw}");
        }
        assert!(!unsupported_sync(&io::Error::other("x")));
    }
}

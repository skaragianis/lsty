//! The points of a write that tests observe or replace.

use std::{fs::File, io, path::Path};

use rustix::io::Errno;

/// Hooks run at fixed points of [`write`](super::write()). An error stops the
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
#[derive(Debug, Clone, Copy)]
pub struct OsHooks;

impl Hooks for OsHooks {}

/// Whether the filesystem cannot sync a folder (`EINVAL`, `ENOTSUP`,
/// `ENOTTY`).
pub(super) fn unsupported_sync(err: &io::Error) -> bool {
    Errno::from_io_error(err).is_some_and(|e| {
        [Errno::INVAL, Errno::NOTSUP, Errno::NOTTY].contains(&e)
    })
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
            let err = io::Error::from(errno);
            assert_eq!(unsupported_sync(&err), want, "errno {errno:?}");
        }
        assert!(!unsupported_sync(&io::Error::other("x")));
    }

    /// The system syncs fail on a device node (`EINVAL`) and on a handle
    /// that only names a folder (`EBADF`); a folder sync skips the first.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_syncs_reach_the_system() {
        use rustix::fs::{Mode, OFlags, open};

        let code = |r: io::Result<()>| r.map_err(|e| e.raw_os_error());
        let device = File::open("/dev/null").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            code(OsHooks.sync_file(&device)),
            Err(Some(Errno::INVAL.raw_os_error()))
        );
        assert_eq!(code(OsHooks.sync_dir(&device)), Ok(()));
        let named = open("/", OFlags::PATH, Mode::empty())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            code(OsHooks.sync_dir(&File::from(named))),
            Err(Some(Errno::BADF.raw_os_error()))
        );
    }
}

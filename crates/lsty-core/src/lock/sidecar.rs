//! The fallback for filesystems that refuse folder locks: one lock file
//! per list file, `.<file>.lock` in the same folder, never removed.

use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
};

use rustix::io::Errno;

use super::{Flock, Held, Mode, folders::Folder, wait::Waiter};
use crate::{errs, fsio::base_name};

/// The mode of a new sidecar before the umask.
const SIDECAR_MODE: u32 = 0o666;

/// The sidecar of `file`.
pub(super) fn path(file: &Path) -> PathBuf {
    let mut name = b".".to_vec();
    name.extend_from_slice(base_name(file).as_bytes());
    name.extend_from_slice(b".lock");
    file.with_file_name(OsString::from_vec(name))
}

/// Locks each file of `folder` on its own sidecar, in sidecar path order.
pub(super) fn lock_all<F: Flock + ?Sized>(
    flock: &F,
    wait: &Waiter<'_>,
    held: &mut Held,
    folder: &Folder,
) -> errs::Result<()> {
    let mut files: Vec<&(PathBuf, Mode)> = folder.files.iter().collect();
    files.sort_by_key(|(file, _)| path(file).into_os_string().into_vec());
    for entry in files {
        lock_one(flock, wait, held, entry)?;
    }
    Ok(())
}

fn lock_one<F: Flock + ?Sized>(
    flock: &F,
    wait: &Waiter<'_>,
    held: &mut Held,
    entry: &(PathBuf, Mode),
) -> errs::Result<()> {
    let (file, mode) = entry;
    let side = path(file);
    let handle = open(&side)
        .map_err(|e| errs::io_at("lock", &side, "open", &side, e))?;
    wait.lock(flock, &handle, *mode).map_err(|f| {
        f.into_error(&side, &errs::lossy_path(base_name(file)))
    })?;
    held.locks.push((handle, side));
    Ok(())
}

/// Opens the sidecar read-only, creating it first when it is missing, so
/// a sidecar the user made read-only still locks.
fn open(side: &Path) -> io::Result<File> {
    let file = File::open(side).or_else(|_| {
        create(side)?;
        File::open(side)
    })?;
    if file.metadata()?.is_dir() {
        return Err(io::Error::from(Errno::ISDIR));
    }
    Ok(file)
}

/// Makes the sidecar unless it exists.
pub(super) fn create(side: &Path) -> io::Result<()> {
    let made = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(SIDECAR_MODE)
        .open(side);
    match made {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

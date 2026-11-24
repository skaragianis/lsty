use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Write},
    os::{
        fd::{AsFd, BorrowedFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::{Path, PathBuf},
};

use rustix::{
    fs::{
        AtFlags, CWD, FileType, Mode, OFlags, fchmod, openat, renameat,
        statat, unlinkat,
    },
    io::Errno,
};

use super::{Hooks, base_name, folder_of, read::MAX_LEN};
use crate::{errs, errs::io_at};

/// The mode of a new file before the umask.
const NEW_FILE_MODE: Mode = Mode::from_bits_truncate(0o666);

/// Replaces `path` with `data` through a synced temp file in the same
/// folder, a rename and a folder sync. `path` is a resolved path: its
/// folder is opened once, and every later step names a file of that
/// folder, never a path. A failure before the rename removes the temp file
/// and leaves the target as it was.
pub fn write<H: Hooks + ?Sized>(
    path: &Path,
    data: &[u8],
    hooks: &H,
) -> errs::Result<()> {
    write_within(path, data, hooks, MAX_LEN)
}

/// `write`, refusing data over `max` bytes.
fn write_within<H: Hooks + ?Sized>(
    path: &Path,
    data: &[u8],
    hooks: &H,
    max: u64,
) -> errs::Result<()> {
    let folder = open_folder(path)?;
    let at = on_disk(folder.as_fd(), path)?;
    write_in_within(at, data, hooks, max)
}

/// The file `path` in the open folder `dir`, with the mode it has now.
fn on_disk<'a>(
    dir: BorrowedFd<'a>,
    path: &'a Path,
) -> errs::Result<Target<'a>> {
    Ok((dir, path, target_mode(dir, base_name(path), path)?))
}

/// Writes each of `files` (a resolved path and its data) in order, as
/// `write` does, but opens each folder once and writes every file of it
/// through that handle, so a folder renamed between two writes cannot
/// take the later one.
pub(crate) fn write_all<H: Hooks + ?Sized>(
    files: &[(&Path, &[u8])],
    hooks: &H,
) -> errs::Result<()> {
    let mut open: Vec<(&Path, OwnedFd)> = Vec::new();
    let mut slots = Vec::new();
    for &(path, _) in files {
        let dir = folder_of(path);
        let slot = if let Some(at) = open.iter().position(|(d, _)| *d == dir) {
            at
        } else {
            open.push((dir, open_folder(path)?));
            open.len() - 1
        };
        slots.push(slot);
    }
    for (&(path, data), slot) in files.iter().zip(slots) {
        let at = on_disk(open[slot].1.as_fd(), path)?;
        write_in(at, data, hooks)?;
    }
    Ok(())
}

fn open_folder(path: &Path) -> errs::Result<OwnedFd> {
    let dir = folder_of(path);
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    openat(CWD, dir, flags, Mode::empty())
        .map_err(|e| io_at("write", path, "open", dir, e.into()))
}

/// A file to write: the open folder it is in, the path that stands for it
/// in hooks and errors, and its permission bits, none when it is new.
pub type Target<'a> = (BorrowedFd<'a>, &'a Path, Option<Mode>);

/// `write` for `target`, with the mode its caller read it with. Nothing
/// here follows a path, so a link put at its name since is replaced, not
/// written through.
pub fn write_in<H: Hooks + ?Sized>(
    target: Target<'_>,
    data: &[u8],
    hooks: &H,
) -> errs::Result<()> {
    write_in_within(target, data, hooks, MAX_LEN)
}

fn write_in_within<H: Hooks + ?Sized>(
    target: Target<'_>,
    data: &[u8],
    hooks: &H,
    max: u64,
) -> errs::Result<()> {
    let (dir, shown, _) = target;
    let name = base_name(shown);
    if data.len() as u64 > max {
        let err = io::ErrorKind::FileTooLarge.into();
        return Err(io_at("write", shown, "write", shown, err));
    }
    write_temp(target, data, hooks)?;
    hooks.before_rename(shown)?;
    let temp = temp_name(name);
    if let Err(e) = renameat(dir, &temp, dir, name) {
        remove_quietly(dir, &temp);
        let cause = rename_cause(e.into());
        return Err(errs::io_rename(&temp_path(shown), shown, cause));
    }
    hooks.after_rename(shown)?;
    sync_folder(dir, shown, hooks)?;
    hooks.after_dir_sync(shown)?;
    Ok(())
}

/// Renaming a file over a folder is `file exists`, whatever the system
/// says.
fn rename_cause(err: io::Error) -> io::Error {
    if err.kind() == io::ErrorKind::IsADirectory {
        return io::Error::from(Errno::EXIST);
    }
    err
}

fn sync_folder<H: Hooks + ?Sized>(
    dir: BorrowedFd<'_>,
    shown: &Path,
    hooks: &H,
) -> errs::Result<()> {
    let at = folder_of(shown);
    let fail = |op| move |e| io_at("sync", at, op, at, e);
    let folder = dir.try_clone_to_owned().map_err(fail("open"))?;
    hooks.sync_dir(&File::from(folder)).map_err(fail("sync"))
}

/// `.<file>.lsty-<pid>.tmp`.
fn temp_name(name: &OsStr) -> OsString {
    let mut temp = OsString::from(".");
    temp.push(name);
    temp.push(format!(".lsty-{}.tmp", std::process::id()));
    temp
}

/// The temp file of `path`, in its folder.
fn temp_path(path: &Path) -> PathBuf {
    path.with_file_name(temp_name(base_name(path)))
}

fn remove_quietly(dir: BorrowedFd<'_>, temp: &OsStr) {
    // The temp file may already be gone; nothing more can be done.
    unlinkat(dir, temp, AtFlags::empty()).unwrap_or_default();
}

fn write_temp<H: Hooks + ?Sized>(
    target: Target<'_>,
    data: &[u8],
    hooks: &H,
) -> errs::Result<()> {
    let (dir, shown, _) = target;
    fill_temp(target, data, hooks).inspect_err(|_| {
        remove_quietly(dir, &temp_name(base_name(shown)));
    })
}

fn fill_temp<H: Hooks + ?Sized>(
    target: Target<'_>,
    data: &[u8],
    hooks: &H,
) -> errs::Result<()> {
    let shown = target.1;
    let temp = temp_path(shown);
    let at = |op: &'static str| |e| io_at("write", shown, op, &temp, e);
    let mut file = create_temp(target)?;
    file.write_all(data).map_err(at("write"))?;
    hooks.sync_file(&file).map_err(at("sync"))?;
    // The file is closed by dropping it. std cannot report a failed close,
    // and the data is already synced, so nothing is lost by not seeing it.
    Ok(())
}

fn create_temp(target: Target<'_>) -> errs::Result<File> {
    let (dir, shown, mode) = target;
    let name = base_name(shown);
    let (temp, at) = (temp_name(name), temp_path(shown));
    let fail = |op, e: Errno| io_at("write", shown, op, &at, e.into());
    match unlinkat(dir, &temp, AtFlags::empty()) {
        Err(e) if e != Errno::NOENT => return Err(fail("remove", e)),
        _ => {}
    }
    let flags = OFlags::WRONLY
        | OFlags::CREATE
        | OFlags::EXCL
        | OFlags::NOFOLLOW
        | OFlags::CLOEXEC;
    let open = mode.unwrap_or(NEW_FILE_MODE);
    let file = File::from(
        openat(dir, &temp, flags, open).map_err(|e| fail("open", e))?,
    );
    if let Some(mode) = mode {
        fchmod(&file, mode).map_err(|e| fail("chmod", e))?;
    }
    Ok(file)
}

/// The permission bits of the file `name`, or `None` when there is none
/// or it is not a regular file (a link there is replaced, so its own
/// mode does not count).
fn target_mode(
    dir: BorrowedFd<'_>,
    name: &OsStr,
    shown: &Path,
) -> errs::Result<Option<Mode>> {
    match statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) => Ok(FileType::from_raw_mode(st.st_mode)
            .is_file()
            .then(|| Mode::from_bits_truncate(st.st_mode & 0o777))),
        Err(Errno::NOENT) => Ok(None),
        Err(e) => Err(io_at("write", shown, "stat", shown, e.into())),
    }
}

/// Removes the temp files of `path` left by a crash. The caller holds the
/// exclusive lock.
pub(crate) fn clean_orphans(path: &Path) -> errs::Result<()> {
    let dir = folder_of(path);
    let base = base_name(path);
    let open = |e| io_at("read", dir, "open", dir, e);
    let next = |e| io_at("read", dir, "readdirent", dir, e);
    for entry in fs::read_dir(dir).map_err(open)? {
        let entry = entry.map_err(next)?;
        if entry.file_type().map_err(next)?.is_dir()
            || !is_orphan_name(entry.file_name().as_bytes(), base.as_bytes())
        {
            continue;
        }
        let full = entry.path();
        fs::remove_file(&full)
            .map_err(|e| io_at("remove", &full, "remove", &full, e))?;
    }
    Ok(())
}

/// Whether `name` is `.<base>.lsty-<digits>.tmp`.
fn is_orphan_name(name: &[u8], base: &[u8]) -> bool {
    let Some(rest) = name.strip_prefix(b".") else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(base) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(b".lsty-") else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(b".tmp") else {
        return false;
    };
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod write_tests;

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

use rustix::io::Errno;

use super::{Hooks, base_name, folder_of, read::MAX_LEN};
use crate::{errs, errs::io_at};

/// The mode of a new file before the umask.
const NEW_FILE_MODE: u32 = 0o666;

/// Replaces `path` with `data` through a synced temp file in the same
/// folder, a rename and a folder sync. `path` is a resolved path. A failure
/// before the rename removes the temp file and leaves the target as it was.
pub fn write<H: Hooks + ?Sized>(
    path: &Path,
    data: &[u8],
    hooks: &H,
) -> errs::Result<()> {
    if data.len() as u64 > MAX_LEN {
        let err = io::ErrorKind::FileTooLarge.into();
        return Err(io_at("write", path, "write", path, err));
    }
    let temp = write_temp(path, data, hooks)?;
    hooks.before_rename(path)?;
    if let Err(e) = fs::rename(&temp, path) {
        remove_quietly(&temp);
        return Err(errs::io_rename(&temp, path, rename_cause(e)));
    }
    hooks.after_rename(path)?;
    sync_folder(path, hooks)?;
    hooks.after_dir_sync(path)?;
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

fn sync_folder<H: Hooks + ?Sized>(path: &Path, hooks: &H) -> errs::Result<()> {
    let dir = folder_of(path);
    let folder =
        File::open(dir).map_err(|e| io_at("sync", dir, "open", dir, e))?;
    hooks
        .sync_dir(&folder)
        .map_err(|e| io_at("sync", dir, "sync", dir, e))
}

/// `.<file>.lsty-<pid>.tmp` in the folder of `path`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(base_name(path));
    name.push(format!(".lsty-{}.tmp", std::process::id()));
    path.with_file_name(name)
}

fn remove_quietly(path: &Path) {
    // The temp file may already be gone; nothing more can be done.
    drop(fs::remove_file(path));
}

fn write_temp<H: Hooks + ?Sized>(
    path: &Path,
    data: &[u8],
    hooks: &H,
) -> errs::Result<PathBuf> {
    let temp = temp_path(path);
    match fill_temp(path, &temp, data, hooks) {
        Ok(()) => Ok(temp),
        Err(e) => {
            remove_quietly(&temp);
            Err(e)
        }
    }
}

fn fill_temp<H: Hooks + ?Sized>(
    path: &Path,
    temp: &Path,
    data: &[u8],
    hooks: &H,
) -> errs::Result<()> {
    let at = |op: &'static str| move |e| io_at("write", path, op, temp, e);
    let mut file = create_temp(path, temp)?;
    file.write_all(data).map_err(at("write"))?;
    hooks.sync_file(&file).map_err(at("sync"))?;
    // The file is closed by dropping it. std cannot report a failed close,
    // and the data is already synced, so nothing is lost by not seeing it.
    Ok(())
}

fn create_temp(path: &Path, temp: &Path) -> errs::Result<File> {
    match fs::remove_file(temp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            return Err(io_at("write", path, "remove", temp, e));
        }
        _ => {}
    }
    let mode = target_mode(path)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(NEW_FILE_MODE)
        .open(temp)
        .map_err(|e| io_at("write", path, "open", temp, e))?;
    if let Some(mode) = mode {
        file.set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|e| io_at("write", path, "chmod", temp, e))?;
    }
    Ok(file)
}

/// The permission bits of `path`, or `None` when it does not exist.
fn target_mode(path: &Path) -> errs::Result<Option<u32>> {
    match fs::metadata(path) {
        Ok(meta) => Ok(Some(meta.permissions().mode() & 0o777)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_at("write", path, "stat", path, e)),
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

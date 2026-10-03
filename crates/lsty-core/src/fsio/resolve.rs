use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use rustix::io::Errno;

use super::{NoDir, io_err};
use crate::errs;

const MAX_LINK_DEPTH: usize = 255;

/// Makes `path` absolute against `cwd` and follows symlinks. A missing
/// file keeps its name inside the resolved folder; a dangling link resolves
/// to its target. A folder that does not exist, or is not a folder, is an
/// `io` error that [`is_no_dir`](super::is_no_dir) recognises. `cwd` must
/// be absolute, or a relative `path` resolves to a relative result.
pub fn resolve(cwd: &Path, path: &Path) -> errs::Result<PathBuf> {
    resolve_depth(cwd, path, 0)
}

fn resolve_depth(
    cwd: &Path,
    path: &Path,
    depth: usize,
) -> errs::Result<PathBuf> {
    let abs = clean(&cwd.join(path));
    match fs::canonicalize(&abs) {
        Ok(found) => Ok(found),
        Err(e) if e.kind() == io::ErrorKind::NotADirectory => {
            resolve_not_dir(cwd, &abs, depth)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            resolve_missing(cwd, &abs, depth)
        }
        Err(e) => Err(resolve_err(path, e)),
    }
}

/// The `io` error of a failed lookup in `path`; a link loop reads
/// `EvalSymlinks: too many links`.
fn resolve_err(path: &Path, err: io::Error) -> errs::Error {
    if err.raw_os_error() == Some(Errno::LOOP.raw_os_error()) {
        let message = format!(
            "resolve {}: EvalSymlinks: too many links",
            path.display()
        );
        return errs::Error::new(errs::Code::Io, message);
    }
    io_err("resolve", path, err)
}

fn resolve_not_dir(
    cwd: &Path,
    abs: &Path,
    depth: usize,
) -> errs::Result<PathBuf> {
    match fs::read_link(abs) {
        Ok(target) => resolve_link(cwd, abs, &target, depth),
        Err(_) => Err(no_dir(abs.parent().unwrap_or(abs))),
    }
}

fn resolve_missing(
    cwd: &Path,
    abs: &Path,
    depth: usize,
) -> errs::Result<PathBuf> {
    if let Ok(target) = fs::read_link(abs) {
        return resolve_link(cwd, abs, &target, depth);
    }
    let (Some(parent), Some(name)) = (abs.parent(), abs.file_name()) else {
        return Err(no_dir(abs));
    };
    match fs::canonicalize(parent) {
        Ok(dir) => Ok(dir.join(name)),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Err(no_dir(parent))
        }
        Err(e) => Err(resolve_err(abs, e)),
    }
}

fn resolve_link(
    cwd: &Path,
    link: &Path,
    target: &Path,
    depth: usize,
) -> errs::Result<PathBuf> {
    if depth >= MAX_LINK_DEPTH {
        let err = io::Error::other("too many links");
        return Err(io_err("resolve", link, err));
    }
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        let parent = link.parent().unwrap_or(link);
        let dir =
            fs::canonicalize(parent).map_err(|e| resolve_err(link, e))?;
        dir.join(target)
    };
    resolve_depth(cwd, &target, depth + 1)
}

fn no_dir(dir: &Path) -> errs::Error {
    errs::Error::from(io::Error::other(NoDir(dir.to_path_buf())))
}

/// `path` with `.` and `..` removed lexically.
fn clean(path: &Path) -> PathBuf {
    let mut parts: Vec<OsString> = Vec::new();
    let mut root = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                root.push(c.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(n) => parts.push(n.to_os_string()),
        }
    }
    root.extend(parts);
    root
}

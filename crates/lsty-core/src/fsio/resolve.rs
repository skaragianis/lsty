use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use rustix::io::Errno;

use super::{NoDir, clean};
use crate::errs;

const MAX_LINK_DEPTH: usize = 255;

/// Follows the symlinks of the absolute `path`. A missing file keeps its
/// name inside the resolved folder; a dangling link resolves to its
/// target. A folder that does not exist, or is not a folder, is an `io`
/// error.
pub(crate) fn resolve(path: &Path) -> errs::Result<PathBuf> {
    resolve_depth(&clean(path), 0)
}

fn resolve_depth(abs: &Path, depth: usize) -> errs::Result<PathBuf> {
    match fs::canonicalize(abs) {
        Ok(found) => Ok(found),
        Err(e) if e.kind() == io::ErrorKind::NotADirectory => {
            resolve_not_dir(abs, depth)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            resolve_missing(abs, depth)
        }
        Err(e) => Err(resolve_err(abs, abs, e)),
    }
}

/// The `io` error of a failed lookup of `walk`, reported for `outer`: a
/// link loop reads `EvalSymlinks: too many links`, anything else names the
/// first prefix of `walk` that cannot be examined, as
/// `resolve P: lstat Q: permission denied`.
fn resolve_err(outer: &Path, walk: &Path, err: io::Error) -> errs::Error {
    if Errno::from_io_error(&err) == Some(Errno::LOOP) {
        let message = format!(
            "resolve {}: EvalSymlinks: too many links",
            errs::lossy_path(outer)
        );
        return errs::Error::new(errs::Code::Io, message);
    }
    match failing_prefix(walk) {
        Some((prefix, cause)) => {
            errs::io_at("resolve", outer, "lstat", &prefix, cause)
        }
        None => errs::io("resolve", outer, err),
    }
}

/// The first prefix of `abs` whose `lstat` fails, with the error. A link
/// on the way is followed, so the prefix may lie in its target.
fn failing_prefix(abs: &Path) -> Option<(PathBuf, io::Error)> {
    let mut path = abs.to_path_buf();
    'walk: for _ in 0..MAX_LINK_DEPTH {
        let parts: Vec<Component<'_>> = path.components().collect();
        let mut prefix = PathBuf::new();
        for (i, part) in parts.iter().enumerate() {
            prefix.push(part);
            let link = match fs::symlink_metadata(&prefix) {
                Err(e) => return Some((prefix, e)),
                Ok(meta) => meta.is_symlink(),
            };
            if link && let Some(next) = through_link(&prefix, &parts[i + 1..])
            {
                path = next;
                continue 'walk;
            }
        }
        return None;
    }
    None
}

/// The path that replaces the link `link` followed by `rest`.
fn through_link(link: &Path, rest: &[Component<'_>]) -> Option<PathBuf> {
    let target = fs::read_link(link).ok()?;
    let mut next = link.parent().unwrap_or(link).join(target);
    next.extend(rest);
    Some(next)
}

fn resolve_not_dir(abs: &Path, depth: usize) -> errs::Result<PathBuf> {
    match fs::read_link(abs) {
        Ok(target) => resolve_link(abs, &target, depth),
        Err(_) => Err(no_dir(abs.parent().unwrap_or(abs))),
    }
}

fn resolve_missing(abs: &Path, depth: usize) -> errs::Result<PathBuf> {
    if let Ok(target) = fs::read_link(abs) {
        return resolve_link(abs, &target, depth);
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
        Err(e) => Err(resolve_err(abs, abs, e)),
    }
}

fn resolve_link(
    link: &Path,
    target: &Path,
    depth: usize,
) -> errs::Result<PathBuf> {
    if depth >= MAX_LINK_DEPTH {
        let err = io::Error::other("too many links");
        return Err(errs::io("resolve", link, err));
    }
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        let parent = link.parent().unwrap_or(link);
        let dir = fs::canonicalize(parent)
            .map_err(|e| resolve_err(link, link, e))?;
        dir.join(target)
    };
    resolve_depth(&target, depth + 1)
}

fn no_dir(dir: &Path) -> errs::Error {
    errs::Error::from(io::Error::other(NoDir(dir.to_path_buf())))
}

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::TODO_NAME;
use crate::fsio;

/// The nearest folder from `cwd` upwards that holds an entry named `.git`.
pub(super) fn git_root(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|dir| fs::symlink_metadata(dir.join(".git")).is_ok())
        .map(Path::to_path_buf)
}

/// The nearest `todo.txt` from the clean `cwd` upwards. The walk ends after
/// the first folder that is the git `root`, `home` or `/`.
pub(super) fn walk_up(
    cwd: &Path,
    root: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let home = home.map(fsio::clean);
    for dir in cwd.ancestors() {
        let candidate = dir.join(TODO_NAME);
        if is_regular(&candidate) {
            return Some(candidate);
        }
        if root == Some(dir) || home.as_deref() == Some(dir) {
            return None;
        }
    }
    None
}

/// A regular file, or a symlink to one.
pub(super) fn is_regular(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}

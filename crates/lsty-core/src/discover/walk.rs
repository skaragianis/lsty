use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::fsio;

/// The nearest folder from `cwd` upwards that holds an entry named `.git`.
pub(super) fn git_root(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|dir| fs::symlink_metadata(dir.join(".git")).is_ok())
        .map(Path::to_path_buf)
}

/// The folders from the clean `cwd` upwards, ending with the first that is
/// the git `root`, `home` (as named or with its links followed) or `/`.
pub(super) fn folders<'a>(
    cwd: &'a Path,
    root: Option<&Path>,
    home: Option<&Path>,
) -> impl Iterator<Item = &'a Path> {
    let homes: Vec<PathBuf> = home
        .into_iter()
        .flat_map(|h| [Some(fsio::clean(h)), fs::canonicalize(h).ok()])
        .flatten()
        .collect();
    let mut ended = false;
    cwd.ancestors().take_while(move |dir| {
        let before = ended;
        ended = root == Some(*dir) || homes.iter().any(|h| h == dir);
        !before
    })
}

/// A regular file, or a symlink to one.
pub(super) fn is_regular(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}

//! The files an item links to with `file:`, as paths on disk.

use std::fs;
use std::path::{Path, PathBuf};

use lsty_core::list::{Item, Snapshot};

/// One `file:` link of an item.
pub(crate) struct LinkedFile<'a> {
    /// The link as written.
    pub raw: &'a [u8],
    /// The path it names: relative to the folder of the todo file, or
    /// under the home folder for `~/`.
    pub path: PathBuf,
    /// Whether something is there.
    pub exists: bool,
}

/// The files `item` links to, in the order of its line. `home` is the
/// folder `~/` stands for.
pub(crate) fn linked_files<'a>(
    snap: &'a Snapshot,
    item: &Item,
    home: Option<&Path>,
) -> Vec<LinkedFile<'a>> {
    let todo = &snap.found().todo;
    let dir = todo.parent().unwrap_or_else(|| Path::new(""));
    let home = home.unwrap_or_else(|| Path::new(""));
    snap.line(item)
        .tags()
        .files
        .iter()
        .map(|link| {
            let path = link.resolve(dir, home);
            let exists = fs::metadata(&path).is_ok();
            LinkedFile {
                raw: link.raw,
                path,
                exists,
            }
        })
        .collect()
}

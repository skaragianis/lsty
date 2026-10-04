//! Finding a program in a search path.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

/// The permission bits that let anyone run a file.
const EXEC_BITS: u32 = 0o111;

/// The first executable file called `name` in a folder of `path`, a list
/// of folders separated as `PATH` is. Entries that are not absolute
/// (empty ones, `.`, `bin`) are skipped, so nothing runs from the
/// current folder.
pub fn find_in_path(path: &OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|file| {
            std::fs::metadata(file).is_ok_and(|m| {
                m.is_file() && m.permissions().mode() & EXEC_BITS != 0
            })
        })
}

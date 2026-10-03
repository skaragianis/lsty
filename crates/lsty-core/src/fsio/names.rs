//! Splitting a path into its folder and its file name.

use std::ffi::OsString;
use std::path::Path;

/// The folder of `path`; a bare file name lives in `.`.
pub(crate) fn folder_of(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if dir.as_os_str().is_empty() => Path::new("."),
        Some(dir) => dir,
        None => path,
    }
}

/// The last component of `path`, or the whole path when it has none.
pub(crate) fn base_name(path: &Path) -> OsString {
    path.file_name().unwrap_or(path.as_os_str()).to_os_string()
}

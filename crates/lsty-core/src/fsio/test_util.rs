//! Helpers shared by the tests of this module.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A temp folder with its symlinks resolved.
pub(super) fn dir() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(tmp.path()).expect("canonicalize");
    (tmp, real)
}

pub(super) fn put(path: &Path, content: &str, mode: u32) {
    fs::write(path, content).expect("write");
    chmod(path, mode);
}

pub(super) fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .expect("chmod");
}

pub(super) fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("stat").permissions().mode() & 0o777
}

pub(super) fn text(path: &Path) -> String {
    String::from_utf8(fs::read(path).expect("read")).expect("utf8")
}

/// Names of the `.lsty-` temp files in `dir`.
pub(super) fn temps(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".lsty-"))
        .collect();
    found.sort();
    found
}

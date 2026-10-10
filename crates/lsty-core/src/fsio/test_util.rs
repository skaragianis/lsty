//! Helpers shared by the tests of this module.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use tempfile::TempDir;

/// A temp folder with its symlinks resolved.
pub(crate) fn dir() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(tmp.path()).expect("canonicalize");
    (tmp, real)
}

pub(crate) fn put(path: &Path, content: &str, mode: u32) {
    fs::write(path, content).expect("write");
    chmod(path, mode);
}

pub(crate) fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .expect("chmod");
}

pub(crate) fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("stat").permissions().mode() & 0o777
}

pub(crate) fn text(path: &Path) -> String {
    String::from_utf8(fs::read(path).expect("read")).expect("utf8")
}

/// Names of the `.lsty-` temp files in `dir`.
pub(crate) fn temps(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".lsty-"))
        .collect();
    found.sort();
    found
}

/// A folder whose mode is 000 until the guard drops, so the temp folder
/// can be removed even when an assertion fails.
pub(crate) struct Locked(PathBuf);

impl Locked {
    pub(crate) fn new(path: &Path) -> Self {
        chmod(path, 0o000);
        Self(path.to_path_buf())
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        chmod(&self.0, 0o755);
    }
}

/// Whether the process ignores folder modes.
pub(crate) fn is_root() -> bool {
    rustix::process::geteuid().is_root()
}

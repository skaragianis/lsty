//! Grouping wants by folder, in lock order.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use super::{Mode, Want};
use crate::fsio::{base_name, folder_of};

/// The wants that fall in one folder.
pub(super) struct Folder {
    pub(super) dir: PathBuf,
    /// The base name of the first file asked for, named when the folder
    /// is busy.
    first: OsString,
    /// Each file once, with the strongest mode asked for it.
    pub(super) files: Vec<(PathBuf, Mode)>,
}

impl Folder {
    /// Exclusive when any file wants it so.
    pub(super) fn mode(&self) -> Mode {
        self.files
            .iter()
            .map(|(_, mode)| *mode)
            .max()
            .unwrap_or(Mode::Shared)
    }

    pub(super) fn first_name(&self) -> String {
        crate::errs::lossy_path(&self.first)
    }
}

/// The folders of `wants`, sorted by their path byte by byte.
pub(super) fn group(wants: &[Want]) -> Vec<Folder> {
    let mut by_dir: BTreeMap<Vec<u8>, Folder> = BTreeMap::new();
    for want in wants {
        let dir = folder_of(&want.file);
        let folder = by_dir
            .entry(dir.as_os_str().as_bytes().to_vec())
            .or_insert_with(|| Folder {
                dir: dir.to_path_buf(),
                first: base_name(&want.file),
                files: Vec::new(),
            });
        add(folder, want);
    }
    by_dir.into_values().collect()
}

fn add(folder: &mut Folder, want: &Want) {
    match folder.files.iter_mut().find(|(f, _)| *f == want.file) {
        Some((_, mode)) => *mode = (*mode).max(want.mode),
        None => folder.files.push((want.file.clone(), want.mode)),
    }
}

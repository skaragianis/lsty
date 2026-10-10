use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{errs::Vars, fsio};

/// Where the files and folders lsty reads and writes are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// The config file.
    pub config_file: PathBuf,
    /// `keys.toml`, next to the config folder's other files.
    pub keys_file: PathBuf,
    /// The folder of theme files.
    pub themes_dir: PathBuf,
    /// The folder of state files.
    pub state_dir: PathBuf,
    /// Whether `LSTY_CONFIG` named `config_file`, so that a missing file
    /// is worth a warning.
    pub explicit: bool,
}

/// Works out the paths from the variables and the home folder.
///
/// A relative `LSTY_CONFIG` stays relative: it is taken from the working
/// folder when the file is read.
pub fn find_paths(vars: &Vars<'_>, home: Option<&Path>) -> Paths {
    let home = home.unwrap_or_else(|| Path::new(""));
    let dir = xdg_dir(vars("XDG_CONFIG_HOME"), &home.join(".config"));
    let state =
        xdg_dir(vars("XDG_STATE_HOME"), &home.join(".local").join("state"));
    let mut paths = Paths {
        config_file: dir.join("config.toml"),
        keys_file: dir.join("keys.toml"),
        themes_dir: dir.join("themes"),
        state_dir: state,
        explicit: false,
    };
    if let Some(file) = vars("LSTY_CONFIG").filter(|v| !v.is_empty()) {
        paths.config_file = PathBuf::from(file);
        paths.explicit = true;
    }
    paths
}

/// `lsty` in the variable's folder when it is absolute, else in `fallback`.
fn xdg_dir(value: Option<OsString>, fallback: &Path) -> PathBuf {
    let base = value
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| fallback.to_path_buf());
    fsio::clean(&base.join("lsty"))
}

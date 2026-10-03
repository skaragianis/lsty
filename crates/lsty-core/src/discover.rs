//! Finding the todo file and its done file.
//!
//! One rule serves the CLI and the TUI: an explicit path, `TODO_FILE`, a
//! walk up from the current folder, `TODO_DIR`, the configured default
//! file, then a fallback where a new list belongs. The result says which
//! step chose the path, so that a missing file can be reported precisely.
//! Nothing here creates a file or reads the process: the caller passes the
//! variables, the current folder and the home folder.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::errs::{self, Vars};
use crate::fsio;

mod done;
mod walk;

#[cfg(test)]
mod tests;

const TODO_NAME: &str = "todo.txt";

/// What [`find`] needs from the caller.
pub struct Input<'a> {
    /// `--file PATH`, `lsty FILE` or `lsty tui FILE`; `None` if there is
    /// none.
    pub explicit: Option<&'a Path>,
    /// The environment variables.
    pub vars: &'a Vars<'a>,
    /// The current folder, absolute.
    pub cwd: &'a Path,
    /// The home folder.
    pub home: Option<&'a Path>,
    /// `default_file` of the config; `None` if unset.
    pub default_file: Option<&'a str>,
}

/// The step that chose the todo file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoundBy {
    /// An explicit path.
    File,
    /// `$TODO_FILE`.
    TodoFile,
    /// A `todo.txt` on the way up from the current folder.
    WalkUp,
    /// `$TODO_DIR/todo.txt`.
    TodoDir,
    /// `default_file` of the config.
    DefaultFile,
    /// `todo.txt` where a new list belongs.
    Fallback,
}

impl FoundBy {
    /// The name reported to the user: `--file`, `TODO_FILE`, `walk-up`,
    /// `TODO_DIR`, `default_file` or `fallback`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "--file",
            Self::TodoFile => "TODO_FILE",
            Self::WalkUp => "walk-up",
            Self::TodoDir => "TODO_DIR",
            Self::DefaultFile => "default_file",
            Self::Fallback => "fallback",
        }
    }
}

impl fmt::Display for FoundBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The todo and done files and the step that chose them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The todo file: absolute, resolved.
    pub todo: PathBuf,
    /// The done file: absolute, resolved.
    pub done: PathBuf,
    /// The step that chose `todo`.
    pub found_by: FoundBy,
    /// Whether the todo file is there.
    pub state: FileState,
}

/// Whether the todo file is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    /// The todo file exists.
    Exists,
    /// The todo file does not exist, but its folder does.
    Missing,
    /// The folder of the todo file does not exist or is a file. `todo` is
    /// then cleaned but follows no symlink.
    DirMissing,
}

impl Input<'_> {
    fn var(&self, name: &str) -> Option<String> {
        (self.vars)(name).filter(|v| !v.is_empty())
    }

    /// `path` made absolute against the current folder and cleaned.
    fn abs(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            fsio::clean(path)
        } else {
            fsio::clean(&self.cwd.join(path))
        }
    }

    /// `~` and `~/` at the start of `path` mean the home folder.
    fn expand_home(&self, path: &str) -> PathBuf {
        let Some(rest) = path.strip_prefix('~') else {
            return PathBuf::from(path);
        };
        if !(rest.is_empty() || rest.starts_with('/')) {
            return PathBuf::from(path);
        }
        let mut full = self
            .home
            .unwrap_or_else(|| Path::new(""))
            .as_os_str()
            .to_owned();
        full.push(rest);
        PathBuf::from(full)
    }

    fn fallback_dir(&self) -> PathBuf {
        walk::git_root(self.cwd).unwrap_or_else(|| self.cwd.to_path_buf())
    }

    fn choose(&self) -> (PathBuf, FoundBy) {
        if let Some(path) = self.explicit {
            return (path.to_path_buf(), FoundBy::File);
        }
        if let Some(path) = self.var("TODO_FILE") {
            return (PathBuf::from(path), FoundBy::TodoFile);
        }
        if let Some(path) = walk::walk_up(self.cwd, self.home) {
            return (path, FoundBy::WalkUp);
        }
        if let Some(dir) = self.var("TODO_DIR") {
            return (Path::new(&dir).join(TODO_NAME), FoundBy::TodoDir);
        }
        if let Some(path) = self.default_file {
            return (self.expand_home(path), FoundBy::DefaultFile);
        }
        (self.fallback_dir().join(TODO_NAME), FoundBy::Fallback)
    }
}

/// Chooses the todo file and derives the done file. Creates nothing.
///
/// # Errors
///
/// An `io` error when the todo file names a folder or cannot be examined,
/// and `invalid_value` when `DONE_FILE` is the todo file.
pub fn find(input: &Input<'_>) -> errs::Result<Found> {
    let (path, found_by) = input.choose();
    let (todo, dir_missing) = resolve(input, &input.abs(&path))?;
    let state = if dir_missing {
        FileState::DirMissing
    } else if regular_file(&todo)? {
        FileState::Exists
    } else {
        FileState::Missing
    };
    let done = done::done_file(input, &todo, found_by)?;
    Ok(Found {
        todo,
        done,
        found_by,
        state,
    })
}

/// Resolves `path`; a path whose folder is missing stays as it is.
fn resolve(input: &Input<'_>, path: &Path) -> errs::Result<(PathBuf, bool)> {
    match fsio::resolve(input.cwd, path) {
        Ok(resolved) => Ok((resolved, false)),
        Err(e) if fsio::is_no_dir(&e) => Ok((path.to_path_buf(), true)),
        Err(e) => Err(e),
    }
}

fn regular_file(path: &Path) -> errs::Result<bool> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => Err(errs::Error::new(
            errs::Code::Io,
            format!("{} is a folder", path.display()),
        )),
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(errs::io_at("stat", path, "stat", path, e)),
    }
}

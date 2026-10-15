//! Finding the todo file and its done file.
//!
//! One rule serves the CLI and the TUI: an explicit path, `TODO_FILE`, a
//! walk up from the current folder, `TODO_DIR`, the configured default
//! file, then a fallback where a new list belongs. The result says which
//! step chose the path, so that a missing file can be reported precisely.
//! Nothing here creates a file or reads the process: the caller passes the
//! variables, the current folder and the home folder.

use std::{
    ffi::OsString,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::{Vars, errs, fsio, todosh::Todosh};

mod done;

pub use done::report_file;
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
    /// What todo.sh's config file sets; `None` if it is not read.
    pub todosh: Option<&'a Todosh>,
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
    pub(crate) const fn as_str(self) -> &'static str {
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
    /// `REPORT_FILE`, absolute and not examined, when it counts. See
    /// [`report_file`].
    pub report_file: Option<PathBuf>,
    /// The step that chose `todo`.
    pub found_by: FoundBy,
    /// Whether the todo file is there.
    pub state: FileState,
}

impl Found {
    /// The error for a todo file that is not there. It names the step that
    /// chose the path; advice is left to the caller.
    pub fn no_file(&self) -> errs::Error {
        let found_by = self.found_by;
        let message = format!(
            "no todo file at {} (chosen by {found_by})",
            errs::lossy_path(&self.todo)
        );
        errs::Error::new(errs::Code::NoFile, message).with_field(
            "found_by",
            errs::Field::Str(found_by.as_str().to_owned()),
        )
    }

    /// The path of the file of `source`.
    pub(crate) fn path(&self, source: errs::Source) -> &Path {
        source.pick(&self.todo, &self.done)
    }
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
    fn var(&self, name: &str) -> Option<OsString> {
        (self.vars)(name).filter(|v| !v.is_empty())
    }

    /// What the config file of todo.sh sets `name` to.
    fn file_var(&self, name: &str) -> Option<OsString> {
        let (text, _) = self.todosh?.get(name)?;
        Some(text).filter(|t| !t.is_empty()).map(OsString::from)
    }

    /// `name` from the environment, else from the config file.
    fn setting(&self, name: &str) -> Option<OsString> {
        self.var(name).or_else(|| self.file_var(name))
    }

    /// `path` made absolute against the current folder and cleaned.
    fn abs(&self, path: &Path) -> PathBuf {
        fsio::clean(&self.cwd.join(path))
    }

    /// `~` and `~/` at the start of `path` mean the home folder.
    fn expand_home(&self, path: &str) -> PathBuf {
        let Some(rest) = path
            .strip_prefix('~')
            .filter(|r| r.is_empty() || r.starts_with('/'))
        else {
            return PathBuf::from(path);
        };
        let mut full = self
            .home
            .map(Path::as_os_str)
            .unwrap_or_default()
            .to_owned();
        full.push(rest);
        PathBuf::from(full)
    }

    fn choose(&self) -> (PathBuf, FoundBy) {
        if let Some(path) = self.explicit {
            return (path.to_path_buf(), FoundBy::File);
        }
        if let Some(path) = self.var("TODO_FILE") {
            return (PathBuf::from(path), FoundBy::TodoFile);
        }
        let cwd = fsio::clean(self.cwd);
        let root = walk::git_root(&cwd);
        if let Some(path) = walk::walk_up(&cwd, root.as_deref(), self.home) {
            return (path, FoundBy::WalkUp);
        }
        // The config file never beats the walk up: todo.sh's own template
        // sets TODO_FILE.
        if let Some(dir) = self.var("TODO_DIR") {
            return (Path::new(&dir).join(TODO_NAME), FoundBy::TodoDir);
        }
        if let Some(path) = self.file_var("TODO_FILE") {
            return (PathBuf::from(path), FoundBy::TodoFile);
        }
        if let Some(dir) = self.file_var("TODO_DIR") {
            return (Path::new(&dir).join(TODO_NAME), FoundBy::TodoDir);
        }
        if let Some(path) = self.default_file {
            return (self.expand_home(path), FoundBy::DefaultFile);
        }
        (root.unwrap_or(cwd).join(TODO_NAME), FoundBy::Fallback)
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
    let (todo, state) = locate(&input.abs(&path))?;
    let done = done::done_file(input, &todo, found_by)?;
    Ok(Found {
        todo,
        done,
        report_file: done::report_setting(input, found_by),
        found_by,
        state,
    })
}

/// The todo file at the absolute `path`, resolved through links, and its
/// state: a missing folder (the path stays as it is), a file, or none yet.
/// Both `find` and a reading of a list that is open give this answer.
///
/// # Errors
///
/// An `io` error when the file names a folder or cannot be examined.
pub fn locate(path: &Path) -> errs::Result<(PathBuf, FileState)> {
    let todo = match fsio::resolve(path) {
        Ok(todo) => todo,
        Err(e) if fsio::is_no_dir(&e) => {
            return Ok((path.to_path_buf(), FileState::DirMissing));
        }
        Err(e) => return Err(e),
    };
    let state = match fs::metadata(&todo) {
        Ok(meta) if meta.is_dir() => {
            return Err(errs::Error::new(
                errs::Code::Io,
                format!("{} is a folder", errs::lossy_path(&todo)),
            ));
        }
        Ok(_) => FileState::Exists,
        Err(e) if e.kind() == io::ErrorKind::NotFound => FileState::Missing,
        Err(e) => return Err(errs::io_at("stat", &todo, "stat", &todo, e)),
    };
    Ok((todo, state))
}

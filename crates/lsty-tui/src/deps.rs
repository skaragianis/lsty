//! Everything the model takes from the outside.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use jiff::{Timestamp, tz::TimeZone};
use lsty_core::{
    clip::Pasteboard,
    config::{Config, Warning},
    discover::Found,
    keys::Keymap,
    links::HookResult,
    ops::IdSource,
    theme::{Profile, Theme},
};

use crate::watch::Watch;

/// Reads the clock: the time of the moment it is called.
pub type Now = Box<dyn Fn() -> Timestamp>;

/// Starts the program that opens a path and does not wait for it; the
/// error is the reason it could not be started.
pub type Opener = Box<dyn Fn(&Path) -> HookResult>;

/// Runs the editor, given as the program, its arguments and the file,
/// on the terminal and waits for it; the error says how it failed.
pub type EditorRun = Box<dyn Fn(&[OsString]) -> HookResult>;

/// What the model starts from and uses only once: the files of the list,
/// which it then reads from the list it holds, the warnings, which it
/// shows one after the other, the start of the watch and the folder of the
/// session state.
pub struct Start {
    /// The todo and done files.
    pub found: Found,
    /// Problems found in the config, the keys and the theme; shown one
    /// per key, each for two seconds.
    pub warnings: Vec<Warning>,
    /// Starts the watch on the files; without it the list is reloaded
    /// only by the minute tick.
    pub watch: Option<Watch>,
    /// The folder of the files that keep the history and the folds
    /// between launches; without it nothing is remembered.
    pub state_dir: Option<PathBuf>,
}

/// What the model is made of and keeps. The model reads nothing itself:
/// not the clock, not the environment, not the terminal.
pub struct Deps {
    /// The options.
    pub config: Config,
    /// The key bindings.
    pub keys: Keymap,
    /// The colours.
    pub theme: Theme,
    /// How many colours the terminal shows.
    pub profile: Profile,
    /// Where the ids of new items come from.
    pub ids: Box<dyn IdSource>,
    /// Reads and writes the system clipboard.
    pub clipboard: Box<dyn Pasteboard>,
    /// The clock.
    pub now: Now,
    /// The local time zone.
    pub zone: TimeZone,
    /// The home folder, where a `file:` link that starts with `~` points.
    pub home: Option<PathBuf>,
    /// The folder of the user's theme files, taken from `cwd` when
    /// relative.
    pub themes: PathBuf,
    /// The config file the theme picker saves to, taken from `cwd` when
    /// relative; without it a chosen theme lasts for the session.
    pub config_file: Option<PathBuf>,
    /// The current folder.
    pub cwd: PathBuf,
    /// The editor setting (`EDITOR`) as the environment has it.
    pub editor: Option<OsString>,
    /// Opens a file that is not text.
    pub opener: Opener,
    /// Runs the editor with the terminal handed over; only the loop of
    /// a real terminal calls it.
    pub run_editor: EditorRun,
}

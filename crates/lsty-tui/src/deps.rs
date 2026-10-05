//! Everything the model takes from the outside.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::clip::Reader;
use lsty_core::config::{Config, Warning};
use lsty_core::discover::Found;
use lsty_core::keys::Keymap;
use lsty_core::ops::IdSource;
use lsty_core::theme::{Profile, Theme};

use crate::watch::Watch;

/// Reads the clock: the time of the moment it is called.
pub type Now = Box<dyn Fn() -> Timestamp>;

/// Starts the program that opens a path and does not wait for it; the
/// error is the reason it could not be started.
pub type Opener = Box<dyn Fn(&Path) -> Result<(), String>>;

/// Runs the editor, given as the program, its arguments and the file,
/// on the terminal and waits for it; the error says how it failed.
pub type EditorRun = Box<dyn Fn(&[OsString]) -> Result<(), String>>;

/// What the model is made of. The model reads nothing itself: not the
/// clock, not the environment, not the terminal.
pub struct Deps {
    /// The todo and done files.
    pub found: Found,
    /// The options.
    pub config: Config,
    /// The key bindings.
    pub keys: Keymap,
    /// The colours.
    pub theme: Theme,
    /// How many colours the terminal shows.
    pub profile: Profile,
    /// Problems found in the config, the keys and the theme; shown one
    /// after the other, each until a key is pressed.
    pub warnings: Vec<Warning>,
    /// Where the ids of new items come from.
    pub ids: Box<dyn IdSource>,
    /// Reads the system clipboard.
    pub clipboard: Box<dyn Reader>,
    /// The clock.
    pub now: Now,
    /// The local time zone.
    pub zone: TimeZone,
    /// The home folder, where a `file:` link that starts with `~` points.
    pub home: Option<PathBuf>,
    /// The folder of the user's theme files, taken from `cwd` when
    /// relative.
    pub themes: PathBuf,
    /// The current folder.
    pub cwd: PathBuf,
    /// The folder of the files that keep the history and the folds
    /// between launches; without it nothing is remembered.
    pub state_dir: Option<PathBuf>,
    /// The editor setting (`EDITOR`) as the environment has it.
    pub editor: Option<OsString>,
    /// Opens a file that is not text.
    pub opener: Opener,
    /// Runs the editor with the terminal handed over; only the loop of
    /// a real terminal calls it.
    pub run_editor: EditorRun,
    /// Starts the watch on the files; without it the list is reloaded
    /// only by the minute tick.
    pub watch: Option<Watch>,
}

/// What the model keeps of [`Deps`]: the todo and done files are read
/// from the list it holds, and the warnings are shown once.
pub(crate) struct Kept {
    pub(crate) config: Config,
    pub(crate) keys: Keymap,
    pub(crate) theme: Theme,
    pub(crate) profile: Profile,
    pub(crate) ids: Box<dyn IdSource>,
    pub(crate) clipboard: Box<dyn Reader>,
    pub(crate) now: Now,
    pub(crate) zone: TimeZone,
    pub(crate) home: Option<PathBuf>,
    pub(crate) themes: PathBuf,
    pub(crate) cwd: PathBuf,
    pub(crate) state_dir: Option<PathBuf>,
    pub(crate) editor: Option<OsString>,
    pub(crate) opener: Opener,
    pub(crate) run_editor: EditorRun,
    pub(crate) watch: Option<Watch>,
}

impl Deps {
    /// The files, the warnings and the rest.
    pub(crate) fn split(self) -> (Found, Vec<Warning>, Kept) {
        let kept = Kept {
            config: self.config,
            keys: self.keys,
            theme: self.theme,
            profile: self.profile,
            ids: self.ids,
            clipboard: self.clipboard,
            now: self.now,
            zone: self.zone,
            home: self.home,
            themes: self.themes,
            cwd: self.cwd,
            state_dir: self.state_dir,
            editor: self.editor,
            opener: self.opener,
            run_editor: self.run_editor,
            watch: self.watch,
        };
        (self.found, self.warnings, kept)
    }
}

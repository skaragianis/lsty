//! Helpers of the unit tests.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::clip::Reader;
use lsty_core::config::Config;
use lsty_core::dates::Clock;
use lsty_core::discover::{self, FileState, Found, FoundBy, Input};
use lsty_core::errs::{self, Code, Nr, Source};
use lsty_core::keys::{self, Stroke};
use lsty_core::list::{Params, Snapshot};
use lsty_core::ops::OsIds;
use lsty_core::theme::{self, Profile};
use lsty_core::todotxt::File;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tempfile::TempDir;

use crate::live::Load;
use crate::model::Model;
use crate::msg::{Cmd, Msg};
use crate::track::Place;
use crate::{Deps, Start};

/// 2026-09-29 09:00 UTC.
pub(crate) const NOW: &str = "2026-09-29T09:00:00Z";

pub(crate) const SAMPLE: &str = "(A) Call client +work\nBuy milk +home\n";

pub(crate) fn instant(text: &str) -> Timestamp {
    text.parse().unwrap_or(Timestamp::UNIX_EPOCH)
}

/// A folder with a todo file, unless `todo` is empty.
pub(crate) fn workdir(todo: &str) -> TempDir {
    let dir = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    if !todo.is_empty() {
        fs::write(dir.path().join("todo.txt"), todo)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    dir
}

/// The dependencies of a model of `dir`'s todo file, with the default
/// config and keys and the built-in default theme.
pub(crate) fn deps(dir: &Path, config: Config) -> Deps {
    let (keymap, _) =
        keys::load(&dir.join("keys.toml"), dir, false, &config.leader);
    let themes = dir.join("themes");
    let (theme, _) = theme::load(theme::DEFAULT_NAME, &themes, dir, true);
    let now = Rc::new(Cell::new(instant(NOW)));
    Deps {
        config,
        keys: keymap,
        theme,
        profile: Profile::TrueColor,
        ids: Box::new(OsIds),
        clipboard: Box::new(NoClipboard),
        now: Box::new(move || now.get()),
        zone: TimeZone::UTC,
        home: Some(dir.to_path_buf()),
        themes,
        config_file: Some(dir.join("config.toml")),
        cwd: dir.to_path_buf(),
        editor: None,
        opener: Box::new(|_| Ok(())),
        run_editor: Box::new(|_| Ok(())),
        state_dir: Some(dir.join("state")),
        watch: None,
    }
}

/// The default config with the sidebar and the detail panel off: the list
/// alone.
pub(crate) fn alone() -> Config {
    Config {
        show_sidebar: false,
        show_detail: false,
        ..Config::default()
    }
}

/// How a model of the `todo.txt` in `dir` starts: no warnings.
pub(crate) fn start(dir: &Path) -> Start {
    let vars = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(Path::new("todo.txt")),
        vars: &vars,
        cwd: dir,
        home: Some(dir),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    Start {
        found,
        warnings: Vec::new(),
    }
}

/// A started model of 80 by 24 cells, of the `todo.txt` in the folder
/// of `deps`.
pub(crate) fn started(deps: Deps) -> Model {
    started_from(start(&deps.cwd), deps)
}

/// A model started from `start`, of 80 by 24 cells.
pub(crate) fn started_from(start: Start, deps: Deps) -> Model {
    let mut model =
        Model::new(start, deps).unwrap_or_else(|e| panic!("{}", e.message()));
    model.update(Msg::Resize(80, 24));
    model
}

/// Presses one key for each character of `text`; the commands of the
/// last press come back.
pub(crate) fn press(model: &mut Model, text: &str) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    for c in text.chars() {
        cmds = model.update(Msg::Key(Stroke::char(c)));
    }
    cmds
}

/// The screen as lines of text.
pub(crate) fn screen(model: &Model, w: u16, h: u16) -> Vec<String> {
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    model.draw(&mut buf);
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// The snapshot of `todo` and `done` as of `now`.
pub(crate) fn snapshot(todo: &str, done: &str, now: &str) -> Snapshot {
    let vars = |name: &str| (name == "LSTY_NOW").then(|| OsString::from(now));
    let clock = Clock::new(&vars, Timestamp::UNIX_EPOCH, TimeZone::UTC)
        .unwrap_or_else(|e| panic!("{}", e.message()));
    let found = Found {
        todo: PathBuf::from("/x/todo.txt"),
        done: PathBuf::from("/x/done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    };
    let params = Params {
        clock,
        stale_after: Duration::from_hours(4),
    };
    Snapshot::build(
        found,
        File::parse(todo.as_bytes()),
        File::parse(done.as_bytes()),
        &params,
    )
}

/// A clipboard that cannot be read.
pub(crate) struct NoClipboard;

impl Reader for NoClipboard {
    fn read(&self) -> errs::Result<Vec<u8>> {
        Err(errs::Error::new(Code::Io, "no clipboard"))
    }
}

/// A clipboard the test fills by hand.
#[derive(Clone, Default)]
pub(crate) struct Board(Rc<RefCell<Option<Vec<u8>>>>);

impl Board {
    pub(crate) fn set(&self, text: &str) {
        *self.0.borrow_mut() = Some(text.as_bytes().to_vec());
    }

    /// Takes what a terminal takes of the commands of an update.
    pub(crate) fn take(&self, cmds: &[Cmd]) {
        for cmd in cmds {
            if let Cmd::Copy(text) = cmd {
                *self.0.borrow_mut() = Some(text.clone());
            }
        }
    }
}

impl Reader for Board {
    fn read(&self) -> errs::Result<Vec<u8>> {
        self.0
            .borrow()
            .clone()
            .ok_or_else(|| errs::Error::new(Code::Io, "empty"))
    }
}

/// The place of line `nr` of a file.
pub(crate) fn at(source: Source, nr: u32) -> Place {
    let nr = Nr::new(nr).unwrap_or_else(|| panic!("line 0"));
    (source, nr)
}

/// The reading that the commands ask for. Panics when there is none.
pub(crate) fn load_of(cmds: Vec<Cmd>) -> Box<Load> {
    for cmd in cmds {
        if let Cmd::Load(load) = cmd {
            return load;
        }
    }
    panic!("no load was asked for");
}

/// A reload with its reading done at once; what the model asks for
/// after the reading comes back.
pub(crate) fn reload(model: &mut Model) -> Vec<Cmd> {
    let msg = load_of(model.update(Msg::Reload)).read();
    model.update(msg)
}

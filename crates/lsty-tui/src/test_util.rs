//! Helpers of the unit tests.

use std::cell::Cell;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::config::Config;
use lsty_core::dates::Clock;
use lsty_core::discover::{self, FileState, Found, FoundBy, Input};
use lsty_core::keys::{self, Stroke};
use lsty_core::list::{Params, Snapshot};
use lsty_core::theme::{self, Profile};
use lsty_core::todotxt::File;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use tempfile::TempDir;

use crate::{Cmd, Deps, Model, Msg};

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
    let vars = |_: &str| None;
    let found = discover::find(&Input {
        explicit: Some(Path::new("todo.txt")),
        vars: &vars,
        cwd: dir,
        home: Some(dir),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()));
    let (keymap, _) =
        keys::load(&dir.join("keys.toml"), dir, false, &config.leader);
    let (theme, _) = theme::load(theme::DEFAULT_NAME, dir, dir, true);
    let now = Rc::new(Cell::new(instant(NOW)));
    Deps {
        found,
        config,
        keys: keymap,
        theme,
        profile: Profile::TrueColor,
        warnings: Vec::new(),
        now: Box::new(move || now.get()),
        zone: TimeZone::UTC,
    }
}

/// A started model of 80 by 24 cells.
pub(crate) fn started(deps: Deps) -> Model {
    let mut model =
        Model::new(deps).unwrap_or_else(|e| panic!("{}", e.message()));
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
    model.view(&mut buf);
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

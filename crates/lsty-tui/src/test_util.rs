//! Helpers of the unit tests.

use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use jiff::{Timestamp, tz::TimeZone};
use lsty_core::{
    clip::Pasteboard,
    config::Config,
    dates::Clock,
    discover::{self, FileState, Found, FoundBy, Input},
    errs::{self, Code, Nr, Source},
    keys::{self, Stroke, parse_keys},
    list::{Params, Snapshot},
    lock,
    ops::OsIds,
    theme::{self, Profile},
    todotxt::File,
};
use ratatui::{buffer::Buffer, layout::Rect};
use tempfile::TempDir;

use crate::{
    Deps, Start,
    live::Load,
    model::Model,
    msg::{Cmd, Msg},
    track::Place,
};

/// Holds the exclusive lock of the todo file in `dir`, as another writer
/// would, until it is released.
pub(crate) fn held_lock(dir: &Path) -> lock::Held {
    let want = lock::Want {
        file: dir.join("todo.txt"),
        mode: lock::Mode::Exclusive,
    };
    lock::acquire(&[want], Duration::from_secs(1), &lock::Cancel::new())
        .unwrap_or_else(|e| panic!("{}", e.message()))
}

/// 2026-09-29 09:00 UTC.
pub(crate) const NOW: &str = "2026-09-29T09:00:00Z";

pub(crate) const SAMPLE: &str = "(A) Call client +work\nBuy milk +home\n";

pub(crate) fn instant(text: &str) -> Timestamp {
    text.parse().unwrap_or(Timestamp::UNIX_EPOCH)
}

/// A folder with a todo file, unless `todo` is empty. Its path has no
/// symlinks, as the path of a todo file never has.
pub(crate) fn workdir(todo: impl AsRef<[u8]>) -> TempDir {
    let tmp = fs::canonicalize(std::env::temp_dir())
        .unwrap_or_else(|e| panic!("{e}"));
    let dir = TempDir::new_in(tmp).unwrap_or_else(|e| panic!("{e}"));
    let todo = todo.as_ref();
    if !todo.is_empty() {
        fs::write(dir.path().join("todo.txt"), todo)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    dir
}

/// A started model of a new folder holding `todo`, and the folder, which
/// must outlive the model: keys that save session state write there.
pub(crate) fn workdir_model(todo: &str, config: Config) -> (TempDir, Model) {
    let dir = workdir(todo);
    let model = started(deps(dir.path(), config));
    (dir, model)
}

/// Like `workdir_model` with the alone config, and `<C-x>` bound to
/// `cancel` in INSERT mode, which no default key does.
pub(crate) fn workdir_cancelling(todo: &str) -> (TempDir, Model) {
    let dir = workdir(todo);
    fs::write(
        dir.path().join("keys.toml"),
        "[insert]\ncancel = [\"<C-x>\"]\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let model = started(deps(dir.path(), alone()));
    (dir, model)
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
        clipboard: Box::new(Board::default()),
        now: Box::new(move || now.get()),
        zone: TimeZone::UTC,
        home: Some(dir.to_path_buf()),
        themes,
        config_file: Some(dir.join("config.toml")),
        cwd: dir.to_path_buf(),
        editor: None,
        opener: Box::new(|_| Ok(())),
        run_editor: Box::new(|_| Ok(())),
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

/// The folder the models of `start` keep their session state in.
pub(crate) fn state_of(dir: &Path) -> PathBuf {
    dir.join("state").join("lsty")
}

/// The files of the `explicit` path, found from `cwd`.
pub(crate) fn found_at(cwd: &Path, explicit: &Path) -> Found {
    let vars = |_: &str| None;
    discover::find(&Input {
        explicit: Some(explicit),
        vars: &vars,
        cwd,
        home: Some(cwd),
        default_file: None,
    })
    .unwrap_or_else(|e| panic!("{}", e.message()))
}

/// How a model of the `todo.txt` in `dir` starts: no warnings.
pub(crate) fn start(dir: &Path) -> Start {
    Start {
        found: found_at(dir, Path::new("todo.txt")),
        warnings: Vec::new(),
        watch: None,
        state_dir: Some(state_of(dir)),
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
/// last press come back, without the timer that clears its message.
pub(crate) fn press(model: &mut Model, text: &str) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    for c in text.chars() {
        cmds = model.update(Msg::Key(Stroke::char(c)));
    }
    cmds.retain(|c| {
        !matches!(
            c,
            Cmd::After {
                msg: Msg::Expire(_),
                ..
            }
        )
    });
    cmds
}

/// Sends the keys of `notation` and returns the commands of the last
/// one, as they are. A notation that does not parse panics.
pub(crate) fn keys(model: &mut Model, notation: &str) -> Vec<Cmd> {
    let strokes = parse_keys(notation, None)
        .unwrap_or_else(|e| panic!("{notation}: {e}"));
    let mut cmds = Vec::new();
    for stroke in strokes {
        cmds = model.update(Msg::Key(stroke));
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
    let vars = |name: &str| (name == "LSTY_NOW").then(|| now.into());
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

/// What the todo file in `dir` holds, empty when it is missing.
pub(crate) fn todo_of(dir: &Path) -> String {
    fs::read_to_string(dir.join("todo.txt")).unwrap_or_default()
}

/// What a [`Board`] holds, what was written to it and why a write
/// fails.
#[derive(Default)]
struct BoardState {
    text: Option<Vec<u8>>,
    written: Vec<Vec<u8>>,
    failing: Option<&'static str>,
}

/// A clipboard the test fills by hand, or that the model writes to.
#[derive(Clone, Default)]
pub(crate) struct Board(Rc<RefCell<BoardState>>);

impl Board {
    pub(crate) fn set(&self, text: &str) {
        self.0.borrow_mut().text = Some(text.as_bytes().to_vec());
    }

    /// Makes every write fail with `why`.
    pub(crate) fn fail_writes(&self, why: &'static str) {
        self.0.borrow_mut().failing = Some(why);
    }

    /// What the model wrote, one text for each write.
    pub(crate) fn written(&self) -> Vec<Vec<u8>> {
        self.0.borrow().written.clone()
    }

    /// Takes what a terminal takes of the commands of an update.
    pub(crate) fn take(&self, cmds: &[Cmd]) {
        for cmd in cmds {
            if let Cmd::Copy(text) = cmd {
                self.0.borrow_mut().text = Some(text.clone());
            }
        }
    }
}

impl Pasteboard for Board {
    fn read(&self) -> errs::Result<Vec<u8>> {
        self.0
            .borrow()
            .text
            .clone()
            .ok_or_else(|| errs::Error::new(Code::Io, "empty"))
    }

    fn write(&self, text: &[u8]) -> errs::Result<()> {
        let mut state = self.0.borrow_mut();
        if let Some(why) = state.failing {
            return Err(errs::Error::new(Code::Io, why));
        }
        state.written.push(text.to_vec());
        Ok(())
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

/// The line of the item under the cursor.
pub(crate) fn cursor_text(model: &Model) -> String {
    let item = model
        .rows
        .get(model.cursor)
        .and_then(|r| model.snap.item(r.id));
    item.map(|i| {
        String::from_utf8_lossy(model.snap.line(i).raw()).into_owned()
    })
    .unwrap_or_default()
}

/// The message timer among `cmds`.
pub(crate) fn expiry(cmds: &[Cmd]) -> Option<Msg> {
    cmds.iter().find_map(|c| match c {
        Cmd::After {
            delay,
            msg: Msg::Expire(n),
        } if *delay == Duration::from_secs(2) => Some(Msg::Expire(*n)),
        Cmd::Quit
        | Cmd::After { .. }
        | Cmd::Copy(_)
        | Cmd::Edit(_)
        | Cmd::Load(_) => None,
    })
}

/// The timer that refilters the list after a panel step, among `cmds`.
pub(crate) fn refiltering(cmds: &[Cmd]) -> Option<Msg> {
    cmds.iter().find_map(|c| match c {
        Cmd::After {
            delay,
            msg: Msg::Refilter(n),
        } if *delay == Duration::from_millis(50) => Some(Msg::Refilter(*n)),
        Cmd::Quit
        | Cmd::After { .. }
        | Cmd::Copy(_)
        | Cmd::Edit(_)
        | Cmd::Load(_) => None,
    })
}

/// Sends `keys` and then the timer of the last panel step among their
/// commands, as the list refilters when the 50 ms have passed.
pub(crate) fn settled(model: &mut Model, notation: &str) {
    let cmds = keys(model, notation);
    if let Some(msg) = refiltering(&cmds) {
        model.update(msg);
    }
}

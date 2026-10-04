//! Keeping the list in step with the files: the watcher's notices, the
//! debounce, the reload and what the status line says about it.

use std::fs;
use std::path::Path;
use std::time::Duration;

use lsty_core::config::Config;
use lsty_core::dates::Clock;
use lsty_core::discover::{FileState, Found};
use lsty_core::errs::{self, Code, Vars};
use lsty_core::fsio;
use lsty_core::list::{self, Params, Snapshot};
use lsty_core::lock::Cancel;
use lsty_core::todotxt::File;

use crate::deps::Deps;
use crate::model::Model;
use crate::msg::{Cmd, Msg};
use crate::panel::Entry;
use crate::track::Remap;
use crate::watch::{Notice, Sink, Watcher};

/// How long after the first event the reload runs.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(50);

/// Where a reload is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Nothing is owed.
    Idle,
    /// An event started the debounce.
    Waiting,
    /// The files are being read.
    Loading,
}

/// What the files held when they were last written or read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    todo: [u8; 32],
    done: [u8; 32],
    todo_exists: bool,
    done_exists: bool,
}

impl Stamp {
    pub(crate) fn of(snap: &Snapshot) -> Self {
        let found = snap.found();
        Self {
            todo: fsio::hash(&snap.todo().to_bytes()),
            done: fsio::hash(&snap.done().to_bytes()),
            todo_exists: found.state == FileState::Exists,
            done_exists: is_file(&found.done),
        }
    }
}

/// A reading of both files.
pub(crate) struct Loaded {
    pub(crate) snap: Snapshot,
    pub(crate) stamp: Stamp,
}

/// Everything a reading of the files needs, so that it can run on
/// another thread.
#[derive(Clone, Debug)]
pub struct Load {
    found: Found,
    config: Config,
    clock: Clock,
    cancel: Cancel,
    pub(crate) serial: u64,
}

impl Load {
    /// The message for a reading that could not be started.
    #[must_use]
    pub(crate) fn fail(self, err: errs::Error) -> Msg {
        Msg::Loaded(Box::new(Reading {
            serial: self.serial,
            clock: self.clock,
            result: Err(err),
        }))
    }

    /// Reads the files, waiting for their locks, and returns the result
    /// as the message for the model.
    #[must_use]
    pub fn read(self) -> Msg {
        let result =
            fetch(&self.found, &self.config, &self.clock, &self.cancel);
        Msg::Loaded(Box::new(Reading {
            serial: self.serial,
            clock: self.clock,
            result,
        }))
    }
}

/// A load is told by its generation.
impl PartialEq for Load {
    fn eq(&self, other: &Self) -> bool {
        self.serial == other.serial
    }
}

impl Eq for Load {}

/// What a [`Load`] found.
pub struct Reading {
    pub(crate) serial: u64,
    pub(crate) clock: Clock,
    pub(crate) result: errs::Result<Loaded>,
}

impl std::fmt::Debug for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reading")
            .field("serial", &self.serial)
            .field("ok", &self.result.is_ok())
            .finish_non_exhaustive()
    }
}

/// A reading is told by its generation.
impl PartialEq for Reading {
    fn eq(&self, other: &Self) -> bool {
        self.serial == other.serial
    }
}

impl Eq for Reading {}

/// Why a reload is due without a notice.
#[derive(Clone, Copy, Debug)]
struct Debt {
    /// The watch is down: every minute reloads.
    polling: bool,
    /// The last reload did not get the files, or the folder is gone.
    owed: bool,
}

/// What the status line says about the files.
#[derive(Clone, Copy, Debug)]
struct Shown {
    /// The todo file is there.
    exists: bool,
    /// The todo file was there and is not any more.
    deleted: bool,
    /// The last reload found the lock busy.
    busy: bool,
}

/// What the model knows about the files beyond the list.
pub(crate) struct Live {
    watcher: Option<Box<dyn Watcher>>,
    phase: Phase,
    debt: Debt,
    shown: Shown,
    pub(crate) seen: Stamp,
    /// An event came, or a reload fell due, while the files were being
    /// read: one more reload is owed after it.
    dirty: bool,
    /// Counts what changes the state shown. A reading that started under
    /// an older number is out of date and dropped.
    pub(crate) serial: u64,
}

impl Live {
    /// Whether the todo file was there at the last reading.
    pub(crate) const fn file_exists(&self) -> bool {
        self.shown.exists
    }

    /// The state after the first reading, which is `seen`. A reload is
    /// owed while the folder is gone.
    pub(crate) fn new(seen: Stamp, folder_gone: bool) -> Self {
        Self {
            watcher: None,
            phase: Phase::Idle,
            debt: Debt {
                polling: false,
                owed: folder_gone,
            },
            shown: Shown {
                exists: seen.todo_exists,
                deleted: false,
                busy: false,
            },
            seen,
            dirty: false,
            serial: 0,
        }
    }
}

fn is_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}

fn is_dir(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_dir())
}

/// The state of the todo file now.
fn probe(todo: &Path) -> FileState {
    if !todo.parent().is_some_and(is_dir) {
        FileState::DirMissing
    } else if is_file(todo) {
        FileState::Exists
    } else {
        FileState::Missing
    }
}

/// Reads both files under shared locks. A folder that is missing has
/// nothing to lock: the list is empty.
pub(crate) fn fetch(
    found: &Found,
    config: &Config,
    clock: &Clock,
    cancel: &Cancel,
) -> errs::Result<Loaded> {
    let mut found = found.clone();
    found.state = probe(&found.todo);
    let params = Params {
        clock: clock.clone(),
        stale_after: config.claim_stale_after,
    };
    let snap = if found.state == FileState::DirMissing {
        Snapshot::build(found, File::default(), File::default(), &params)
    } else {
        list::load(&found, config.lock_timeout, cancel, &params)?
    };
    Ok(Loaded {
        stamp: Stamp::of(&snap),
        snap,
    })
}

/// A reading of time that takes nothing from the environment.
fn no_vars(_: &str) -> Option<std::ffi::OsString> {
    None
}

/// The clock at the moment `deps` give.
pub(crate) fn clock_of(deps: &Deps) -> errs::Result<Clock> {
    let vars: &Vars<'_> = &no_vars;
    Clock::new(vars, (deps.now)(), deps.zone.clone())
}

impl Model {
    /// The clock at the time of the call.
    pub(crate) fn clock_now(&self) -> errs::Result<Clock> {
        clock_of(&self.deps)
    }

    /// Starts the watch on the files. When it cannot start the list is
    /// reloaded every minute instead.
    pub(crate) fn start_watch(&mut self, sink: Sink) {
        let Some(start) = &self.deps.watch else {
            return;
        };
        match start(&self.deps.found, sink) {
            Ok(watcher) => self.live.watcher = Some(watcher),
            Err(why) => self.poll_because(&why),
        }
    }

    /// What the watch tells.
    pub(crate) fn on_notice(&mut self, notice: Notice) -> Vec<Cmd> {
        match notice {
            Notice::Changed => self.on_event().into_iter().collect(),
            Notice::Failed(why) => {
                self.poll_because(&why);
                Vec::new()
            }
        }
    }

    /// Says once that the watch is down and the list reloads every
    /// minute.
    fn poll_because(&mut self, why: &str) {
        if self.live.debt.polling {
            return;
        }
        self.live.debt.polling = true;
        self.message = Some(format!(
            "can't watch {}: {why}; reloading every minute",
            self.base_name()
        ));
    }

    /// An event starts the debounce. Events within it extend nothing;
    /// one during a reading owes one more reload.
    fn on_event(&mut self) -> Option<Cmd> {
        match self.live.phase {
            Phase::Idle => {
                self.live.phase = Phase::Waiting;
                Some(Cmd::After {
                    delay: DEBOUNCE,
                    msg: Msg::Reload,
                })
            }
            Phase::Loading => {
                self.live.dirty = true;
                None
            }
            Phase::Waiting => None,
        }
    }

    /// Starts reading the files again.
    pub(crate) fn reload(&mut self) -> Option<Cmd> {
        let clock = match self.clock_now() {
            Ok(clock) => clock,
            Err(e) => {
                self.live.phase = Phase::Idle;
                self.message = Some(e.message().to_owned());
                return None;
            }
        };
        self.live.phase = Phase::Loading;
        if let Some(watcher) = self.live.watcher.as_mut() {
            // A folder that cannot be watched is told by the next event.
            drop(watcher.rewatch());
        }
        Some(Cmd::Load(Box::new(Load {
            found: self.deps.found.clone(),
            config: self.deps.config.clone(),
            clock,
            cancel: self.cancel.clone(),
            serial: self.live.serial,
        })))
    }

    /// A reading is back. One that started before the state shown last
    /// changed is dropped and owed again.
    pub(crate) fn on_loaded(&mut self, reading: Reading) -> Vec<Cmd> {
        self.live.phase = Phase::Idle;
        if reading.serial == self.live.serial {
            match reading.result {
                Ok(loaded) => self.accept(loaded, reading.clock),
                Err(e) => self.failed(&e),
            }
        } else {
            self.live.dirty = true;
        }
        if !self.live.dirty {
            return Vec::new();
        }
        self.live.dirty = false;
        self.on_event().into_iter().collect()
    }

    /// A reload that failed is owed again at the next event or tick.
    fn failed(&mut self, e: &errs::Error) {
        self.live.debt.owed = true;
        if e.code() == Code::LockTimeout {
            self.live.shown.busy = true;
        } else {
            self.message = Some(e.message().to_owned());
        }
    }

    /// Shows a reading. While the folder is gone a reload is owed for
    /// each tick, so that its return is noticed.
    fn accept(&mut self, loaded: Loaded, clock: Clock) {
        let gone = loaded.snap.found().state == FileState::DirMissing;
        self.live.debt.owed = gone;
        self.live.shown.busy = false;
        if loaded.stamp != self.live.seen {
            self.install(loaded, clock);
        }
    }

    /// Starts a reload now when one is owed or the watch is down.
    pub(crate) fn reload_owed(&mut self) -> Option<Cmd> {
        if !self.live.debt.owed && !self.live.debt.polling {
            return None;
        }
        match self.live.phase {
            Phase::Idle => self.reload(),
            Phase::Loading => {
                self.live.dirty = true;
                None
            }
            Phase::Waiting => None,
        }
    }

    /// Makes `loaded` the state shown, keeping the cursor on its item.
    fn install(&mut self, loaded: Loaded, clock: Clock) {
        let place = self.cursor_place();
        let offset = self.cursor.saturating_sub(self.top);
        let panels: Vec<(Vec<Entry>, usize)> = self
            .panels
            .iter()
            .map(|p| (p.entries.clone(), p.cursor))
            .collect();
        let old = std::mem::replace(&mut self.snap, loaded.snap);
        let moved = Remap::new(&old, &self.snap);
        let exists = loaded.stamp.todo_exists;
        self.live.shown.deleted =
            !exists && (self.live.shown.exists || self.live.shown.deleted);
        self.live.shown.exists = exists;
        self.live.seen = loaded.stamp;
        self.live.serial += 1;
        self.deps.found = self.snap.found().clone();
        self.clock = clock;
        self.rebuild_at(place.map(|at| moved.place(at)), offset);
        for (panel, (entries, cursor)) in self.panels.iter_mut().zip(&panels) {
            panel.land(entries, *cursor);
        }
        self.scroll_panels();
    }

    /// What the status line says about the files.
    pub(crate) fn notices(&self) -> Vec<String> {
        let name = self.base_name();
        let mut out = Vec::new();
        if self.live.shown.deleted {
            out.push(format!("{name} deleted"));
        }
        if self.live.shown.busy {
            out.push(format!("{name} is busy; showing the last state read"));
        }
        out
    }
}

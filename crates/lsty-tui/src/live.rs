//! Keeping the list in step with the files: the watcher's notices, the
//! debounce, the reload and what the status line says about it.

use std::{collections::HashSet, time::Duration};

use jiff::tz::TimeZone;
use lsty_core::{
    Vars,
    config::Config,
    dates::Clock,
    discover::{self, FileState, Found},
    errs::{self, Code},
    list::{self, Params, Snapshot},
    lock::Cancel,
    ops::Change,
    todotxt::File,
};

use crate::{
    deps::Now,
    model::Model,
    motion::View,
    msg::{Cmd, Msg},
    rows::Row,
    track::{Place, Remap, Wrote},
    watch::{Notice, Sink, Watch, Watcher},
};

/// How long after the first event the reload runs.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(50);

/// Where a reload is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Nothing is owed.
    Idle,
    /// An event started the debounce.
    Waiting,
    /// The files are being read. `dirty`: an event came, or a reload fell
    /// due, meanwhile, and one more reload is owed after it.
    Loading { dirty: bool },
}

/// Whether a reading found what the list shows: the same file state,
/// parsed the same.
fn same_state(new: &Snapshot, shown: &Snapshot) -> bool {
    new.found().state == shown.found().state
        && new.todo() == shown.todo()
        && new.done() == shown.done()
}

/// Everything a reading of the files needs, so that it can run on
/// another thread.
#[derive(Debug)]
pub(crate) struct Load {
    found: Found,
    config: Config,
    pub(crate) clock: Clock,
    cancel: Cancel,
    pub(crate) serial: u64,
}

impl Load {
    /// Reads the files, waiting for their locks, and returns the result
    /// as the message for the model.
    #[must_use]
    pub(crate) fn read(self) -> Msg {
        let result =
            fetch(&self.found, &self.config, &self.clock, &self.cancel);
        answer(self.serial, self.clock, result)
    }
}

/// The message for the reading of generation `serial`.
pub(crate) fn answer(
    serial: u64,
    clock: Clock,
    result: errs::Result<Snapshot>,
) -> Msg {
    Msg::Loaded(Box::new(Reading {
        serial,
        clock,
        result,
    }))
}

/// A load is told by its generation.
impl PartialEq for Load {
    fn eq(&self, other: &Self) -> bool {
        self.serial == other.serial
    }
}

impl Eq for Load {}

/// What a [`Load`] found.
pub(crate) struct Reading {
    pub(crate) serial: u64,
    pub(crate) clock: Clock,
    pub(crate) result: errs::Result<Snapshot>,
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
    /// Counts what changes the state shown. A reading that started under
    /// an older number is out of date and dropped.
    serial: u64,
}

impl Live {
    /// The state after the first reading, `snap`. A reload is owed while
    /// the folder is gone.
    pub(crate) fn new(snap: &Snapshot) -> Self {
        Self {
            watcher: None,
            phase: Phase::Idle,
            debt: Debt {
                polling: false,
                owed: snap.found().state == FileState::DirMissing,
            },
            shown: Shown {
                deleted: false,
                busy: false,
            },
            serial: 0,
        }
    }
}

/// Reads both files under shared locks. A folder that is missing has
/// nothing to lock: the list is empty.
pub(crate) fn fetch(
    found: &Found,
    config: &Config,
    clock: &Clock,
    cancel: &Cancel,
) -> errs::Result<Snapshot> {
    let mut found = found.clone();
    found.state = discover::locate(&found.todo)?.1;
    let params = Params::new(clock.clone(), config);
    if found.state == FileState::DirMissing {
        Ok(Snapshot::build(
            found,
            File::default(),
            File::default(),
            &params,
        ))
    } else {
        list::load(&found, config.lock_timeout, cancel, &params)
    }
}

/// The clock at the moment `now` gives, in `zone`.
pub(crate) fn clock_in(now: &Now, zone: &TimeZone) -> errs::Result<Clock> {
    // A reading of time that takes nothing from the environment
    let vars: &Vars<'_> = &|_| None;
    Clock::new(vars, now(), zone.clone())
}

impl Model {
    /// The clock at the time of the call.
    pub(crate) fn clock_now(&self) -> errs::Result<Clock> {
        clock_in(&self.deps.now, &self.deps.zone)
    }

    /// Starts the watch on the files. When it cannot start the list is
    /// reloaded every minute instead.
    pub(crate) fn start_watch(&mut self, watch: Option<Watch>, sink: Sink) {
        let Some(start) = watch else {
            return;
        };
        match start(self.snap.found(), sink) {
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
            Phase::Loading { .. } => {
                self.live.phase = Phase::Loading { dirty: true };
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
        self.live.phase = Phase::Loading { dirty: false };
        if let Some(watcher) = self.live.watcher.as_mut() {
            // A folder that cannot be watched is told by the next event.
            drop(watcher.rewatch());
        }
        Some(Cmd::Load(Box::new(Load {
            found: self.snap.found().clone(),
            config: self.deps.config.clone(),
            clock,
            cancel: self.cancel.clone(),
            serial: self.live.serial,
        })))
    }

    /// A reading is back. One that started before the state shown last
    /// changed is dropped and owed again.
    pub(crate) fn on_loaded(&mut self, reading: Reading) -> Vec<Cmd> {
        let stale = reading.serial != self.live.serial;
        let owed =
            stale || matches!(self.live.phase, Phase::Loading { dirty: true });
        self.live.phase = Phase::Idle;
        if !stale {
            match reading.result {
                Ok(snap) => self.accept(snap, reading.clock),
                Err(e) => self.failed(&e),
            }
        }
        if !owed {
            return Vec::new();
        }
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
    fn accept(&mut self, snap: Snapshot, clock: Clock) {
        self.live.debt.owed = snap.found().state == FileState::DirMissing;
        self.live.shown.busy = false;
        if !same_state(&snap, &self.snap) {
            self.install(snap, clock, None);
        }
    }

    /// Owes a reload: the files are not what the list shows.
    pub(crate) fn owe_reload(&mut self) {
        self.live.debt.owed = true;
    }

    /// Starts a reload now when one is owed or the watch is down.
    pub(crate) fn reload_owed(&mut self) -> Option<Cmd> {
        if !self.live.debt.owed && !self.live.debt.polling {
            return None;
        }
        match self.live.phase {
            Phase::Idle => self.reload(),
            Phase::Loading { .. } | Phase::Waiting => self.on_event(),
        }
    }

    /// Makes `snap` the state shown, keeping the cursor on its task.
    /// Tells whether the write chose the row the cursor went to.
    pub(crate) fn install(
        &mut self,
        snap: Snapshot,
        clock: Clock,
        wrote: Option<&Wrote<'_>>,
    ) -> bool {
        let place = self.cursor_place();
        let offset = self.view_offset();
        let selected = self.selected_places();
        let old = std::mem::replace(&mut self.snap, snap);
        let mut follow = selected.clone();
        follow.extend(place);
        let moved =
            Remap::new(&old, &self.snap, wrote.map(|w| w.record), &follow);
        self.live.shown.deleted = !self.file_exists()
            && (old.found().state == FileState::Exists
                || self.live.shown.deleted);
        self.live.serial += 1;
        self.clock = clock;
        self.hist.follow(&moved);
        self.remap_clip(&moved);
        self.sticky.remap(&moved);
        self.remap_peek(&moved);
        self.remap_pointer(&moved);
        self.remap_prompt(&moved);
        self.remap_drafts(&moved, &old, wrote.is_some());
        if let Some(w) = wrote {
            self.stick(&moved, w.touched);
        }
        let aim = match self.pin_spot() {
            Some(spot) if wrote.is_none() => Some(spot),
            _ => place.map(|at| moved.place(at)),
        };
        let old_rows = std::mem::take(&mut self.rows);
        self.rebuild_rows();
        self.rebuild_panels();
        let landed =
            wrote.and_then(|w| self.landing(&old_rows, &moved, w, place));
        let chose = landed.is_some();
        let (aim, offset, reveal) = match landed {
            // The top of the view stays; the cursor is shown by the
            // least scroll.
            Some(at) => (Some(at), View::Top(self.top), true),
            None => (aim, offset, matches!(offset, View::Cursor(_))),
        };
        self.follow(aim, offset);
        self.resume(&moved, &selected);
        if reveal {
            self.scroll();
        }
        chose
    }

    /// Where a write sends the cursor, when it takes it off its task. It
    /// goes by the action's own lines (see [`Change::own`]). A delete or
    /// a completion of the cursor's task sends it to the first row below
    /// the cursor that the write did not act on; a delete with none
    /// below takes the nearest above. An undo or redo sends it to the
    /// first row of the lines it made appear, else the nearest row left
    /// below the topmost line it removed, else the topmost row of the
    /// lines it changed.
    fn landing(
        &self,
        old: &[Row],
        moved: &Remap,
        wrote: &Wrote<'_>,
        cursor: Option<Place>,
    ) -> Option<Place> {
        let record = wrote.record;
        let own: Vec<_> = record
            .files
            .iter()
            .flat_map(|f| f.changes.iter().map(|c| (f.source, c)))
            .filter(|(_, c)| c.own)
            .collect();
        let places = |keep: fn(&Change) -> bool,
                      nr: fn(&Change) -> Option<_>| {
            let at = own.iter().filter(|(_, c)| keep(c));
            at.filter_map(|&(s, c)| Some((s, nr(c)?)))
                .collect::<HashSet<Place>>()
        };
        let appeared = places(|c| c.before.is_empty(), |c| c.nr_after);
        let vanished = places(|c| c.after.is_empty(), |c| c.nr_before);
        let acted = places(|_| true, |c| c.nr_before);
        let from = if wrote.undone {
            let present = places(|_| true, |c| c.nr_after);
            let mut rows =
                self.rows.iter().filter(|r| present.contains(&r.place()));
            let new = rows.clone().find(|r| appeared.contains(&r.place()));
            if let Some(row) = new {
                return Some(row.place());
            }
            match old.iter().position(|r| vanished.contains(&r.place())) {
                Some(at) => at,
                None => return rows.next().map(Row::place),
            }
        } else {
            if !matches!(record.action.as_str(), "del" | "do")
                || !acted.contains(&cursor?)
            {
                return None;
            }
            self.cursor
        };
        let left = |r: &&Row| {
            moved.track(r.place()).is_some()
                && (wrote.undone || !acted.contains(&r.place()))
        };
        let below = old.iter().skip(from + 1).find(left);
        // A completion that leaves no row below stays on its task.
        let up = wrote.undone || record.action == "del";
        let above = old.iter().take(from).rev().find(left).filter(|_| up);
        let stay = wrote.touched.iter().find(|t| !up && t.was == cursor);
        let stay = stay.map(|t| t.now);
        let at = below.or(above).map(|r| moved.place(r.place()));
        at.or(stay)
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

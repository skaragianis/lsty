//! Keeping the list in step with the files: the watcher's notices, the
//! debounce, the reload and what the status line says about it.

use std::{cell::Cell, collections::HashSet, time::Duration};

use jiff::tz::TimeZone;
use lsty_core::{
    Vars,
    config::Config,
    dates::Clock,
    discover::{self, FileState, Found},
    errs::{self, Code},
    list::{self, Params, Snapshot},
    lock::Cancel,
    ops::{Change, Landing},
    todotxt::File,
};

use crate::{
    actions::places,
    counts::ids_of,
    deps::Now,
    model::Model,
    motion::View,
    msg::{Cmd, Msg},
    rows::Row,
    track::{Place, Remap, Wrote, landed_at},
    watch::{Notice, Sink, Watch, Watcher},
};

/// How long after the first event the reload runs.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(50);

/// Where a reload is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// Nothing is owed.
    #[default]
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

/// What the status line says about the files.
#[derive(Clone, Copy, Debug, Default)]
struct Shown {
    /// The todo file was there and is not any more.
    deleted: bool,
    /// The last reload found the lock busy.
    busy: bool,
    unwatched: bool,
}

/// What the model knows about the files beyond the list.
#[derive(Default)]
pub(crate) struct Live {
    watcher: Option<Box<dyn Watcher>>,
    phase: Phase,
    /// The lists the core read when it refused a write on lines that
    /// changed, and the clock it read them by, shown once the input is
    /// over.
    pub(crate) read: Cell<Option<(Snapshot, Clock)>>,
    shown: Shown,
    /// Counts what changes the state shown. A reading that started under
    /// an older number is out of date and dropped.
    serial: u64,
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
            Err(why) => self.unwatched_because(&why),
        }
    }

    /// What the watch tells.
    pub(crate) fn on_notice(&mut self, notice: Notice) -> Vec<Cmd> {
        match notice {
            Notice::Changed => self.on_event().into_iter().collect(),
            Notice::Failed(why) => {
                self.unwatched_because(&why);
                Vec::new()
            }
        }
    }

    /// Says once that the watch is down and the list reloads every
    /// minute.
    fn unwatched_because(&mut self, why: &str) {
        if self.live.shown.unwatched {
            return;
        }
        self.live.shown.unwatched = true;
        self.warn(format!(
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
                self.alert(e.message().to_owned());
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

    /// A reload that failed: the next event or minute reads again.
    fn failed(&mut self, e: &errs::Error) {
        if e.code() == Code::LockTimeout {
            self.live.shown.busy = true;
        } else {
            self.alert(e.message().to_owned());
        }
    }

    /// Shows a reading: of the watch, or the one an operation made under
    /// the lock, so that its write lands as on that reading.
    pub(crate) fn accept(&mut self, snap: Snapshot, clock: Clock) {
        self.live.shown.busy = false;
        if !same_state(&snap, &self.snap) {
            self.install(snap, clock, None);
        }
    }

    /// Shows the lists the core read when it refused a write on lines that
    /// changed, as a reading of the watch would be, before the next input
    /// reads the list.
    pub(crate) fn show_read(&mut self) {
        if let Some((snap, clock)) = self.live.read.take() {
            self.unseen = true;
            self.accept(snap, clock);
        }
    }

    /// Starts a reload now, or after the one under way. The minute does
    /// this watched or not, since a watch can miss events (network and
    /// shared folders).
    pub(crate) fn read_now(&mut self) -> Option<Cmd> {
        match self.live.phase {
            Phase::Idle => self.reload(),
            Phase::Loading { .. } | Phase::Waiting => self.on_event(),
        }
    }

    /// Makes `snap` the state shown. After a write the cursor stays on its
    /// task. After an outside edit it stays on a line that did not change
    /// and is still shown, and otherwise on its screen row.
    /// Tells whether the write chose the row the cursor went to.
    pub(crate) fn install(
        &mut self,
        snap: Snapshot,
        clock: Clock,
        wrote: Option<&Wrote<'_>>,
    ) -> bool {
        let place = self.cursor_place();
        let offset = self.view_offset();
        let ends = self.selection_ends();
        let typed = self.typed_lines();
        let old = std::mem::replace(&mut self.snap, snap);
        let record = wrote.map(|w| w.record);
        let moved = Remap::new(&old, &self.snap, record, place.as_slice());
        let landing = wrote.is_none().then(|| Landing::of(&old, &self.snap));
        self.live.shown.deleted = !self.file_exists()
            && (old.found().state == FileState::Exists
                || self.live.shown.deleted);
        self.live.serial += 1;
        self.clock = clock;
        self.hist.follow(&moved);
        self.remap_clip(&moved, landing.as_ref());
        self.sticky.remap(&moved);
        self.remap_peek(&moved);
        self.remap_pointer(&moved);
        self.land_prompt(&old);
        self.remap_drafts(&moved, &old, landing.as_ref());
        if let Some(w) = wrote {
            self.stick(&moved, w.touched);
        }
        // An open field aims at its place. That is in the coordinates of
        // the reading before a write, which is right because the line the
        // write adds takes that index.
        let spot = self.edit.as_ref().and_then(|d| d.at);
        let old_rows = std::mem::take(&mut self.rows);
        self.rebuild_rows();
        self.rebuild_panels();
        let aim = spot.or_else(|| match &landing {
            Some(landing) => place
                .and_then(|at| landed_at(landing, at))
                .filter(|at| self.row_at(*at).is_some()),
            None => place.map(|at| moved.place(at)),
        });
        let landed =
            wrote.and_then(|w| self.landing(&old_rows, &moved, w, place));
        let chose = landed.is_some();
        let (aim, offset, reveal) = match landed {
            // The top of the view stays; the cursor is shown by the
            // least scroll.
            Some(at) => (Some(at), View::Top(self.top), true),
            None => (aim, offset, matches!(offset, View::Cursor(_))),
        };
        match aim {
            Some(_) => self.follow(aim, offset),
            None => self.stay_on_row(),
        }
        self.resume(ends, &old_rows, |at| match &landing {
            Some(landing) => landed_at(landing, at),
            None => moved.track(at),
        });
        self.check_typed(typed, landing.as_ref());
        if ends.is_some() && self.visual.is_none() {
            self.reset_keys();
        }
        if reveal {
            self.scroll();
        }
        chose
    }

    /// The lines the keys typed so far are about, when any wait for more.
    fn typed_lines(&self) -> Option<Vec<Place>> {
        let matcher = self.matchers.get(&self.key_mode())?;
        (matcher.waits() || self.visual.is_some())
            .then(|| self.lines_under(matcher.count()))
    }

    /// The lines of the `count` rows from the cursor.
    fn lines_under(&self, count: usize) -> Vec<Place> {
        places(&self.snap, &ids_of(self.rows_of(count)))
    }

    /// Notes that the keys typed, or the selection, are no longer about
    /// the same lines. They stay, for a motion to go on; the first key that
    /// would write refuses (see [`crate::press::writes_line`]).
    fn check_typed(
        &mut self,
        typed: Option<Vec<Place>>,
        landing: Option<&Landing>,
    ) {
        let (Some(typed), Some(landing)) = (typed, landing) else {
            return;
        };
        let now = self.lines_under(self.typed_count());
        let same = typed
            .iter()
            .map(|at| landed_at(landing, *at))
            .eq(now.into_iter().map(Some));
        self.keys_lost |= !same;
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
            if !matches!(record.action.as_str(), "del" | "do" | "wontdo")
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

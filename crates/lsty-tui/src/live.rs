//! Keeping the list in step with the files: the watcher's notices, the
//! debounce, the reload and what the status line says about it.

use std::{
    collections::HashSet,
    fs,
    time::{Duration, SystemTime},
};

use jiff::tz::TimeZone;
use lsty_core::{
    Vars,
    config::Config,
    dates::Clock,
    discover::{self, FileState, Found},
    errs::{self, Code},
    list::{self, Params, Snapshot},
    lock::Cancel,
    ops::{self, Change, Landing},
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

/// The size and modification time of the todo and done files, as a reading
/// found them before it read; `None` for a file that is not there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stamp([Option<(u64, SystemTime)>; 2]);

impl Stamp {
    fn of(found: &Found) -> Self {
        Self([&found.todo, &found.done].map(|path| {
            let meta = fs::metadata(path).ok()?;
            Some((meta.len(), meta.modified().ok()?))
        }))
    }
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
    result: errs::Result<(Snapshot, Stamp)>,
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
    pub(crate) result: errs::Result<(Snapshot, Stamp)>,
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
    /// The list said it changed: the input that said so reads at once.
    kick: bool,
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
    /// The files as the last reading found them.
    stamp: Stamp,
    /// Counts what changes the state shown. A reading that started under
    /// an older number is out of date and dropped.
    serial: u64,
}

impl Live {
    /// The state after the first reading, `snap`. A reload is owed while
    /// the folder is gone.
    pub(crate) fn new((snap, stamp): &(Snapshot, Stamp)) -> Self {
        Self {
            watcher: None,
            phase: Phase::Idle,
            debt: Debt {
                polling: false,
                owed: snap.found().state == FileState::DirMissing,
                kick: false,
            },
            shown: Shown {
                deleted: false,
                busy: false,
            },
            stamp: *stamp,
            serial: 0,
        }
    }
}

/// Reads both files under shared locks, with what they were before. A
/// folder that is missing has nothing to lock: the list is empty.
pub(crate) fn fetch(
    found: &Found,
    config: &Config,
    clock: &Clock,
    cancel: &Cancel,
) -> errs::Result<(Snapshot, Stamp)> {
    let stamp = Stamp::of(found);
    let mut found = found.clone();
    found.state = discover::locate(&found.todo)?.1;
    let params = Params::new(clock.clone(), config);
    let snap = if found.state == FileState::DirMissing {
        Snapshot::build(found, File::default(), File::default(), &params)
    } else {
        list::load(&found, config.lock_timeout, cancel, &params)?
    };
    Ok((snap, stamp))
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
                Ok(read) => self.accept(read, reading.clock),
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
    fn accept(&mut self, (snap, stamp): (Snapshot, Stamp), clock: Clock) {
        self.live.stamp = stamp;
        self.live.debt.owed = snap.found().state == FileState::DirMissing;
        self.live.shown.busy = false;
        if !same_state(&snap, &self.snap) {
            self.install(snap, clock, None);
        }
    }

    /// Shows the files an operation read when another writer had changed
    /// them since the list did, so that its write lands as on that reading.
    pub(crate) fn catch_up(&mut self, read: &Snapshot, clock: Clock) {
        if !same_state(read, &self.snap) {
            self.install(read.clone(), clock, None);
        }
    }

    /// Before a key that is not dim on the list drawn: reads the files when
    /// their size or time is not what the last reading found. It does not
    /// wait for the lock; a held one leaves the list as it is, for the next
    /// key to try. The lines of the `count` rows from the cursor, or of the
    /// selection, that the reading moved are lost (see
    /// [`Model::check_typed`]).
    pub(crate) fn read_if_moved(&mut self, count: usize) {
        if Stamp::of(self.snap.found()) == self.live.stamp {
            return;
        }
        let mut config = self.deps.config.clone();
        config.lock_timeout = Duration::ZERO;
        let Ok(clock) = self.clock_now() else {
            return;
        };
        let Ok(read) = fetch(self.snap.found(), &config, &clock, &self.cancel)
        else {
            return;
        };
        let typed = self.edit.is_none().then(|| self.lines_under(count));
        let landing = Landing::of(&self.snap, &read.0);
        self.accept(read, clock);
        self.check_typed(typed, Some(&landing), count);
    }

    /// Takes the files as the last reading found them, as when they change
    /// between the check before a key and its press.
    #[cfg(any(test, feature = "fuzzing"))]
    pub(crate) fn miss_change(&mut self) {
        self.live.stamp = Stamp::of(self.snap.found());
    }

    /// Reads the files before a key that writes on the line under the
    /// cursor opens a field. Whether that line is not there to write, said
    /// when so.
    pub(crate) fn row_changed(&mut self) -> bool {
        let read = self.clock_now().and_then(|clock| {
            let read = fetch(
                self.snap.found(),
                &self.deps.config,
                &clock,
                &self.cancel,
            )?;
            Ok((read, clock))
        });
        let (read, clock) = match read {
            Ok(read) => read,
            Err(e) => {
                self.fail(&e);
                return true;
            }
        };
        let landing = Landing::of(&self.snap, &read.0);
        let at = self.cursor_place();
        let changed = at.is_some_and(|at| landed_at(&landing, at).is_none());
        self.accept(read, clock);
        if changed {
            self.alert(ops::LIST_CHANGED.to_owned());
        }
        changed
    }

    /// Owes a reload: the files are not what the list shows.
    pub(crate) fn owe_reload(&mut self) {
        self.live.debt.owed = true;
        self.live.debt.kick = true;
    }

    /// Starts the reload an input owed by saying the list changed, so that
    /// the hints show the files as they are.
    pub(crate) fn kick_reload(&mut self) -> Option<Cmd> {
        if !std::mem::take(&mut self.live.debt.kick) {
            return None;
        }
        self.reload_owed()
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
        self.check_typed(typed, landing.as_ref(), self.typed_count());
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
        count: usize,
    ) {
        let (Some(typed), Some(landing)) = (typed, landing) else {
            return;
        };
        let now = self.lines_under(count);
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

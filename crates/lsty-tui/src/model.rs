//! The model: the list as loaded, the cursor and the status.

use std::collections::BTreeMap;
use std::time::Duration;

use lsty_core::config::Warning;
use lsty_core::dates::Clock;
use lsty_core::discover::FileState;
use lsty_core::errs::{self, Vars};
use lsty_core::keys::{Action, Matcher, Mode, Outcome, Stroke};
use lsty_core::list::{self, Params, Snapshot};
use lsty_core::lock::Cancel;
use lsty_core::todotxt::File;
use ratatui::buffer::Buffer;

use crate::columns::Natural;
use crate::deps::Deps;
use crate::filters;
use crate::focus::Focus;
use crate::hints;
use crate::msg::{Cmd, Msg};
use crate::panel::Panel;
use crate::reldate::Today;
use crate::rows::{self, Context, Row, Shows};
use crate::styles::Styles;

const NANOS_PER_MINUTE: i128 = 60_000_000_000;

/// The screen of the list, and what it holds between key presses.
pub struct Model {
    pub(crate) deps: Deps,
    pub(crate) clock: Clock,
    pub(crate) snap: Snapshot,
    pub(crate) styles: Styles,
    pub(crate) shows: Shows,
    pub(crate) mode: Mode,
    matchers: BTreeMap<Mode, Matcher>,
    pub(crate) rows: Vec<Row>,
    pub(crate) natural: Natural,
    pub(crate) cursor: usize,
    pub(crate) top: usize,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) message: Option<String>,
    pub(crate) sidebar_on: bool,
    pub(crate) detail_on: bool,
    pub(crate) overlay: bool,
    pub(crate) focus: Focus,
    pub(crate) last: Focus,
    pub(crate) panels: [Panel; 3],
    pub(crate) catalog: Vec<filters::Entry>,
    warnings: Vec<Warning>,
    shown: usize,
}

/// A reading of time that takes nothing from the environment.
fn no_vars(_: &str) -> Option<std::ffi::OsString> {
    None
}

impl Model {
    /// The model of the list in `deps.found`, read once.
    ///
    /// # Errors
    ///
    /// The clock that is out of range, a lock that is not free in time or
    /// a file that cannot be read.
    pub fn new(deps: Deps) -> errs::Result<Self> {
        let vars: &Vars<'_> = &no_vars;
        let clock = Clock::new(vars, (deps.now)(), deps.zone.clone())?;
        let snap = load(&deps, &clock)?;
        let styles = Styles::new(&deps.theme, deps.profile);
        let mut warnings = deps.warnings.clone();
        warnings.extend(hints::warnings(&deps.config.hints));
        let (catalog, more) = filters::catalog(&deps.config);
        warnings.extend(more);
        let mut model = Self {
            sidebar_on: deps.config.show_sidebar,
            detail_on: deps.config.show_detail,
            overlay: false,
            focus: Focus::List,
            last: Focus::Projects,
            panels: Self::new_panels(),
            catalog,
            shows: Shows::new(&deps.config),
            matchers: matchers(&deps),
            deps,
            clock,
            snap,
            styles,
            mode: Mode::Normal,
            rows: Vec::new(),
            natural: Natural::default(),
            cursor: 0,
            top: 0,
            width: 0,
            height: 0,
            message: None,
            warnings,
            shown: 0,
        };
        model.rebuild();
        model.next_warning();
        Ok(model)
    }

    /// What to do before the first message: wait for the next minute.
    pub fn init(&self) -> Vec<Cmd> {
        vec![self.next_minute()]
    }

    /// Handles one message.
    pub fn update(&mut self, msg: Msg) -> Vec<Cmd> {
        match msg {
            Msg::Resize(w, h) => {
                self.resize(w, h);
                Vec::new()
            }
            Msg::Key(stroke) => self.press(stroke),
            Msg::Minute => self.on_minute(),
        }
    }

    /// Draws the screen into `buf`, which has the size of the terminal.
    pub fn view(&self, buf: &mut Buffer) {
        self.draw(buf);
    }

    /// The name of the todo file as the status line shows it.
    pub(crate) fn file_name(&self) -> String {
        let name = self
            .deps
            .found
            .todo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if self.deps.found.state == FileState::Exists {
            name
        } else {
            format!("[New] {name}")
        }
    }

    /// The name of the todo file.
    pub(crate) fn base_name(&self) -> String {
        self.deps
            .found
            .todo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn press(&mut self, stroke: Stroke) -> Vec<Cmd> {
        self.message = None;
        let cmds = self.feed(stroke);
        if self.message.is_none() {
            self.next_warning();
        }
        cmds
    }

    fn feed(&mut self, stroke: Stroke) -> Vec<Cmd> {
        let Some(matcher) = self.matchers.get_mut(&self.key_mode()) else {
            return Vec::new();
        };
        match matcher.feed(stroke).outcome {
            Outcome::Matched { action, count } => {
                let count = count.map_or(0, |c| {
                    usize::try_from(c.get()).unwrap_or(usize::MAX)
                });
                self.run(action, count)
            }
            Outcome::Pending | Outcome::NoMatch => Vec::new(),
        }
    }

    /// Does an action. Those that a later part builds do nothing.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "most actions have no effect yet"
    )]
    fn run(&mut self, action: Action, count: usize) -> Vec<Cmd> {
        match action {
            Action::Quit => return vec![Cmd::Quit],
            Action::CursorDown
            | Action::CursorUp
            | Action::GoTop
            | Action::GoBottom
            | Action::HalfPageDown
            | Action::HalfPageUp => self.motion(action, count),
            Action::FocusNext
            | Action::FocusPrev
            | Action::FocusLeft
            | Action::FocusRight
            | Action::FocusDown
            | Action::FocusUp => self.move_focus(action),
            Action::ToggleSidebar => self.toggle_sidebar(),
            Action::ToggleDetail => self.toggle_detail(),
            Action::Fold => self.fold(),
            Action::Unfold => self.unfold(),
            _ => {}
        }
        Vec::new()
    }

    /// Shows the next config warning, as the message.
    fn next_warning(&mut self) {
        let Some(w) = self.warnings.get(self.shown) else {
            return;
        };
        let text = format!("{}: {}", w.source, w.message);
        self.shown += 1;
        let total = self.warnings.len();
        self.message = Some(if total > 1 {
            format!("{text} ({} of {total})", self.shown)
        } else {
            text
        });
    }

    pub(crate) fn rebuild(&mut self) {
        let ctx = self.context();
        let place = self.cursor_place();
        let offset = self.cursor.saturating_sub(self.top);
        self.rows = rows::build(&self.snap, self.shows, ctx);
        self.natural = Natural::of(&self.rows);
        self.rebuild_panels();
        self.follow(place, offset);
        self.scroll_panels();
    }

    fn context(&self) -> Context {
        Context {
            today: Today {
                date: self.clock.today(),
                soon: self.deps.config.upcoming_days,
            },
            now: self.clock.now(),
        }
    }

    /// The wait until the next minute of the clock starts.
    fn next_minute(&self) -> Cmd {
        let nanos = (self.deps.now)().as_nanosecond();
        let wait = NANOS_PER_MINUTE - nanos.rem_euclid(NANOS_PER_MINUTE);
        Cmd::After {
            delay: Duration::from_nanos(u64::try_from(wait).unwrap_or(0)),
            msg: Msg::Minute,
        }
    }

    /// Derives the list again for the new time.
    fn on_minute(&mut self) -> Vec<Cmd> {
        let next = vec![self.next_minute()];
        let vars: &Vars<'_> = &no_vars;
        let Ok(clock) =
            Clock::new(vars, (self.deps.now)(), self.deps.zone.clone())
        else {
            return next;
        };
        let params = Params {
            clock: clock.clone(),
            stale_after: self.deps.config.claim_stale_after,
        };
        let placeholder = Snapshot::build(
            self.deps.found.clone(),
            File::default(),
            File::default(),
            &params,
        );
        let old = std::mem::replace(&mut self.snap, placeholder);
        let (found, todo, done) = old.into_files();
        self.snap = Snapshot::build(found, todo, done, &params);
        self.clock = clock;
        self.rebuild();
        next
    }
}

fn matchers(deps: &Deps) -> BTreeMap<Mode, Matcher> {
    Mode::ALL
        .into_iter()
        .map(|mode| {
            let matcher = match mode {
                Mode::Completion => {
                    deps.keys.layered(Mode::Completion, Mode::Insert)
                }
                Mode::Normal
                | Mode::Visual
                | Mode::Insert
                | Mode::Command
                | Mode::Panel
                | Mode::Help
                | Mode::Picker => deps.keys.matcher(mode),
            };
            (mode, matcher)
        })
        .collect()
}

/// Reads both files under shared locks. A folder that is missing has
/// nothing to lock: the list is empty.
fn load(deps: &Deps, clock: &Clock) -> errs::Result<Snapshot> {
    let params = Params {
        clock: clock.clone(),
        stale_after: deps.config.claim_stale_after,
    };
    if deps.found.state == FileState::DirMissing {
        return Ok(Snapshot::build(
            deps.found.clone(),
            File::default(),
            File::default(),
            &params,
        ));
    }
    list::load(
        &deps.found,
        deps.config.lock_timeout,
        &Cancel::new(),
        &params,
    )
}

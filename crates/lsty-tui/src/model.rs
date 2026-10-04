//! The model: the list as loaded, the cursor and the status.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use lsty_core::config::Warning;
use lsty_core::dates::Clock;
use lsty_core::discover::FileState;
use lsty_core::errs::{self, Source};
use lsty_core::keys::{Action, Matcher, Mode, Outcome, Stroke};
use lsty_core::list::{Params, Snapshot, SortField};
use lsty_core::lock::Cancel;
use lsty_core::ops::UndoRecord;
use lsty_core::todotxt::File;
use ratatui::buffer::Buffer;

use crate::columns::Natural;
use crate::deps::Deps;
use crate::field::Field;
use crate::filters;
use crate::focus::Focus;
use crate::hints;
use crate::live::{self, Live};
use crate::messages::Tone;
use crate::msg::{Cmd, Msg};
use crate::panel::Panel;
use crate::prompt::{Kind as PromptKind, Prompt};
use crate::reldate::Today;
use crate::rows::{self, Arrange, Context, Row, Shows};
use crate::sticky::Sticky;
use crate::styles::Styles;
use crate::watch::Sink;

const NANOS_PER_MINUTE: i128 = 60_000_000_000;

/// The screen of the list, and what it holds between key presses.
pub struct Model {
    pub(crate) deps: Deps,
    pub(crate) clock: Clock,
    pub(crate) snap: Snapshot,
    pub(crate) styles: Styles,
    pub(crate) shows: Shows,
    pub(crate) mode: Mode,
    pub(crate) matchers: BTreeMap<Mode, Matcher>,
    pub(crate) rows: Vec<Row>,
    /// The order of the rows; `File` is the manual order.
    pub(crate) sort: SortField,
    /// The items shown though the view does not match them.
    pub(crate) sticky: Sticky,
    pub(crate) natural: Natural,
    pub(crate) cursor: usize,
    pub(crate) top: usize,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) message: Option<String>,
    /// How the message is drawn.
    pub(crate) tone: Tone,
    /// What the writes of this session did, oldest first.
    pub(crate) undo: Vec<UndoRecord>,
    pub(crate) sidebar_on: bool,
    pub(crate) detail_on: bool,
    pub(crate) overlay: bool,
    pub(crate) focus: Focus,
    pub(crate) last: Focus,
    pub(crate) panels: [Panel; 3],
    pub(crate) catalog: Vec<filters::Entry>,
    /// The search that was kept.
    pub(crate) search: String,
    /// The text of the search prompt while it is open.
    pub(crate) input: Option<Field>,
    /// The prompt that the text belongs to; none for the search.
    pub(crate) prompt: Option<Prompt>,
    pub(crate) live: Live,
    /// Ends a wait for a lock when the program is asked to stop.
    pub(crate) cancel: Cancel,
    warnings: Vec<Warning>,
    shown: usize,
}

impl Model {
    /// The model of the list in `deps.found`, read once.
    ///
    /// # Errors
    ///
    /// The clock that is out of range, a lock that is not free in time or
    /// a file that cannot be read.
    pub fn new(deps: Deps) -> errs::Result<Self> {
        Self::with_sink(deps, Arc::new(|_| {}))
    }

    /// Like [`Model::new`], with the sink that the watch hands its
    /// notices to; the caller turns them into [`Msg::Notice`].
    ///
    /// # Errors
    ///
    /// As for [`Model::new`].
    pub fn with_sink(deps: Deps, sink: Sink) -> errs::Result<Self> {
        let clock = live::clock_of(&deps)?;
        let cancel = Cancel::new();
        let loaded = live::fetch(&deps.found, &deps.config, &clock, &cancel)?;
        let mut deps = deps;
        deps.found = loaded.snap.found().clone();
        let gone = deps.found.state == FileState::DirMissing;
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
            search: String::new(),
            input: None,
            prompt: None,
            shows: Shows::new(&deps.config),
            matchers: matchers(&deps),
            deps,
            clock,
            snap: loaded.snap,
            live: Live::new(loaded.stamp, gone),
            cancel,
            styles,
            mode: Mode::Normal,
            rows: Vec::new(),
            sort: SortField::File,
            sticky: Sticky::default(),
            natural: Natural::default(),
            cursor: 0,
            top: 0,
            width: 0,
            height: 0,
            message: None,
            tone: Tone::Notice,
            undo: Vec::new(),
            warnings: Vec::new(),
            shown: 0,
        };
        warnings.extend(model.start_filter());
        model.warnings = warnings;
        model.rebuild();
        model.start_watch(sink);
        if model.message.is_none() {
            model.next_warning();
        }
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
            Msg::Reload => self.reload().into_iter().collect(),
            Msg::Notice(notice) => self.on_notice(notice),
            Msg::Loaded(reading) => self.on_loaded(*reading),
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
        if self.live.file_exists() {
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
        self.tone = Tone::Notice;
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
        let fed = matcher.feed(stroke);
        if self.input.is_some() {
            self.prompt_keys(fed);
            return Vec::new();
        }
        match fed.outcome {
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
            Action::ToggleEntry => self.toggle_entry(),
            Action::SelectAndJump => self.select_and_jump(),
            Action::Search => self.open_search(),
            Action::SetDue => self.open_prompt_on(PromptKind::SetDue, count),
            Action::AddTag => self.open_prompt_on(PromptKind::AddTag, count),
            Action::RemoveTag => {
                self.open_prompt_on(PromptKind::RemoveTag, count);
            }
            Action::CycleSort => self.cycle_sort(),
            Action::MoveItemDown | Action::MoveItemUp => {
                self.move_item(action, count);
            }
            Action::CycleState
            | Action::Stop
            | Action::ToggleComplete
            | Action::Delete
            | Action::CyclePriority
            | Action::CyclePriorityBack => self.act(action, count),
            _ => {}
        }
        Vec::new()
    }

    /// Opens the prompt of `kind` on the item under the cursor, or
    /// `count` items from it.
    fn open_prompt_on(&mut self, kind: PromptKind, count: usize) {
        let items = self.picked(count);
        self.open_prompt(kind, &items);
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

    /// Lists the rows and the names again, keeping the cursor row where
    /// it is on the screen.
    pub(crate) fn rebuild(&mut self) {
        let place = self.cursor_place();
        let offset = self.cursor.saturating_sub(self.top);
        self.rebuild_at(place, offset);
    }

    /// Lists the rows and the names again, with the cursor on the first
    /// row at or after `place` and `offset` rows below the top.
    pub(crate) fn rebuild_at(
        &mut self,
        place: Option<(Source, u32)>,
        offset: usize,
    ) {
        self.rebuild_rows();
        self.rebuild_panels();
        self.follow(place, offset);
        self.scroll_panels();
    }

    /// Lists the rows and the names again after a filter changed. The
    /// view stays where it is unless the cursor row would leave it.
    pub(crate) fn refresh(&mut self) {
        let place = self.cursor_place();
        self.sticky.clear();
        self.rebuild_rows();
        self.rebuild_panels();
        self.settle_cursor(place);
        self.scroll_panels();
    }

    /// Lists the rows again for a new search: the names and their counts
    /// stay as they are.
    pub(crate) fn reshow(&mut self) {
        let place = self.cursor_place();
        self.sticky.clear();
        self.rebuild_rows();
        self.settle_cursor(place);
    }

    fn rebuild_rows(&mut self) {
        let ctx = self.context();
        let how = Arrange {
            sort: self.sort,
            sticky: &self.sticky,
        };
        self.rows = rows::build(&self.snap, &self.scope(), ctx, how);
        self.natural = Natural::of(&self.rows);
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
        let mut next = vec![self.next_minute()];
        let Ok(clock) = self.clock_now() else {
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
        next.extend(self.reload_owed());
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

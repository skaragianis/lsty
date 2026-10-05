//! The model: the list as loaded, the cursor and the status.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use lsty_core::config::Warning;
use lsty_core::dates::Clock;
use lsty_core::discover::FileState;
use lsty_core::errs;
use lsty_core::keys::{Action, Matcher, Mode, Outcome, Stroke};
use lsty_core::list::{Params, Snapshot, SortField};
use lsty_core::lock::Cancel;
use lsty_core::todotxt::File;
use ratatui::buffer::Buffer;

use crate::candidates::Sources;
use crate::columns::Natural;
use crate::deps::Deps;
use crate::filters;
use crate::focus::Focus;
use crate::hints;
use crate::history::{Again, History};
use crate::insert::Draft;
use crate::line::Line;
use crate::live::{self, Live};
use crate::messages::Tone;
use crate::mouse::Pointer;
use crate::msg::{Cmd, Msg};
use crate::output::Output;
use crate::panel::Panel;
use crate::picker::Picker;
use crate::prompt::Kind as PromptKind;
use crate::reldate::Today;
use crate::rows::{self, Arrange, Context, Row, Shows};
use crate::session::Session;
use crate::sticky::Sticky;
use crate::store::Store;
use crate::styles::Styles;
use crate::track::Place;
use crate::watch::Sink;
use crate::yank::Yank;

const NANOS_PER_MINUTE: i128 = 60_000_000_000;

/// The screen of the list, and what it holds between key presses.
pub struct Model {
    pub(crate) deps: Deps,
    pub(crate) clock: Clock,
    pub(crate) snap: Snapshot,
    pub(crate) styles: Styles,
    pub(crate) shows: Shows,
    pub(crate) matchers: BTreeMap<Mode, Matcher>,
    pub(crate) rows: Vec<Row>,
    /// The order of the rows; `File` is the manual order.
    pub(crate) sort: SortField,
    /// The items shown though the view does not match them.
    pub(crate) sticky: Sticky,
    pub(crate) natural: Natural,
    pub(crate) cursor: usize,
    /// The line the selection started on, in VISUAL mode.
    pub(crate) visual: Option<Place>,
    pub(crate) top: usize,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) message: Option<String>,
    /// How the message is drawn.
    pub(crate) tone: Tone,
    /// What the writes of this session did, oldest first.
    pub(crate) hist: History,
    /// The last change, for `.`.
    pub(crate) again: Option<Again>,
    pub(crate) sidebar_on: bool,
    pub(crate) detail_on: bool,
    pub(crate) overlay: bool,
    pub(crate) focus: Focus,
    pub(crate) last: Focus,
    pub(crate) panels: [Panel; 3],
    pub(crate) catalog: Vec<filters::Entry>,
    /// The search that was kept.
    pub(crate) search: String,
    /// The text line of the status line while it is open.
    pub(crate) line: Option<Line>,
    /// What a command of the `:` line showed, over the list.
    pub(crate) output: Option<Output>,
    /// The links of an item, while the popup asks which to open.
    pub(crate) opening: Option<Picker>,
    /// The line being edited in INSERT mode.
    pub(crate) edit: Option<Draft>,
    /// The draft that was thrown away last, which `u` brings back.
    pub(crate) kept: Option<Draft>,
    /// What the popup offers, collected for the reading it was made on.
    pub(crate) sources: Option<Sources>,
    pub(crate) live: Live,
    /// What the mouse has done that later events need.
    pub(crate) pointer: Pointer,
    /// The histories and folds that last between launches.
    pub(crate) session: Session,
    /// Ends a wait for a lock when the program is asked to stop.
    pub(crate) cancel: Cancel,
    /// The last yank, which a paste puts back.
    pub(crate) clip: Option<Yank>,
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
        let found = loaded.snap.found().clone();
        let deps = Deps { found, ..deps };
        let gone = deps.found.state == FileState::DirMissing;
        let styles = Styles::new(&deps.theme, deps.profile);
        let (catalog, more) = filters::catalog(&deps.config);
        let hints = hints::warnings(&deps.config.hints);
        let warnings = [deps.warnings.clone(), hints, more].concat();
        let mut model = Self {
            sidebar_on: deps.config.show_sidebar,
            detail_on: deps.config.show_detail,
            overlay: false,
            focus: Focus::List,
            last: Focus::Projects,
            panels: Self::new_panels(),
            catalog,
            search: String::new(),
            line: None,
            output: None,
            opening: None,
            edit: None,
            kept: None,
            sources: None,
            shows: Shows::new(&deps.config),
            matchers: matchers(&deps),
            session: Session::new(Store::new(deps.state_dir.clone())),
            deps,
            clock,
            snap: loaded.snap,
            live: Live::new(loaded.stamp, gone),
            pointer: Pointer::default(),
            cancel,
            clip: None,
            styles,
            rows: Vec::new(),
            sort: SortField::File,
            sticky: Sticky::default(),
            natural: Natural::default(),
            cursor: 0,
            visual: None,
            top: 0,
            width: 0,
            height: 0,
            message: None,
            tone: Tone::Notice,
            hist: History::default(),
            again: None,
            warnings: Vec::new(),
            shown: 0,
        };
        model.start(sink, warnings);
        Ok(model)
    }

    /// The steps after the model exists: the filter it starts with, the
    /// folds, the first rows, the watch and the first warning.
    fn start(&mut self, sink: Sink, mut warnings: Vec<Warning>) {
        warnings.extend(self.start_filter());
        self.warnings = warnings;
        self.start_folds();
        self.rebuild();
        self.start_watch(sink);
        if self.message.is_none() {
            self.next_warning();
        }
    }

    /// What to do before the first message: wait for the next minute.
    pub fn init(&self) -> Vec<Cmd> {
        vec![self.next_minute()]
    }

    /// Handles one message.
    pub fn update(&mut self, msg: Msg) -> Vec<Cmd> {
        let cmds = self.dispatch(msg);
        self.drop_peek();
        cmds
    }

    fn dispatch(&mut self, msg: Msg) -> Vec<Cmd> {
        match msg {
            Msg::Resize(w, h) => {
                self.resize(w, h);
                Vec::new()
            }
            Msg::Key(stroke) => self.press(stroke),
            Msg::Paste(text) => {
                self.paste(&text);
                Vec::new()
            }
            Msg::Mouse(event) => self.mouse(event),
            Msg::Minute => self.on_minute(),
            Msg::Reload => self.reload().into_iter().collect(),
            Msg::Notice(notice) => self.on_notice(notice),
            Msg::Loaded(reading) => self.on_loaded(*reading),
            Msg::Edited(result) => {
                self.editor_exited(result);
                Vec::new()
            }
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
        if self.edit.is_some() {
            self.edit_press(stroke);
            return Vec::new();
        }
        let Some(matcher) = self.matchers.get_mut(&self.key_mode()) else {
            return Vec::new();
        };
        let fed = matcher.feed(stroke);
        if self.opening.is_some() {
            return self.picker_keys(stroke, &fed);
        }
        if self.output.is_some() {
            self.output_keys(&fed);
            return Vec::new();
        }
        if self.line.is_some() {
            return self.prompt_keys(fed);
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
    pub(crate) fn run(&mut self, action: Action, count: usize) -> Vec<Cmd> {
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
            Action::EnterVisual => self.enter_visual(),
            Action::ExitVisual => self.exit_visual(),
            Action::Search => self.open_search(),
            Action::CommandLine => self.open_command_line(),
            Action::EditDescription
            | Action::EditAppend
            | Action::EditLineStart
            | Action::ChangeDescription => self.open_edit(action),
            Action::InsertBelow => self.open_new(true),
            Action::InsertAbove => self.open_new(false),
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
            | Action::CyclePriority
            | Action::CyclePriorityBack => {
                self.act(action, count);
            }
            Action::Delete => return self.delete_items(count),
            Action::Yank => return self.yank(count),
            Action::PasteBelow | Action::PasteAbove => {
                self.paste_items(action);
            }
            Action::OpenLink => return self.open_link(count),
            Action::Help => self.open_help(),
            Action::Undo => self.undo_last(),
            Action::Redo => self.redo_last(),
            Action::Repeat => self.repeat(count),
            _ => {}
        }
        Vec::new()
    }

    /// Opens the prompt of `kind` on the item under the cursor, or
    /// `count` items from it.
    fn open_prompt_on(&mut self, kind: PromptKind, count: usize) {
        let items = self.items_of(count);
        let count = self.count_for(count, items.len());
        self.open_prompt(kind, &items, count);
    }

    /// Shows the next config warning, as the message.
    pub(crate) fn next_warning(&mut self) {
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
    pub(crate) fn rebuild_at(&mut self, place: Option<Place>, offset: usize) {
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

    pub(crate) fn context(&self) -> Context {
        Context {
            today: Today {
                date: self.clock.today(),
                soon: self.deps.config.upcoming_days,
            },
            now: self.clock.now(),
        }
    }

    /// The mode the status line names: what is open decides.
    pub(crate) const fn mode(&self) -> Mode {
        if self.line.is_some() {
            Mode::Command
        } else if self.edit.is_some() {
            Mode::Insert
        } else if self.visual.is_some() {
            Mode::Visual
        } else {
            Mode::Normal
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

    /// Builds the state of the files again as the options now have it.
    pub(crate) fn derive_again(&mut self) {
        self.derive_again_at(self.clock.clone());
    }

    /// Builds the state of the files again for `clock`.
    fn derive_again_at(&mut self, clock: Clock) {
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
    }

    /// Derives the list again for the new time.
    fn on_minute(&mut self) -> Vec<Cmd> {
        let mut next = vec![self.next_minute()];
        let Ok(clock) = self.clock_now() else {
            return next;
        };
        self.derive_again_at(clock);
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

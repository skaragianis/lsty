//! The model: the list as loaded, the cursor and the status.

use std::{collections::BTreeMap, time::Duration};

use lsty_core::{
    config::Warning,
    dates::Clock,
    discover::FileState,
    errs,
    keys::{Action, Category, Matcher, Mode, Outcome, Stroke},
    list::{Params, Snapshot, SortField},
    lock::Cancel,
    theme::Theme,
    todotxt::File,
};

use crate::{
    candidates::Sources,
    chips::title_path,
    columns::Natural,
    deps::{Deps, Start},
    filters,
    focus::Focus,
    hints,
    history::{Again, History},
    insert::Draft,
    line::Line,
    live::{self, Live},
    messages::Tone,
    motion::{Step, View},
    mouse::Pointer,
    msg::{Cmd, Msg},
    output::Output,
    panel::Panel,
    picker::Picker,
    prompt::Kind as PromptKind,
    reldate::Today,
    rows::{self, Context, Inline, Row, Shows},
    session::Session,
    sticky::Sticky,
    store::Store,
    styles::Styles,
    themes::ThemePicker,
    track::Place,
    watch::{Sink, Watch},
    yank::Yank,
};

const NANOS_PER_MINUTE: i128 = 60_000_000_000;

/// How long a message stays in the status line.
const MESSAGE_LIFE: Duration = Duration::from_secs(2);

/// The screen of the list, and what it holds between key presses.
pub(crate) struct Model {
    pub(crate) deps: Deps,
    pub(crate) clock: Clock,
    /// The list shown. Replaced only by `install` (with what the files
    /// held) or rebuilt from its own files, so a reading can be compared
    /// with it.
    pub(crate) snap: Snapshot,
    /// The path of the todo file as the list title shows it.
    pub(crate) path: String,
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
    /// Counts the messages put up and cleared, so that a timer clears
    /// only the message it was set for.
    said: u64,
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
    /// The themes, while the popup of `:theme` offers them.
    pub(crate) choosing: Option<ThemePicker>,
    /// The theme in use before the `:` line or the popup of `:theme`
    /// showed another.
    pub(crate) previewed: Option<Theme>,
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
    /// [`Model::with_sink`] with a sink that drops every notice.
    #[cfg(any(test, feature = "harness"))]
    pub(crate) fn new(start: Start, deps: Deps) -> errs::Result<Self> {
        Self::with_sink(start, deps, std::sync::Arc::new(|_| {}))
    }

    /// The model of the list in `start.found`, read once, with the sink
    /// that the watch hands its notices to; the caller turns them into
    /// [`Msg::Notice`].
    ///
    /// # Errors
    ///
    /// The clock that is out of range, a lock that is not free in time or
    /// a file that cannot be read.
    pub(crate) fn with_sink(
        start: Start,
        deps: Deps,
        sink: Sink,
    ) -> errs::Result<Self> {
        let clock = live::clock_in(&deps.now, &deps.zone)?;
        let cancel = Cancel::new();
        let snap = live::fetch(&start.found, &deps.config, &clock, &cancel)?;
        let styles = Styles::new(&deps.theme, deps.profile);
        let (catalog, more) = filters::catalog(&deps.config);
        let hints = hints::warnings(&deps.config.hints);
        let warnings = [start.warnings, hints, more].concat();
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
            choosing: None,
            previewed: None,
            edit: None,
            kept: None,
            sources: None,
            shows: Shows::new(&deps.config),
            matchers: matchers(&deps),
            session: Session::new(Store::new(start.state_dir)),
            path: title_path(&start.found.todo, deps.home.as_deref()),
            deps,
            clock,
            live: Live::new(&snap),
            snap,
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
            said: 0,
            tone: Tone::Notice,
            hist: History::default(),
            again: None,
            warnings: Vec::new(),
            shown: 0,
        };
        model.start(start.watch, sink, warnings);
        Ok(model)
    }

    /// The steps after the model exists: the filter it starts with, the
    /// folds, the first rows, the watch and the first warning.
    fn start(
        &mut self,
        watch: Option<Watch>,
        sink: Sink,
        mut warnings: Vec<Warning>,
    ) {
        warnings.extend(self.start_filter());
        self.warnings = warnings;
        self.start_folds();
        self.rebuild();
        self.start_watch(watch, sink);
        if self.message.is_none() {
            self.next_warning();
        }
    }

    /// What to do before the first message: wait for the next minute.
    pub(crate) fn init(&self) -> Vec<Cmd> {
        vec![self.next_minute()]
    }

    /// Handles one message. A notice that the human's own input put in
    /// the status line is cleared after a while; errors and others stay
    /// until the next key.
    pub(crate) fn update(&mut self, msg: Msg) -> Vec<Cmd> {
        if matches!(msg, Msg::Mouse(_)) && !self.deps.config.mouse {
            return Vec::new();
        }
        let input = matches!(
            msg,
            Msg::Key(_) | Msg::Mouse(_) | Msg::Paste(_) | Msg::Edited(_)
        );
        let (said, before) = (self.said, self.message.clone());
        if input {
            self.catch_up_day();
            self.sticky.age();
        }
        let mut cmds = self.dispatch(msg);
        if input {
            self.leave_sticky();
        }
        self.drop_peek();
        if self.message.is_some()
            && (self.said != said || self.message != before)
        {
            self.said += 1;
            if input && self.line.is_none() && self.tone == Tone::Notice {
                cmds.push(Cmd::After {
                    delay: MESSAGE_LIFE,
                    msg: Msg::Expire(self.said),
                });
            }
        }
        cmds
    }

    /// Clears the message line before a key or a gesture acts.
    pub(crate) fn clear_message(&mut self) {
        self.said += 1;
        self.message = None;
        self.tone = Tone::Notice;
    }

    /// Clears the message the timer `said` was set for, unless another
    /// has been put up since.
    fn expire(&mut self, said: u64) {
        if said == self.said {
            self.clear_message();
            self.sticky.age();
            self.leave_sticky();
        }
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
            Msg::Expire(said) => {
                self.expire(said);
                Vec::new()
            }
            Msg::Reload => self.reload().into_iter().collect(),
            Msg::Notice(notice) => self.on_notice(notice),
            Msg::Loaded(reading) => self.on_loaded(*reading),
            Msg::Edited(result) => {
                if let Err(why) = result {
                    self.alert(format!("editor exited: {why}"));
                }
                Vec::new()
            }
        }
    }

    /// Whether the todo file was there at the last reading.
    pub(crate) fn file_exists(&self) -> bool {
        self.snap.found().state == FileState::Exists
    }

    /// The name of the todo file as the status line shows it.
    pub(crate) fn file_name(&self) -> String {
        let name = self.base_name();
        if self.file_exists() {
            name
        } else {
            format!("[New] {name}")
        }
    }

    /// The name of the todo file.
    pub(crate) fn base_name(&self) -> String {
        self.snap
            .found()
            .todo
            .file_name()
            .map(errs::lossy_path)
            .unwrap_or_default()
    }

    fn press(&mut self, stroke: Stroke) -> Vec<Cmd> {
        self.clear_message();
        let cmds = self.feed(stroke);
        if self.message.is_none() {
            self.next_warning();
        }
        cmds
    }

    fn feed(&mut self, stroke: Stroke) -> Vec<Cmd> {
        if self.edit.is_some() {
            self.reveal();
            self.edit_press(stroke);
            return Vec::new();
        }
        let Some(matcher) = self.matchers.get_mut(&self.key_mode()) else {
            return Vec::new();
        };
        let fed = matcher.feed(stroke);
        let on_view = matches!(fed.outcome, Outcome::Matched { action, .. }
            if Step::of(action, 0, 0).is_some_and(Step::works_on_view));
        if !on_view {
            self.reveal();
        }
        if self.opening.is_some() {
            return self.picker_keys(stroke, &fed);
        }
        if self.choosing.is_some() {
            self.theme_keys(&fed);
            return Vec::new();
        }
        if self.output.is_some() {
            self.output_keys(&fed);
            return Vec::new();
        }
        if self.line.is_some() {
            return self.prompt_keys(fed);
        }
        match fed.outcome {
            Outcome::Matched { action, count } => self.run(action, count),
            Outcome::Pending | Outcome::NoMatch => Vec::new(),
        }
    }

    /// Does an action of NORMAL or VISUAL mode. The actions of the other
    /// modes are handled where their mode is, and do nothing here.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the actions of the other modes have no effect here"
    )]
    pub(crate) fn run(&mut self, action: Action, count: usize) -> Vec<Cmd> {
        match action {
            Action::Quit => return vec![Cmd::Quit],
            motion if motion.is_motion() => self.motion(motion, count),
            moves if moves.category() == Category::Windows => {
                self.move_focus(moves);
            }
            Action::ToggleSidebar => self.toggle_sidebar(),
            Action::ToggleDetail => self.toggle_detail(),
            Action::Fold => self.fold(),
            Action::Unfold => self.unfold(),
            Action::ToggleEntry => {
                self.act_on_entry();
            }
            Action::SelectAndJump => self.select_and_jump(),
            Action::EnterVisual => self.enter_visual(),
            Action::ExitVisual => self.exit_visual(),
            Action::Search => self.open_search(),
            Action::ClearSearch => self.clear_search(),
            Action::CommandLine => self.open_command_line(),
            Action::EditDescription
            | Action::EditAppend
            | Action::EditLineStart
            | Action::ChangeDescription => self.open_edit(action),
            Action::InsertBelow => self.open_new(true),
            Action::InsertAbove => self.open_new(false),
            Action::SetDue => self.open_prompt(PromptKind::SetDue, count),
            Action::AddTag => self.open_prompt(PromptKind::AddTag, count),
            Action::RemoveTag => {
                self.open_prompt(PromptKind::RemoveTag, count);
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
            Action::OpenLink => return self.open_key(count),
            Action::NextLink | Action::PrevLink => self.tab_link(action),
            Action::Help => self.open_help(""),
            Action::Undo => self.undo_last(),
            Action::Redo => self.redo_last(),
            Action::Repeat => return self.repeat(count),
            _ => {}
        }
        Vec::new()
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
        self.rebuild_at(place, self.view_offset());
    }

    /// Whether the wheel has scrolled the cursor row out of the view.
    pub(crate) fn cursor_hidden(&self) -> bool {
        self.edit.is_none()
            && !self.rows.is_empty()
            && !(self.top..self.top + self.pane_rows()).contains(&self.cursor)
    }

    /// Where the view stays when the rows are listed again: the cursor
    /// row keeps its place in it, or, when the cursor row is out of view,
    /// the view keeps its top. When the view shows the last row it goes
    /// to the new end, so that rows added there come into view. A field
    /// open keeps the view where it is.
    pub(crate) fn view_offset(&self) -> View {
        let at_bottom = self.edit.is_none()
            && self.top + self.pane_rows() >= self.rows.len();
        if self.cursor_hidden() {
            View::Top(if at_bottom { usize::MAX } else { self.top })
        } else if at_bottom {
            View::Cursor(0)
        } else {
            View::Cursor(self.cursor.saturating_sub(self.top))
        }
    }

    /// Lists the rows and the names again, with the cursor on the first
    /// row at or after `place` and `offset` rows below the top.
    pub(crate) fn rebuild_at(&mut self, place: Option<Place>, offset: View) {
        self.rebuild_rows();
        self.rebuild_panels();
        self.follow(place, offset);
    }

    /// Lists the rows and the names again after a filter changed. The
    /// view stays where it is unless the cursor row would leave it.
    pub(crate) fn refresh(&mut self) {
        self.reshow();
        self.rebuild_panels();
        self.scroll_panels();
    }

    /// Lists the rows again for a new search: the names and their counts
    /// stay as they are.
    pub(crate) fn reshow(&mut self) {
        let place = self.drawn_cursor();
        self.sticky.clear();
        self.rebuild_rows();
        self.settle_cursor(place);
    }

    pub(crate) fn rebuild_rows(&mut self) {
        let ctx = self.context();
        self.rows = rows::build(
            &self.snap,
            &self.scope(),
            ctx,
            self.sort,
            &self.sticky,
        );
        self.natural = Natural::of(&self.rows);
    }

    pub(crate) fn context(&self) -> Context {
        Context {
            today: Today {
                date: self.clock.today(),
                soon: self.deps.config.upcoming_days,
            },
            now: self.clock.now(),
            inline: Inline::new(&self.deps.config),
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
            delay: Duration::from_nanos_u128(wait.unsigned_abs()),
            msg: Msg::Minute,
        }
    }

    /// Builds the state of the files again as the options now have it.
    pub(crate) fn derive_again(&mut self) {
        self.derive_again_at(self.clock.clone());
    }

    /// Builds the state of the files again for `clock`.
    fn derive_again_at(&mut self, clock: Clock) {
        let params = Params::new(clock.clone(), &self.deps.config);
        let placeholder = Snapshot::build(
            self.snap.found().clone(),
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

    /// Derives the list again when the day has changed since the last
    /// minute tick was handled, as after the machine slept.
    fn catch_up_day(&mut self) {
        if let Ok(clock) = self.clock_now()
            && clock.today() != self.clock.today()
        {
            self.derive_again_at(clock);
        }
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
                | Mode::Detail
                | Mode::Help
                | Mode::Picker => deps.keys.matcher(mode),
            };
            (mode, matcher)
        })
        .collect()
}

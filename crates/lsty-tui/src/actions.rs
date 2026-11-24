//! The keys that write: each one runs the single command it resolves to,
//! as the human.

use std::cell::Cell;

use lsty_cli::Places;
use lsty_core::{
    dates::Clock,
    errs::{self, Code, Msg, Nr, Part, Source},
    keys::Action,
    list::{Claim, Snapshot, State, TaskId},
    ops::{
        self, ClaimOpts, DoOpts, Env, Landing, Outcome, PriPair, StartOpts,
        Target,
    },
    todotxt::Priority,
};

use crate::{
    history::{Again, pairs_of},
    messages::{self, Tone},
    model::Model,
    press::{Answer, Command, Refusal},
    track::{Place, Touched, Wrote, place_of},
};

/// A task an action is about, as the list showed it.
pub(crate) struct Chosen {
    source: Source,
    nr: Nr,
    state: State,
    claim: Claim,
    name: String,
    priority: Option<Priority>,
}

impl Chosen {
    /// The reference to the line, which [`Model::attempt`] aims from the
    /// reading the list shows.
    pub(crate) fn target(&self) -> Target {
        Target::at(self.source, i64::from(self.nr.get()))
    }

    /// Whether `command` changes this task, rather than say it is already
    /// there.
    const fn changed_by(&self, command: Command) -> bool {
        match command {
            Command::Do | Command::Wontdo => {
                !matches!(self.state, State::Done)
            }
            Command::Start => {
                matches!(self.state, State::Todo)
                    || matches!(self.claim, Claim::Agent)
            }
            Command::Stop => matches!(self.state, State::Doing),
            Command::Reopen | Command::Delete | Command::Step { .. } => true,
        }
    }
}

impl Model {
    /// Does a task action over the task under the cursor, or `count`
    /// tasks from it.
    pub(crate) fn act(&mut self, action: Action, count: usize) -> bool {
        let answer = self.answers_at(count)(action);
        let ids = self.tasks_of(count);
        let tasks = self.chosen(&ids);
        if let Answer::Refused(why) = answer {
            self.refuse(why, &tasks);
            return false;
        }
        let again = self.count_for(count, tasks.len());
        let toggles = action == Action::ToggleStart;
        let stops = answer == Answer::Resolved(Command::Stop);
        let back = toggles && !stops && self.just_stopped(count);
        let selected = self.visual.is_some();
        let ran = self.ending(&ids, |m| {
            if back {
                m.put_back()
            } else {
                m.answered(answer, &tasks, selected)
            }
        });
        if ran {
            self.again = Some(Again::Task {
                action,
                count: again,
            });
            if toggles && stops {
                self.remember_stop();
            }
        }
        ran
    }

    /// Does what `answer` says over `tasks`. Tells whether a write ran.
    fn answered(
        &mut self,
        answer: Answer,
        tasks: &[Chosen],
        selected: bool,
    ) -> bool {
        let Some((command, passes)) = answer.runs() else {
            return false;
        };
        let changing: Vec<&Chosen> = tasks
            .iter()
            .filter(|task| !passes || task.changed_by(command))
            .collect();
        let there = tasks.len() - changing.len();
        // A completion goes on below its press, not to a task it passed over.
        let cursor = self.cursor;
        if there > 0 && matches!(command, Command::Do | Command::Wontdo) {
            self.cursor = (cursor + tasks.len()).min(self.rows.len() - 1);
        }
        let aimed: Vec<Target> = tasks
            .iter()
            .filter(|_| selected)
            .map(Chosen::target)
            .collect();
        let result = self.attempt(|env| {
            let env = Env {
                aimed: &aimed,
                dry: !passes,
                ..env.clone()
            };
            self.perform(&env, command, &changing)
        });
        // The messages that say where a task went are no line of the count.
        let moves = result.as_ref().map_or(0, |(outcome, _)| {
            outcome.said().iter().filter(|m| moved(m).is_some()).count()
        });
        let Some(said) = self.settle(result).filter(|_| passes) else {
            self.message.tone = Tone::Alert;
            self.cursor = cursor;
            return false;
        };
        if let Some((did, already)) = command.words().filter(|_| there > 0) {
            let head = shortened(&said, said.len() - moves, &changing, did);
            let already = format!("{} {already}", ops::counted(there, "task"));
            self.message.say(format!("{head}; {already}"));
        }
        self.scroll();
        true
    }

    /// Takes what an operation did, or shows why it did not run. Gives
    /// the messages shown when the operation ran.
    pub(crate) fn settle(
        &mut self,
        result: errs::Result<(Outcome, Clock)>,
    ) -> Option<Vec<String>> {
        match result {
            Ok((outcome, clock)) => Some(self.adopt(outcome, clock)),
            Err(e) => {
                self.fail(&e);
                None
            }
        }
    }

    pub(crate) fn chosen(&self, ids: &[TaskId]) -> Vec<Chosen> {
        ids.iter()
            .filter_map(|id| {
                let task = self.snap.task(*id)?;
                let line = self.snap.line(task);
                Some(Chosen {
                    source: task.source(),
                    nr: task.nr(),
                    state: task.state(),
                    claim: task.claim(),
                    name: messages::name_of(line),
                    priority: line.effective_priority(),
                })
            })
            .collect()
    }

    /// Runs `run` over an environment of the current files, as the
    /// human, aimed from the reading the list shows; the clock is the one
    /// it used.
    pub(crate) fn attempt<T>(
        &self,
        run: impl FnOnce(&Env<'_>) -> errs::Result<T>,
    ) -> errs::Result<(T, Clock)> {
        let clock = self.clock_now()?;
        // A reading of an earlier refusal in this input is out of date.
        self.live.read.take();
        let mut env = Env::new(self.snap.found(), &self.deps.config, &clock);
        env.create = true;
        env.record_undo = true;
        env.cancel = self.cancel.clone();
        env.ids = self.deps.ids.as_ref();
        env.aim = Some(&self.snap);
        let read = Cell::new(None);
        env.read_back = Some(&read);
        let ran = run(&env);
        if let Some(snap) = read.take() {
            self.live.read.set(Some((snap, clock.clone())));
        }
        Ok((ran?, clock))
    }

    /// Runs `command` over `tasks`.
    fn perform(
        &self,
        env: &Env<'_>,
        command: Command,
        tasks: &[&Chosen],
    ) -> errs::Result<Outcome> {
        let targets: Vec<Target> = tasks.iter().map(|t| t.target()).collect();
        let claim = ClaimOpts::default();
        match command {
            Command::Do => ops::finish(env, &targets, DoOpts::default()),
            Command::Wontdo => {
                let opts = DoOpts {
                    wontdo: true,
                    ..DoOpts::default()
                };
                ops::finish(env, &targets, opts)
            }
            Command::Start => ops::start(env, &targets, StartOpts::default()),
            Command::Stop => ops::stop(env, &targets, claim),
            Command::Reopen if tasks[0].source == Source::Done => {
                ops::restore(env, &targets, claim)
            }
            Command::Reopen => ops::reopen(env, &targets, claim),
            Command::Delete => ops::del(env, &targets, claim),
            Command::Step { forward } => {
                let last = self.deps.config.priority_range;
                let pairs =
                    tasks.iter().zip(targets).map(|(task, target)| PriPair {
                        target,
                        priority: stepped(task.priority, last, forward),
                    });
                ops::pri(env, &pairs.collect::<Vec<_>>())
            }
        }
    }

    /// Takes the files an operation wrote as the state shown, keeps its
    /// undo record and shows its messages, which it gives back.
    pub(crate) fn adopt(
        &mut self,
        outcome: Outcome,
        clock: Clock,
    ) -> Vec<String> {
        self.adopt_then(outcome, clock, |_| ())
    }

    /// `adopt`, running `then` once the list is installed and before the
    /// messages name its rows.
    pub(crate) fn adopt_then(
        &mut self,
        mut outcome: Outcome,
        clock: Clock,
        then: impl FnOnce(&mut Self),
    ) -> Vec<String> {
        let was = self.accept_read(outcome.read(), &clock);
        let msgs = outcome.said().to_vec();
        if outcome.no_op() {
            then(self);
            let said = self.rendered(&msgs, &was);
            self.message.put(join(&said));
            self.live.no_op = Some((msgs, said.join("; ")));
            return said;
        }
        let touched = touched(&outcome);
        let record = outcome.take_undo();
        let snap = outcome.into_after();
        let wrote = record.as_ref().map(|record| Wrote {
            record,
            touched: &touched,
            undone: false,
        });
        let from = self.cursor_place();
        let landed = self.install(snap, clock, wrote.as_ref());
        if let Some(record) = &record {
            self.keep_links(record);
        }
        then(self);
        let said = self.rendered(&msgs, &was);
        self.message.put(join(&said));
        if !landed {
            self.stick_cursor(&touched, from);
        }
        self.keep_sticky(&touched);
        if let Some(record) = record {
            let pairs = pairs_of(&record, &touched);
            self.hist.record(record, msgs.into_iter().next(), pairs);
        }
        said
    }

    /// Shows `read`, the list an operation read under the lock, when it is
    /// not the one shown. Gives the rows its lines had on the screen at the
    /// press, which a message names a task's earlier row by; a line the
    /// screen did not show has the row it has in `read`.
    pub(crate) fn accept_read(
        &mut self,
        read: &Snapshot,
        clock: &Clock,
    ) -> Places {
        if read.serial() == self.snap.serial() {
            return self.lines();
        }
        let (shown, landing) = (self.lines(), Landing::of(&self.snap, read));
        self.accept(read.clone(), clock.clone());
        let mut was = self.lines();
        for ((source, nr), row) in shown {
            let to = landing.line(source, nr.index()).and_then(Nr::from_index);
            was.extend(to.map(|to| ((source, to), row)));
        }
        was
    }

    /// Names the tasks of the message of a refusal by their places in the
    /// list shown, which is the one the core read.
    pub(crate) fn name_refusal(&mut self) {
        if let Some((said, tail)) = self.live.refused.take() {
            self.release_sticky();
            let text = self.rendered(&[said], &self.lines());
            self.message.edit(|_| format!("{}{tail}", text[0]));
        }
    }

    /// Names the tasks of a message of a write that changed nothing by the
    /// rows left once the rows kept in sight went, if it still shows.
    pub(crate) fn rename_no_op(&mut self) {
        let Some((msgs, said)) = self.live.no_op.take() else {
            return;
        };
        if self.message.text() == Some(said.as_str()) {
            let said = self.rendered(&msgs, &self.lines());
            self.message.edit(|_| said.join("; "));
        }
    }

    /// The place each shown task has, by its line: the number its row has in
    /// the column.
    pub(crate) fn lines(&self) -> Places {
        let rows = self.rows.iter().enumerate();
        rows.map(|(i, row)| (row.place(), self.place(i))).collect()
    }

    /// `msgs` with each task named by its place in the list shown, or in
    /// `was` when the write removed it; a task with no row by its `id:`,
    /// else its line. The clause that says where a task went (archived or
    /// moved) is left out when it shows at the row it had.
    pub(crate) fn rendered(&self, msgs: &[Msg], was: &Places) -> Vec<String> {
        let now = self.lines();
        let row = |source, place| match place? {
            errs::Place::Written(nr) => now.get(&(source, nr)),
            errs::Place::Read(nr) => was.get(&(source, nr)),
        };
        // A task the write only adds is the field open for it.
        let name = |name: &errs::Name| match name.place {
            None if self.new_field() => (self.field_at() + 1).to_string(),
            place => row(name.source, place)
                .map_or_else(|| unshown(name), usize::to_string),
        };
        let text = |msg: &Msg| errs::lossy(&msg.render(name));
        msgs.iter()
            .map(|msg| match moved(msg) {
                Some(((source, place), to, said))
                    if row(source, Some(place)).is_some()
                        && row(source, Some(place))
                            == row(to.source, to.place) =>
                {
                    text(&msg.without_last(2).then(said))
                }
                _ => text(msg),
            })
            .collect()
    }

    /// Whether the rule refuses `action` over the tasks of `count`, having
    /// said why.
    pub(crate) fn refused(&mut self, action: Action, count: usize) -> bool {
        let Answer::Refused(why) = self.answers_at(count)(action) else {
            return false;
        };
        let tasks = self.chosen(&self.tasks_of(count));
        self.refuse(why, &tasks);
        true
    }

    /// Says why a press over `tasks` writes nothing: in the error style when
    /// a task stands in the way, else as a plain message.
    pub(crate) fn refuse(&mut self, why: Refusal, tasks: &[Chosen]) {
        let archived = messages::SOME_ARCHIVED;
        let text = match why {
            Refusal::Quiet => return,
            Refusal::Sorted => messages::sorted(self.sort.name()),
            Refusal::Says(text) => text.to_owned(),
            Refusal::Closed | Refusal::Archived => {
                named(tasks, messages::archived_task, archived)
            }
            Refusal::NoId => named(tasks, messages::archived_no_id, archived),
            Refusal::AllDone => {
                named(tasks, messages::closed_task, messages::CLOSED_TASKS)
            }
            Refusal::SomeDone => messages::SOME_DONE.to_owned(),
            Refusal::Changed => ops::LIST_CHANGED.to_owned(),
        };
        if matches!(why, Refusal::Sorted | Refusal::Says(_)) {
            self.warn(text);
        } else {
            self.alert(text);
        }
    }

    /// Shows a refusal of the keys in the error style.
    pub(crate) fn alert(&mut self, text: String) {
        self.message.tone = Tone::Alert;
        self.message.say(text);
    }

    /// Shows a warning, or a refusal that is no error.
    pub(crate) fn warn(&mut self, text: String) {
        self.message.tone = Tone::Warning;
        self.message.say(text);
    }

    /// Shows why an operation did not run; one refused on lines that
    /// changed also ends a selection.
    pub(crate) fn fail(&mut self, e: &errs::Error) {
        if e.code() == Code::ExpectMismatch {
            self.exit_visual();
        }
        // Its names are lines of the list the core refused against, which
        // `show_read` names them by once it is shown.
        self.live.refused =
            e.said().cloned().map(|said| (said, String::new()));
        let text = if e.code() == Code::LockTimeout {
            messages::busy(&self.base_name())
        } else {
            e.message().to_owned()
        };
        self.alert(text);
    }

    /// `fail`, followed by what to do about it when the error says.
    pub(crate) fn fail_hinted(&mut self, e: &errs::Error) {
        self.fail(e);
        if let Some(hint) = e.hint() {
            let tail = format!(" — {hint}");
            self.message.edit(|message| format!("{message}{tail}"));
            if let Some((_, said)) = &mut self.live.refused {
                *said = tail;
            }
        }
    }
}

/// What `said` becomes when some tasks were already as asked, before the
/// count of those: a count of the `changing` ones where one line says
/// each. `did` is how the command says each.
fn shortened(
    said: &[String],
    lines: usize,
    changing: &[&Chosen],
    did: &str,
) -> String {
    let change = changing.len();
    // Taking a claim over has one line, which the count would lose.
    let plain = !changing.iter().any(|t| matches!(t.claim, Claim::Agent));
    if plain && lines == change {
        format!("{} {did}", ops::counted(change, "task"))
    } else {
        said.join("; ")
    }
}

fn join(said: &[String]) -> Option<String> {
    Some(said.join("; ")).filter(|m| !m.is_empty())
}

/// Where a task went, the name that says it, and what a message says of
/// the task when it went nowhere: the task before the write, where it is
/// after.
type Moved<'a> = ((Source, errs::Place), &'a errs::Name, &'static str);

/// What a message that says where a task went names: an archived task, or a
/// move, an undone or redone one too.
fn moved(msg: &Msg) -> Option<Moved<'_>> {
    match msg.parts() {
        [.., Part::Name(from), Part::Text(text), Part::Name(to)] => {
            let said = match text.as_slice() {
                errs::MOVED_TO => " moved",
                errs::MOVED_BACK => " moved back",
                _ => return Some((to.from?, to, "")),
            };
            Some(((from.source, from.place?), to, said))
        }
        [.., Part::Name(to)] => Some((to.from?, to, "")),
        _ => None,
    }
}

/// The lines the operation was about, each with the place it stands in
/// now and, when it can be paired, the place it stood in when read. An
/// added line has no entry in `before` and a removed one none in
/// `tasks`; when the counts differ no `was` is kept.
fn touched(outcome: &Outcome) -> Vec<Touched> {
    let now = places(outcome.after(), outcome.tasks());
    let was = places(outcome.read(), outcome.before());
    let paired = now.len() == was.len();
    now.into_iter()
        .enumerate()
        .map(|(i, now)| Touched {
            now,
            was: was.get(i).copied().filter(|_| paired),
        })
        .collect()
}

/// The places the tasks of `ids` have in `snap`.
pub(crate) fn places(snap: &Snapshot, ids: &[TaskId]) -> Vec<Place> {
    ids.iter()
        .filter_map(|id| snap.task(*id))
        .map(place_of)
        .collect()
}

/// A refusal: `one` of the name of a lone task, `many` of several.
fn named(tasks: &[Chosen], one: fn(&str) -> String, many: &str) -> String {
    match tasks {
        [task] => one(&task.name),
        _ => many.to_owned(),
    }
}

/// How a message names a task no row shows: by its `id:`, else by its line,
/// or as the command line does when it has none.
fn unshown(name: &errs::Name) -> String {
    match name.place {
        Some(errs::Place::Written(nr) | errs::Place::Read(nr)) => {
            errs::unplaced(name.source, nr, name.id.as_deref())
        }
        None => name
            .id
            .as_ref()
            .map_or_else(|| name.cli(), |id| format!("id:{id}")),
    }
}

/// The priority after `cur` in the cycle none, `last`, … A, none when
/// `forward` (raising), or the other way round (lowering). One past
/// `last` goes to `last` when raised and to none when lowered.
pub(crate) fn stepped(
    cur: Option<Priority>,
    last: Priority,
    forward: bool,
) -> Option<Priority> {
    let letter = cur.map(Priority::letter);
    let next = match (letter, forward) {
        (None, true) => Some(last.letter()),
        (Some(c), true) if c > last.letter() => Some(last.letter()),
        (Some(c), true) => Some(c - 1),
        (None, false) => Some(Priority::A.letter()),
        (Some(c), false) if c >= last.letter() => None,
        (Some(c), false) => Some(c + 1),
    };
    next.and_then(Priority::new)
}

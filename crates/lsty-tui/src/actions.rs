//! The keys that write: each one is a single operation over all the
//! tasks it is about, run as the human.

use lsty_core::{
    dates::Clock,
    errs::{self, Code, Field as Extra, Nr, Source},
    keys::Action,
    list::{Snapshot, State, TaskId},
    ops::{self, ClaimOpts, DoOpts, Env, Outcome, PriPair, StartOpts, Target},
    todotxt::Priority,
};

use crate::{
    history::{Again, pairs_of},
    messages::{self, Tone},
    model::Model,
    track::{Place, Touched, Wrote, place_of},
};

/// What a key does to its tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Write {
    /// `toggle_start`: start, stop or reopen, by state.
    Start,
    Stop,
    /// `toggle_complete`: complete, or reopen a done task.
    Toggle,
    /// `wontdo`: finish as won't do, or reopen a won't do task.
    Wontdo,
    Delete,
    /// `cycle_priority` and its way back.
    Step {
        forward: bool,
    },
}

impl Write {
    /// What `action` writes, if it is one of the task actions.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the other actions do not write tasks"
    )]
    fn of(action: Action) -> Option<Self> {
        Some(match action {
            Action::ToggleStart => Self::Start,
            Action::Stop => Self::Stop,
            Action::ToggleComplete => Self::Toggle,
            Action::Wontdo => Self::Wontdo,
            Action::Delete => Self::Delete,
            Action::CyclePriority => Self::Step { forward: true },
            Action::CyclePriorityBack => Self::Step { forward: false },
            _ => return None,
        })
    }

    /// Whether the tasks must all be in one state.
    const fn same_state(self) -> bool {
        matches!(self, Self::Start | Self::Stop | Self::Toggle | Self::Wontdo)
    }
}

/// A task an action is about, as the list showed it.
pub(crate) struct Chosen {
    source: Source,
    nr: Nr,
    state: State,
    wontdo: bool,
    pub(crate) text: Vec<u8>,
    name: String,
    priority: Option<Priority>,
}

impl Chosen {
    pub(crate) const fn archived(&self) -> bool {
        matches!(self.source, Source::Done)
    }

    /// The reference to the line, guarded by the text it had when the
    /// list showed it.
    pub(crate) fn target(&self) -> Target {
        let mut target = Target::at(self.source, i64::from(self.nr.get()));
        target.set_expect(self.text.clone(), true);
        target
    }
}

impl Model {
    /// Does a task action over the task under the cursor, or `count`
    /// tasks from it.
    pub(crate) fn act(&mut self, action: Action, count: usize) -> bool {
        let Some(write) = Write::of(action) else {
            return false;
        };
        let ids = self.tasks_of(count);
        let tasks = self.chosen(&ids);
        let Some(state) = tasks.first().map(|i| i.state) else {
            return false;
        };
        let again = self.count_for(count, tasks.len());
        let ran = self.ending(&ids, |m| {
            if let Some(text) = refusal(write, &tasks) {
                m.alert(text);
                return false;
            }
            let result = m.attempt(|env| m.perform(env, write, state, &tasks));
            m.settle(result)
        });
        if ran {
            self.again = Some(Again::Task {
                action,
                count: again,
            });
        }
        ran
    }

    /// Takes what an operation did, or shows why it did not run. Tells
    /// whether the operation ran.
    pub(crate) fn settle(
        &mut self,
        result: errs::Result<(Outcome, Clock)>,
    ) -> bool {
        match result {
            Ok((outcome, clock)) => {
                self.adopt(outcome, clock);
                true
            }
            Err(e) => {
                self.fail(&e);
                false
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
                    wontdo: self.snap.is_wontdo(task),
                    text: line.text(),
                    name: messages::name_of(line),
                    priority: line.effective_priority(),
                })
            })
            .collect()
    }

    /// Runs `run` over an environment of the current files, as the
    /// human; the clock is the one it used.
    pub(crate) fn attempt<T>(
        &self,
        run: impl FnOnce(&Env<'_>) -> errs::Result<T>,
    ) -> errs::Result<(T, Clock)> {
        let clock = self.clock_now()?;
        let mut env = Env::new(self.snap.found(), &self.deps.config, &clock);
        env.create = true;
        env.record_undo = true;
        env.cancel = self.cancel.clone();
        env.ids = self.deps.ids.as_ref();
        Ok((run(&env)?, clock))
    }

    /// Runs the operation of `write` over `tasks`, the first of which
    /// is in `state`.
    fn perform(
        &self,
        env: &Env<'_>,
        write: Write,
        state: State,
        tasks: &[Chosen],
    ) -> errs::Result<Outcome> {
        let targets: Vec<Target> = tasks.iter().map(Chosen::target).collect();
        let claim = ClaimOpts::default();
        if tasks.iter().any(Chosen::archived) {
            return ops::restore(env, &targets, claim);
        }
        match (write, state) {
            (Write::Stop, _) | (Write::Start, State::Doing) => {
                ops::stop(env, &targets, claim)
            }
            (Write::Delete, _) => ops::del(env, &targets, claim),
            (Write::Step { forward }, _) => {
                let last = self.deps.config.priority_range;
                let pairs =
                    tasks.iter().zip(targets).map(|(task, target)| PriPair {
                        target,
                        priority: stepped(task.priority, last, forward),
                    });
                ops::pri(env, &pairs.collect::<Vec<_>>())
            }
            (Write::Start, State::Todo) => {
                ops::start(env, &targets, StartOpts::default())
            }
            (Write::Toggle, State::Doing | State::Todo) => {
                ops::finish(env, &targets, DoOpts::default())
            }
            (Write::Wontdo, State::Doing | State::Todo) => {
                let opts = DoOpts {
                    wontdo: true,
                    ..DoOpts::default()
                };
                ops::finish(env, &targets, opts)
            }
            (Write::Start | Write::Toggle | Write::Wontdo, State::Done) => {
                ops::reopen(env, &targets, claim)
            }
        }
    }

    /// Takes the files an operation wrote as the state shown, keeps its
    /// undo record and shows its messages.
    pub(crate) fn adopt(&mut self, mut outcome: Outcome, clock: Clock) {
        let said: Vec<String> =
            outcome.messages().iter().map(|m| errs::lossy(m)).collect();
        if outcome.no_op() {
            self.message = join(&said);
            return;
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
        self.message = join(&said);
        if !landed {
            self.stick_cursor(&touched, from);
        }
        self.keep_sticky(&touched);
        if let Some(record) = record {
            let pairs = pairs_of(&record, &touched);
            let first = said.first().cloned().unwrap_or_default();
            self.hist.record(record, first, pairs);
        }
    }

    /// Shows a refusal of the keys in the error style.
    pub(crate) fn alert(&mut self, text: String) {
        self.tone = Tone::Alert;
        self.message = Some(text);
    }

    /// Shows why an operation did not run; nothing else changes.
    pub(crate) fn fail(&mut self, e: &errs::Error) {
        let text = match e.code() {
            Code::LockTimeout => messages::busy(&self.base_name()),
            Code::ExpectMismatch => {
                self.owe_reload();
                e.message().to_owned()
            }
            Code::Usage
            | Code::UnknownCommand
            | Code::InvalidValue
            | Code::NoTty
            | Code::NoIdentity
            | Code::NoFile
            | Code::NoTask
            | Code::Blocked
            | Code::Cycle
            | Code::Claimed
            | Code::WrongState
            | Code::IdTaken
            | Code::Ambiguous
            | Code::EditConflict
            | Code::Timeout
            | Code::Io => with_forms(e),
        };
        self.alert(text);
    }
}

fn join(said: &[String]) -> Option<String> {
    Some(said.join("; ")).filter(|m| !m.is_empty())
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

/// Why the keys turn the tasks away before any operation.
fn refusal(write: Write, tasks: &[Chosen]) -> Option<String> {
    if write == Write::Toggle && tasks.iter().all(Chosen::archived) {
        return None;
    }
    archived_refusal(tasks).or_else(|| {
        if !write.same_state() {
            return None;
        }
        let states: Vec<State> = tasks.iter().map(|i| i.state).collect();
        messages::mixed_states(&states).or_else(|| done_refusal(write, tasks))
    })
}

/// The refusal of `T` on tasks of which some are done but not won't do.
fn done_refusal(write: Write, tasks: &[Chosen]) -> Option<String> {
    let plain = |t: &Chosen| t.state == State::Done && !t.wontdo;
    if write != Write::Wontdo || !tasks.iter().any(plain) {
        return None;
    }
    Some(match tasks {
        [one] => messages::done_task(&one.name),
        _ => messages::DONE_TASKS.to_owned(),
    })
}

/// The refusal of tasks of which some are archived, or `None` when none
/// is.
pub(crate) fn archived_refusal(tasks: &[Chosen]) -> Option<String> {
    if !tasks.iter().any(Chosen::archived) {
        return None;
    }
    Some(match tasks {
        [one] => messages::archived_task(&one.name),
        _ => messages::ARCHIVED_TASKS.to_owned(),
    })
}

/// The message of `e`, followed by the forms it accepts.
fn with_forms(e: &errs::Error) -> String {
    if let Some(Extra::Strs(forms)) = e.field("accepted") {
        format!("{} — {}", e.message(), forms.join("; "))
    } else {
        e.message().to_owned()
    }
}

/// The priority after `cur` in the cycle none, A, … `last`, none, or
/// the other way round.
pub(crate) fn stepped(
    cur: Option<Priority>,
    last: Priority,
    forward: bool,
) -> Option<Priority> {
    let letter = cur.map(Priority::letter);
    let next = match (letter, forward) {
        (None, true) => Some(Priority::A.letter()),
        (Some(c), true) if c >= last.letter() => None,
        (Some(c), true) => Some(c + 1),
        (None, false) => Some(last.letter()),
        (Some(c), false) if c > last.letter() => Some(last.letter()),
        (Some(c), false) => Some(c - 1),
    };
    next.and_then(Priority::new)
}

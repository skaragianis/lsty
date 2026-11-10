//! The keys that write: each one runs the single command it resolves to,
//! as the human.

use lsty_core::{
    dates::Clock,
    errs::{self, Code, Nr, Source},
    keys::Action,
    list::{Claim, Snapshot, State, TaskId},
    ops::{self, ClaimOpts, DoOpts, Env, Outcome, PriPair, StartOpts, Target},
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
        let ran = self.ending(&ids, |m| m.answered(answer, back, &tasks));
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

    /// Does what `answer` says over `tasks`, or, for `back`, puts back the
    /// claims `t` just ended. Tells whether a write ran.
    fn answered(
        &mut self,
        answer: Answer,
        back: bool,
        tasks: &[Chosen],
    ) -> bool {
        let (command, passes) = match answer {
            Answer::Applies | Answer::Refused(_) => return false,
            Answer::CoreRefuses(command) => (command, false),
            Answer::Resolved(command) => (command, true),
        };
        if back {
            return self.put_back();
        }
        let changing: Vec<&Chosen> = tasks
            .iter()
            .filter(|task| !passes || task.changed_by(command))
            .collect();
        let (change, there) = (changing.len(), tasks.len() - changing.len());
        // A completion goes on below its press, not to a task it passed over.
        let cursor = self.cursor;
        if there > 0 && matches!(command, Command::Do | Command::Wontdo) {
            self.cursor = (cursor + tasks.len()).min(self.rows.len() - 1);
        }
        let aimed: Vec<Target> = tasks.iter().map(Chosen::target).collect();
        let result = self.attempt(|env| {
            let env = Env {
                aimed: &aimed,
                dry: !passes,
                ..env.clone()
            };
            self.perform(&env, command, &changing)
        });
        let said: Vec<String> = result.as_ref().map_or_else(
            |_| Vec::new(),
            |(outcome, _)| {
                outcome.messages().iter().map(|m| errs::lossy(m)).collect()
            },
        );
        if !self.settle(result) || !passes {
            self.tone = Tone::Alert;
            self.cursor = cursor;
            return false;
        }
        if let Some((did, already)) = command.words().filter(|_| there > 0) {
            let all = said.join("; ");
            // Taking a claim over has one line, which the count would lose.
            let plain =
                !changing.iter().any(|t| matches!(t.claim, Claim::Agent));
            let lines = said.iter().filter(|l| !l.starts_with("archived to "));
            let head = if plain && lines.count() == change {
                format!("{change} {did}")
            } else {
                all.clone()
            };
            let short = format!("{head}; {there} {already}");
            self.message =
                self.message.take().map(|m| m.replacen(&all, &short, 1));
        }
        self.scroll();
        true
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
        let mut env = Env::new(self.snap.found(), &self.deps.config, &clock);
        env.create = true;
        env.record_undo = true;
        env.cancel = self.cancel.clone();
        env.ids = self.deps.ids.as_ref();
        env.aim = Some(&self.snap);
        Ok((run(&env)?, clock))
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
    /// undo record and shows its messages.
    pub(crate) fn adopt(&mut self, mut outcome: Outcome, clock: Clock) {
        let said: Vec<String> =
            outcome.messages().iter().map(|m| errs::lossy(m)).collect();
        if outcome.no_op() {
            self.message = join(&said);
            return;
        }
        self.catch_up(outcome.read(), clock.clone());
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
            self.message = Some(text);
        } else {
            self.alert(text);
        }
    }

    /// Shows a refusal of the keys in the error style.
    pub(crate) fn alert(&mut self, text: String) {
        self.tone = Tone::Alert;
        self.message = Some(text);
    }

    /// Shows why an operation did not run; nothing else changes.
    pub(crate) fn fail(&mut self, e: &errs::Error) {
        if e.code() == Code::ExpectMismatch {
            self.owe_reload();
            self.exit_visual();
        }
        let text = if e.code() == Code::LockTimeout {
            messages::busy(&self.base_name())
        } else {
            e.message().to_owned()
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

/// A refusal: `one` of the name of a lone task, `many` of several.
fn named(tasks: &[Chosen], one: fn(&str) -> String, many: &str) -> String {
    match tasks {
        [task] => one(&task.name),
        _ => many.to_owned(),
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

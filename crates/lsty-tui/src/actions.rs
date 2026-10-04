//! The keys that write: each one is a single operation over all the
//! items it is about, run as the human.

use lsty_core::dates::Clock;
use lsty_core::errs::{self, Code, Field as Extra, Source};
use lsty_core::keys::Action;
use lsty_core::list::{ItemId, Snapshot, State};
use lsty_core::ops::{
    self, ClaimOpts, DoOpts, Env, Outcome, PriPair, StartOpts, Target,
};
use lsty_core::todotxt::Priority;

use crate::history::{Again, pairs_of};
use crate::live::{Loaded, Stamp};
use crate::messages::{self, Tone};
use crate::model::Model;
use crate::track::{Place, Wrote};

/// What a key does to its items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Write {
    /// `cycle_state`: start, complete or reopen, by state.
    Cycle,
    Stop,
    /// `toggle_complete`: complete, or reopen a done item.
    Toggle,
    Delete,
    /// `cycle_priority` and its way back.
    Step {
        forward: bool,
    },
}

impl Write {
    /// What `action` writes, if it is one of the item actions.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the other actions do not write items"
    )]
    fn of(action: Action) -> Option<Self> {
        Some(match action {
            Action::CycleState => Self::Cycle,
            Action::Stop => Self::Stop,
            Action::ToggleComplete => Self::Toggle,
            Action::Delete => Self::Delete,
            Action::CyclePriority => Self::Step { forward: true },
            Action::CyclePriorityBack => Self::Step { forward: false },
            _ => return None,
        })
    }

    /// Whether the items must all be in one state.
    const fn same_state(self) -> bool {
        matches!(self, Self::Cycle | Self::Stop | Self::Toggle)
    }
}

/// An item an action is about, as the list showed it.
pub(crate) struct Chosen {
    source: Source,
    nr: u32,
    state: State,
    pub(crate) text: Vec<u8>,
    priority: Option<Priority>,
}

impl Chosen {
    pub(crate) const fn archived(&self) -> bool {
        matches!(self.source, Source::Done)
    }

    /// The reference to the line, guarded by the text it had when the
    /// list showed it.
    pub(crate) fn target(&self) -> Target {
        let nr = i64::from(self.nr);
        let mut target = match self.source {
            Source::Todo => Target::todo(nr),
            Source::Done => Target::done(nr),
        };
        // Only a reference by id refuses a guard.
        drop(target.set_expect(self.text.clone(), true));
        target
    }
}

impl Model {
    /// Does an item action over the item under the cursor, or `count`
    /// items from it.
    pub(crate) fn act(&mut self, action: Action, count: usize) {
        let Some(write) = Write::of(action) else {
            return;
        };
        let ids = self.items_of(count);
        let items = self.chosen(&ids);
        if items.is_empty() {
            return;
        }
        let again = self.count_for(count, items.len());
        let ran = self.ending(&ids, |m| {
            if let Some(text) = refusal(write, &items) {
                m.alert(text);
                return false;
            }
            let result = m.attempt(|env| m.perform(env, write, &items));
            m.settle(result)
        });
        if ran {
            self.again = Some(Again::Item {
                action,
                count: again,
            });
        }
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

    pub(crate) fn chosen(&self, ids: &[ItemId]) -> Vec<Chosen> {
        ids.iter()
            .filter_map(|id| {
                let item = self.snap.item(*id)?;
                let line = self.snap.line(item);
                Some(Chosen {
                    source: item.source(),
                    nr: item.nr().get(),
                    state: item.state(),
                    text: line.text(),
                    priority: line.effective_priority(),
                })
            })
            .collect()
    }

    /// Runs `run` over an environment of the current files, as the
    /// human; the clock is the one it used.
    pub(crate) fn attempt(
        &self,
        run: impl FnOnce(&Env<'_>) -> errs::Result<Outcome>,
    ) -> errs::Result<(Outcome, Clock)> {
        self.attempt_as(false, run)
    }

    /// Like [`Model::attempt`], for text a person typed: a mistake that
    /// can be flagged is written and warned about, not refused.
    pub(crate) fn attempt_soft(
        &self,
        run: impl FnOnce(&Env<'_>) -> errs::Result<Outcome>,
    ) -> errs::Result<(Outcome, Clock)> {
        self.attempt_as(true, run)
    }

    fn attempt_as(
        &self,
        soft: bool,
        run: impl FnOnce(&Env<'_>) -> errs::Result<Outcome>,
    ) -> errs::Result<(Outcome, Clock)> {
        let clock = self.clock_now()?;
        let mut env = Env::new(&self.deps.found, &self.deps.config, &clock);
        env.create = true;
        env.record_undo = true;
        env.cycle_warn_only = soft;
        env.cancel = self.cancel.clone();
        env.ids = self.deps.ids.as_ref();
        Ok((run(&env)?, clock))
    }

    /// Runs the operation of `write`.
    fn perform(
        &self,
        env: &Env<'_>,
        write: Write,
        items: &[Chosen],
    ) -> errs::Result<Outcome> {
        let targets: Vec<Target> = items.iter().map(Chosen::target).collect();
        if items.iter().any(Chosen::archived) {
            ops::restore(env, &targets, ClaimOpts::default())
        } else {
            self.operate(env, write, items, targets)
        }
    }

    fn operate(
        &self,
        env: &Env<'_>,
        write: Write,
        items: &[Chosen],
        targets: Vec<Target>,
    ) -> errs::Result<Outcome> {
        let claim = ClaimOpts::default();
        let state = items.first().map(|i| i.state);
        match (write, state) {
            (Write::Stop, _) => ops::stop(env, &targets, claim),
            (Write::Delete, _) => ops::del(env, &targets, claim),
            (Write::Step { forward }, _) => {
                let last = self.deps.config.priority_range;
                let pairs =
                    items.iter().zip(targets).map(|(item, target)| PriPair {
                        target,
                        letter: stepped(item.priority, last, forward),
                    });
                ops::pri(env, &pairs.collect::<Vec<_>>())
            }
            (Write::Cycle, Some(State::Todo)) => {
                ops::start(env, &targets, StartOpts::default())
            }
            (Write::Cycle | Write::Toggle, Some(State::Doing) | None)
            | (Write::Toggle, Some(State::Todo)) => {
                ops::finish(env, &targets, DoOpts::default())
            }
            (Write::Cycle | Write::Toggle, Some(State::Done)) => {
                ops::reopen(env, &targets, claim)
            }
        }
    }

    /// Takes the files an operation wrote as the state shown, keeps its
    /// undo record and shows its messages.
    pub(crate) fn adopt(&mut self, mut outcome: Outcome, clock: Clock) {
        let mut said: Vec<String> =
            outcome.messages().iter().map(|m| errs::lossy(m)).collect();
        if outcome.no_op() {
            self.message = join(&said);
            return;
        }
        let touched = places(outcome.after(), outcome.items());
        let from = places(outcome.read(), outcome.before());
        let record = outcome.take_undo();
        let snap = outcome.into_after();
        let stamp = Stamp::of(&snap);
        let wrote = record.as_ref().map(|record| Wrote {
            record,
            touched: &touched,
            from: &from,
        });
        self.install(Loaded { snap, stamp }, clock, wrote.as_ref());
        if self.keep_sticky(&touched) {
            said.push(messages::STICKY.to_owned());
        }
        self.message = join(&said);
        if let Some(record) = record {
            let pairs = pairs_of(&record, &touched, &from);
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
            | Code::NoItem
            | Code::Blocked
            | Code::Cycle
            | Code::Claimed
            | Code::WrongState
            | Code::IdTaken
            | Code::Ambiguous
            | Code::EditConflict
            | Code::Io => with_forms(e),
        };
        self.alert(text);
    }
}

fn join(said: &[String]) -> Option<String> {
    Some(said.join("; ")).filter(|m| !m.is_empty())
}

/// The places the items of `ids` have in `snap`.
pub(crate) fn places(snap: &Snapshot, ids: &[ItemId]) -> Vec<Place> {
    ids.iter()
        .filter_map(|id| snap.item(*id))
        .map(|item| (item.source(), item.nr().get()))
        .collect()
}

/// Why the keys turn the items away before any operation.
fn refusal(write: Write, items: &[Chosen]) -> Option<String> {
    if items.iter().any(Chosen::archived) {
        let restores =
            write == Write::Toggle && items.iter().all(Chosen::archived);
        return if restores {
            None
        } else {
            archived_refusal(items)
        };
    }
    if items.len() > 1 && write.same_state() {
        let states: Vec<State> = items.iter().map(|i| i.state).collect();
        return messages::mixed_states(&states);
    }
    None
}

/// The refusal of items of which some are archived, or `None` when none
/// is.
pub(crate) fn archived_refusal(items: &[Chosen]) -> Option<String> {
    if !items.iter().any(Chosen::archived) {
        return None;
    }
    Some(match items {
        [one] => messages::archived(Some(&errs::lossy(&one.text))),
        _ => messages::archived(None),
    })
}

/// The message of `e`, followed by the forms it accepts.
fn with_forms(e: &errs::Error) -> String {
    let forms = e.fields().find_map(|(key, value)| match value {
        Extra::Strs(forms) if key == "accepted" => Some(forms),
        Extra::Null
        | Extra::Int(_)
        | Extra::Str(_)
        | Extra::Strs(_)
        | Extra::Ref(_)
        | Extra::Refs(_) => None,
    });
    match forms {
        Some(forms) => format!("{} — {}", e.message(), forms.join("; ")),
        None => e.message().to_owned(),
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
        (Some(c), false) if c == Priority::A.letter() => None,
        (Some(c), false) => Some(c - 1),
    };
    next.and_then(Priority::new)
}

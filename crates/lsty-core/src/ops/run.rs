//! The steps of an operation: lock, read, plan, check, write, report.

use super::env::Env;
use super::result::Outcome;
use super::staged::SealedFile;
use super::tx::Tx;
use crate::errs::{Code, Error, Result, Source};
use crate::fsio;
use crate::list::Snapshot;
use crate::lock::{self, Mode, Want};
use crate::todotxt::File;

/// Locks the list, reads it again, lets `plan` stage its changes, checks
/// them and writes them. The lock is held throughout. An error from
/// `plan` or a check ends the run with nothing written.
pub(super) fn run<F>(
    env: &Env<'_>,
    action: &str,
    writes_done: bool,
    plan: F,
) -> Result<Outcome>
where
    F: FnOnce(&mut Tx) -> Result<()>,
{
    run_with(env, action, writes_done, Rules::Apply, plan)
}

/// Whether the rule layer checks what a plan staged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rules {
    /// Claims, stamps, ids and cycles are checked.
    Apply,
    /// The plan restores what was there and nothing is checked.
    Skip,
    /// As `Apply`, but a dependency cycle is a warning, so that typing
    /// is never lost.
    WarnCycles,
}

/// Like [`run`], with a choice of whether the rule layer runs.
pub(super) fn run_with<F>(
    env: &Env<'_>,
    action: &str,
    writes_done: bool,
    rules: Rules,
    plan: F,
) -> Result<Outcome>
where
    F: FnOnce(&mut Tx) -> Result<()>,
{
    let done_mode = if writes_done {
        Mode::Exclusive
    } else {
        Mode::Shared
    };
    let wants = [
        Want {
            file: env.files.todo.clone(),
            mode: Mode::Exclusive,
        },
        Want {
            file: env.files.done.clone(),
            mode: done_mode,
        },
    ];
    let mut held = lock::acquire(&wants, env.timeout, &env.cancel)?;
    let result = run_locked(env, action, done_mode, rules, plan);
    let released = held.release();
    match (result, released) {
        (Ok(_), Err(e)) => Err(e),
        (result, _) => result,
    }
}

fn run_locked<F>(
    env: &Env<'_>,
    action: &str,
    done_mode: Mode,
    rules: Rules,
    plan: F,
) -> Result<Outcome>
where
    F: FnOnce(&mut Tx) -> Result<()>,
{
    let mut tx = open(env, done_mode)?;
    plan(&mut tx)?;
    if rules != Rules::Skip {
        tx.cascade();
        tx.check_claims(env)?;
        tx.check_stamps()?;
        tx.check_ids()?;
        tx.check_cycles(env, rules)?;
    }
    let sealed = tx.seal();
    sealed.check_done_lock(action, done_mode)?;
    let written = sealed.write(env)?;
    Ok(sealed.report(env, action, &written))
}

/// Reads both files again and builds the snapshot the plan sees.
fn open(env: &Env<'_>, done_mode: Mode) -> Result<Tx> {
    let (todo, todo_exists) = read_file(&env.files.todo)?;
    if !todo_exists && !env.create {
        return Err(env.files.no_file());
    }
    let (done, _) = read_file(&env.files.done)?;
    clean(env, done_mode)?;
    let snap = Snapshot::build(env.files.clone(), todo, done, &env.params());
    Ok(Tx::new(snap, !todo_exists))
}

/// The file, and whether it exists; a missing file is an empty one.
fn read_file(path: &std::path::Path) -> Result<(File, bool)> {
    Ok(match fsio::read(path)? {
        Some(data) => (File::parse(&data), true),
        None => (File::default(), false),
    })
}

/// Removes the temp files of the files this operation may write.
fn clean(env: &Env<'_>, done_mode: Mode) -> Result<()> {
    fsio::clean_orphans(&env.files.todo)?;
    if done_mode == Mode::Exclusive {
        fsio::clean_orphans(&env.files.done)?;
    }
    Ok(())
}

/// A plan that is over: nothing can be staged any more, and the bytes
/// of each file are fixed.
#[derive(Debug)]
pub(super) struct Sealed {
    pub(super) tx: Tx,
    pub(super) todo: SealedFile,
    pub(super) done: SealedFile,
}

impl Tx {
    /// Ends the plan and fixes the bytes of both files.
    fn seal(self) -> Sealed {
        Sealed {
            todo: self.todo.seal(),
            done: self.done.seal(),
            tx: self,
        }
    }
}

impl Sealed {
    const fn file(&self, source: Source) -> &SealedFile {
        match source {
            Source::Todo => &self.todo,
            Source::Done => &self.done,
        }
    }

    /// Refuses a plan that stages the done file when it is locked only
    /// for reading.
    fn check_done_lock(&self, action: &str, done_mode: Mode) -> Result<()> {
        let stages_done =
            self.done.changed || self.tx.dest == Some(Source::Done);
        if done_mode == Mode::Exclusive || !stages_done {
            return Ok(());
        }
        Err(Error::new(
            Code::Io,
            format!(
                "{action} changed the done file without locking it for writing"
            ),
        ))
    }

    /// Writes each file that changed, the destination first. Returns
    /// the files written, in order.
    fn write(&self, env: &Env<'_>) -> Result<Vec<Source>> {
        let mut order: Vec<Source> = [Source::Todo, Source::Done]
            .into_iter()
            .filter(|&s| self.file(s).writes)
            .collect();
        if order.len() == 2 && Some(Source::Done) == self.tx.dest {
            order.reverse();
        }
        for &source in &order {
            let data = &self.file(source).bytes;
            fsio::write(env.files.path(source), data, env.hooks)?;
        }
        Ok(order)
    }
}

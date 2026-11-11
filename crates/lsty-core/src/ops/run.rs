//! The steps of an operation: lock, read, plan, check, write, report.

use super::{
    env::Env, landing::Landing, result::Outcome, staged::SealedFile, tx::Tx,
};
use crate::{
    discover::{self, FileState},
    errs::{Code, Error, Result, Source},
    fsio,
    list::Snapshot,
    lock::{self, Mode, Want},
    todotxt::File,
};

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

/// How the rule layer checks what a plan staged.
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

/// Like [`run`], with a choice of rules.
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
    if env.dry && env.aim.is_some() {
        return run_locked(env, action, Mode::Exclusive, rules, plan);
    }
    let done_mode = if writes_done {
        Mode::Exclusive
    } else {
        Mode::Shared
    };
    let wants = [
        Want {
            file: env.found.todo.clone(),
            mode: Mode::Exclusive,
        },
        Want {
            file: env.found.done.clone(),
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
    let held = env.aim.filter(|_| env.dry);
    let mut tx = match held {
        Some(snap) => Tx::new(snap.clone(), false),
        None => open(env, done_mode)?,
    };
    if let Err(e) = planned(env, &mut tx, rules, plan) {
        // Only a reading made here is worth showing: it says what is there.
        let slot = env.read_back.filter(|_| held.is_none());
        if let Some(slot) = slot.filter(|_| e.code() == Code::ExpectMismatch) {
            slot.set(Some(tx.snap));
        }
        return Err(e);
    }
    let sealed = tx.seal();
    sealed.check_done_lock(action, done_mode)?;
    let written = if env.dry {
        Vec::new()
    } else {
        sealed.write(env)?
    };
    Ok(sealed.report(env, action, &written))
}

/// Checks that the lines the action was aimed at are there, lets `plan`
/// stage its changes and checks them.
fn planned<F>(env: &Env<'_>, tx: &mut Tx, rules: Rules, plan: F) -> Result<()>
where
    F: FnOnce(&mut Tx) -> Result<()>,
{
    if let Some(landing) = &tx.landing {
        for target in env.aimed {
            target.landed(landing)?;
        }
    }
    plan(tx)?;
    if rules != Rules::Skip {
        tx.cascade();
        tx.check_claims(env)?;
        tx.check_stamps()?;
        tx.check_ids()?;
        tx.check_cycles(env, rules)?;
    }
    Ok(())
}

/// Reads both files again and builds the snapshot the plan sees.
fn open(env: &Env<'_>, done_mode: Mode) -> Result<Tx> {
    let (todo, todo_exists) = read_file(&env.found.todo)?;
    if !todo_exists && !env.create {
        return Err(env.found.no_file());
    }
    let (done, _) = read_file(&env.found.done)?;
    clean(env, done_mode)?;
    let mut found = env.found.clone();
    // The caller may show this reading: it says what is there now.
    found.state = if todo_exists {
        FileState::Exists
    } else {
        discover::locate(&found.todo)?.1
    };
    let snap = Snapshot::build(found, todo, done, &env.params());
    let landing = env.aim.map(|from| Landing::of(from, &snap));
    let mut tx = Tx::new(snap, !todo_exists);
    tx.landing = landing;
    Ok(tx)
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
    fsio::clean_orphans(&env.found.todo)?;
    if done_mode == Mode::Exclusive {
        fsio::clean_orphans(&env.found.done)?;
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
    fn file(&self, source: Source) -> &SealedFile {
        source.pick(&self.todo, &self.done)
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
            fsio::write(env.found.path(source), data, env.hooks)?;
        }
        if let Some((path, data)) = &self.tx.log {
            fsio::write(path, data, env.hooks)?;
        }
        Ok(order)
    }
}

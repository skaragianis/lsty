//! `lsty wait ID…`: blocks until the line of one of the tasks changes,
//! unless one is already done.

use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

use lsty_core::{
    dates::parse_duration,
    errs::{Code, Error, Result, Source},
    list::{Snapshot, State},
    todotxt::Id,
    watch::Sink,
};

use crate::{
    commands::Call,
    env::Env,
    output::{Field, Output},
    parse::{Opt, needs, usage},
    reflist::ref_list,
};

/// How long a wait lasts without `--timeout`.
const DEFAULT_TIMEOUT: &str = "1h";

/// How often the files are read again when no event comes, which also
/// covers a watch that failed.
const POLL: Duration = Duration::from_secs(60);

/// How often they are read when the watch could not start.
const POLL_UNWATCHED: Duration = Duration::from_secs(2);

/// `lsty wait`: returns once the line of one of the tasks is no longer
/// what it was, or at once when one is done or archived, and says to look
/// at all of them.
pub(crate) fn wait(call: &Call<'_>, env: &Env<'_>) -> Result<Output> {
    let ids = ids(&call.parsed.args)?;
    let given = call.option(Opt::Timeout).unwrap_or(DEFAULT_TIMEOUT);
    let limit = timeout(given)?;
    let started = Instant::now();
    let snap = call.load()?;
    let mut done = false;
    for id in &ids {
        let task = snap.by_id(id.as_str())?;
        done |= task.state() == State::Done || task.source() == Source::Done;
    }
    // A task already done or archived is the answer: nothing to wait for.
    if done {
        return Ok(answer(call, &ids));
    }
    let before = lines(&snap, &ids);
    let (tx, rx) = mpsc::channel();
    // Kept here, the sink keeps the channel open when the watch fails.
    let sink: Sink = Arc::new(move |_| {
        tx.send(()).ok();
    });
    let mut watcher = (env.watch)(call.ops.found, Arc::clone(&sink)).ok();
    let poll = if watcher.is_some() {
        POLL
    } else {
        POLL_UNWATCHED
    };
    while lines(&call.load()?, &ids) == before {
        let left = match limit {
            Some(limit) => limit
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero())
                .ok_or_else(|| timed_out(given))?,
            None => poll,
        };
        // Woken by an event or not, the files are read again.
        rx.recv_timeout(left.min(poll)).ok();
        if let Some(watcher) = &mut watcher {
            // A folder that cannot be watched again is read at each poll.
            drop(watcher.rewatch());
        }
    }
    Ok(answer(call, &ids))
}

/// The success of a wait: look at every task.
fn answer(call: &Call<'_>, ids: &[Id]) -> Output {
    let next = format!(
        "lsty show {}",
        ids.iter()
            .map(|id| format!("id:{id}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Output::either(
        call.json(),
        || {
            vec![
                Field::string("action", call.command.name),
                Field::string("next", &next),
            ]
        },
        || vec![format!("hint: run \"{next}\"").into_bytes()],
    )
}

/// The ids of the references; a line number is a usage error.
fn ids(args: &[String]) -> Result<Vec<Id>> {
    let targets = ref_list(args)?;
    if targets.is_empty() {
        return Err(needs());
    }
    targets
        .iter()
        .map(|target| {
            target.id().cloned().ok_or_else(|| {
                usage(format!(
                    "wait takes id: references, not {}",
                    target.handle()
                ))
            })
        })
        .collect()
}

/// The time `--timeout` allows; zero (`0`, `0s`…) sets no limit.
fn timeout(text: &str) -> Result<Option<Duration>> {
    if matches!(
        text.strip_prefix('0'),
        Some("" | "ms" | "s" | "m" | "h" | "d")
    ) {
        return Ok(None);
    }
    parse_duration(text).map(Some)
}

fn timed_out(given: &str) -> Error {
    Error::new(Code::Timeout, format!("no task changed within {given}"))
}

/// Where a task is: its file and its raw line; `None` when it is gone or
/// its id is no longer on one line.
type Place = Option<(Source, Vec<u8>)>;

/// What a wait compares: the place of each task.
fn lines(snap: &Snapshot, ids: &[Id]) -> Vec<Place> {
    ids.iter()
        .map(|id| {
            let task = snap.by_id(id.as_str()).ok()?;
            Some((task.source(), snap.line(task).raw().to_vec()))
        })
        .collect()
}

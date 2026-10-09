use std::path::Path;
use std::time::Duration;

use jiff::tz::TimeZone;

use super::{Params, Snapshot};
use crate::dates::Clock;
use crate::discover::{FileState, Found, FoundBy};
use crate::todotxt::File;

pub(crate) const STALE_AFTER: Duration = Duration::from_hours(4);

/// Today is 2026-09-29 and now is 09:00 UTC.
pub(crate) fn clock() -> Clock {
    let var =
        |name: &str| (name == "LSTY_NOW").then(|| "2026-09-29T0900Z".into());
    Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, TimeZone::UTC)
        .expect("clock")
}

pub(crate) fn params() -> Params {
    Params {
        clock: clock(),
        stale_after: STALE_AFTER,
    }
}

pub(crate) fn found(dir: impl AsRef<Path>) -> Found {
    let dir = dir.as_ref();
    Found {
        todo: dir.join("todo.txt"),
        done: dir.join("done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    }
}

/// A file of `lines`, each ending in a newline.
pub(crate) fn file(lines: &[&str]) -> File {
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    File::parse(text.as_bytes())
}

pub(crate) fn snapshot(todo: &[&str], done: &[&str]) -> Snapshot {
    snapshot_in("/d", todo, done)
}

/// A snapshot of `todo` and `done` lines in the folder `dir`.
pub(crate) fn snapshot_in(
    dir: impl AsRef<Path>,
    todo: &[&str],
    done: &[&str],
) -> Snapshot {
    Snapshot::build(found(dir), file(todo), file(done), &params())
}

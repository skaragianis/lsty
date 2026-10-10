use std::{path::Path, time::Duration};

use jiff::tz::TimeZone;

use super::{Params, Snapshot};
use crate::{
    dates::Clock,
    discover::{FileState, Found, FoundBy},
    todotxt::File,
};

const STALE_AFTER: Duration = Duration::from_hours(4);

/// Today is 2026-09-29 and now is 09:00 UTC.
pub(super) fn clock() -> Clock {
    let now = "2026-09-29T09:00Z".parse().expect("instant");
    Clock::new(&|_| None, now, TimeZone::UTC).expect("clock")
}

pub(super) fn params() -> Params {
    Params {
        clock: clock(),
        stale_after: STALE_AFTER,
    }
}

pub(super) fn found(dir: impl AsRef<Path>) -> Found {
    let dir = dir.as_ref();
    Found {
        todo: dir.join("todo.txt"),
        done: dir.join("done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    }
}

/// A file of `lines`, each ending in a newline.
fn file(lines: &[&str]) -> File {
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    File::parse(text.as_bytes())
}

/// A snapshot of `todo` and `done` lines in the folder `/d`.
pub(crate) fn snapshot(todo: &[&str], done: &[&str]) -> Snapshot {
    Snapshot::build(found("/d"), file(todo), file(done), &params())
}

/// A snapshot whose todo file is exactly `todo`, with an empty done file.
pub(crate) fn snapshot_of(todo: &[u8]) -> Snapshot {
    Snapshot::build(found("/d"), File::parse(todo), file(&[]), &params())
}

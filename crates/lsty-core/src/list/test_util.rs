use std::path::PathBuf;
use std::time::Duration;

use jiff::tz::TimeZone;

use super::{Params, Snapshot};
use crate::dates::Clock;
use crate::discover::{FileState, Found, FoundBy};
use crate::todotxt::File;

pub(super) const STALE_AFTER: Duration = Duration::from_hours(4);

/// Today is 2026-09-29 and now is 09:00 UTC.
pub(super) fn clock() -> Clock {
    let var =
        |name: &str| (name == "LSTY_NOW").then(|| "2026-09-29T0900Z".into());
    Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, TimeZone::UTC)
        .expect("clock")
}

pub(super) fn params() -> Params {
    Params {
        clock: clock(),
        stale_after: STALE_AFTER,
    }
}

pub(super) fn found(dir: &str) -> Found {
    Found {
        todo: PathBuf::from(dir).join("todo.txt"),
        done: PathBuf::from(dir).join("done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    }
}

/// A file of `lines`, each ending in a newline.
pub(super) fn file(lines: &[&str]) -> File {
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    File::parse(text.as_bytes())
}

pub(super) fn snapshot(todo: &[&str], done: &[&str]) -> Snapshot {
    Snapshot::build(found("/d"), file(todo), file(done), &params())
}

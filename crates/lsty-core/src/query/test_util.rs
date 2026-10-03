use std::path::PathBuf;
use std::time::Duration;

use jiff::tz::TimeZone;

use super::{Context, Query};
use crate::dates::{Clock, Date};
use crate::discover::{FileState, Found, FoundBy};
use crate::list::{Params, Snapshot};
use crate::todotxt::File;

pub(super) fn utc() -> TimeZone {
    TimeZone::UTC
}

/// Today is 2026-09-29 and now is 09:00 UTC.
fn clock() -> Clock {
    let var = |name: &str| {
        (name == "LSTY_NOW").then(|| String::from("2026-09-29T0900Z"))
    };
    Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, utc()).expect("clock")
}

fn file(lines: &[&str]) -> File {
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    File::parse(text.as_bytes())
}

pub(super) fn snapshot(todo: &[&str], done: &[&str]) -> Snapshot {
    build(file(todo), file(done))
}

fn build(todo: File, done: File) -> Snapshot {
    let found = Found {
        todo: PathBuf::from("/d/todo.txt"),
        done: PathBuf::from("/d/done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    };
    let params = Params {
        clock: clock(),
        stale_after: Duration::from_hours(4),
    };
    Snapshot::build(found, todo, done, &params)
}

pub(super) fn date(y: u16, m: u8, d: u8) -> Date {
    Date::new(y, m, d).expect("date")
}

pub(super) fn parse(text: &str) -> Query {
    Query::parse(text).expect("parse")
}

/// Whether `query` matches the one item `line`, on 2026-09-29 in UTC.
pub(super) fn hits(query: &Query, line: &str) -> bool {
    hits_on(query, line, date(2026, 9, 29), &utc())
}

pub(super) fn hits_on(
    query: &Query,
    line: &str,
    today: Date,
    zone: &TimeZone,
) -> bool {
    let snap = snapshot(&[line], &[]);
    let item = snap.items().first().expect("item");
    query.matches(&snap, item, &Context { today, zone })
}

/// Whether `query` matches a one-line todo file holding exactly `bytes`.
pub(super) fn hits_bytes(query: &Query, bytes: &[u8]) -> bool {
    let snap = build(File::parse(bytes), file(&[]));
    let item = snap.items().first().expect("item");
    let zone = utc();
    let today = date(2026, 9, 29);
    query.matches(&snap, item, &Context { today, zone: &zone })
}

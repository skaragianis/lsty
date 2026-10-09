//! Any text either parses or is refused as an invalid `filter` value, and
//! a parsed query can be evaluated.

#![no_main]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use jiff::tz::TimeZone;
use libfuzzer_sys::fuzz_target;
use lsty_core::dates::{Clock, Date};
use lsty_core::discover::{FileState, Found, FoundBy};
use lsty_core::errs::Code;
use lsty_core::list::{Params, Snapshot};
use lsty_core::query::{Context, Query};
use lsty_core::todotxt::File;

const LINE: &[u8] =
    b"(A) 2026-09-22 Call +work @phone due:2026-10-02 started:2026-09-29T0800Z\n";

fn build() -> Snapshot {
    let var = |name: &str| {
        (name == "LSTY_NOW").then(|| OsString::from("2026-09-29T0900Z"))
    };
    let Ok(clock) =
        Clock::new(&var, jiff::Timestamp::UNIX_EPOCH, TimeZone::UTC)
    else {
        panic!("the fixed clock variable is refused");
    };
    let found = Found {
        todo: PathBuf::from("/d/todo.txt"),
        done: PathBuf::from("/d/done.txt"),
        found_by: FoundBy::File,
        state: FileState::Exists,
    };
    let params = Params {
        clock,
        stale_after: Duration::from_hours(4),
    };
    Snapshot::build(found, File::parse(LINE), File::parse(b""), &params)
}

fn matching(q: &Query, snap: &Snapshot, ctx: &Context<'_>) -> Vec<bool> {
    snap.items()
        .iter()
        .map(|i| q.matches(snap, i, ctx))
        .collect()
}

/// Text without a quote reads the same as its words given as arguments.
fn check_args(text: &str, q: &Query, snap: &Snapshot, ctx: &Context<'_>) {
    if text.contains('"') {
        return;
    }
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    let Ok(from_args) = Query::parse_args(&words) else {
        panic!("parse accepted {text:?} but its arguments are refused");
    };
    assert_eq!(matching(q, snap, ctx), matching(&from_args, snap, ctx));
}

fn check(text: &str, snap: &Snapshot) {
    let zone = TimeZone::UTC;
    let Some(today) = Date::new(2026, 9, 29) else {
        return;
    };
    let ctx = Context { today, zone: &zone };
    match Query::parse(text) {
        Ok(q) => {
            matching(&q, snap, &ctx);
            q.overrides();
            check_args(text, &q, snap, &ctx);
        }
        Err(e) => {
            assert_eq!(e.code(), Code::InvalidValue);
            assert_eq!(e.text("field"), Some("filter"));
        }
    }
}

fn snapshot() -> &'static Snapshot {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(build)
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    check(text, snapshot());
    drop(Query::parse_args(&[text]));
});
